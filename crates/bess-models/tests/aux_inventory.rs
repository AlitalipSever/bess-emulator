//! The waterfall identity: every watt the plant consumes has an address.
//!
//! The M1 gate is not only a number in a band, it is the claim that the
//! nameplate-to-field gap can be read item by item. That claim is only worth
//! as much as its accounting, so these tests check the two ways the
//! accounting can quietly go wrong: an item that is metered but not
//! attributed, and an item that is attributed twice.

use bess_core::state::PcsOpState;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-01-01 00:00:00 UTC.
const NEW_YEAR_S: i64 = 1_767_225_600;
/// 2026-07-14, the warmest stretch of the replayed year.
const JULY_S: i64 = NEW_YEAR_S + 194 * 86_400;

fn simulation(setpoint_w: Option<f64>) -> Simulation {
    started_at(NEW_YEAR_S, setpoint_w)
}

fn started_at(start_unix_s: i64, setpoint_w: Option<f64>) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, start_unix_s);
    sim.set_external_setpoint_w(setpoint_w);
    sim
}

fn run(sim: &mut Simulation, ticks: u64) {
    let weather = gw01_weather();
    for _ in 0..ticks {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
    }
}

/// The identity itself. The metered total comes off the substation, the
/// items come off the consumers that reported them, and the two paths never
/// meet inside the code. If a category is ever added to one and forgotten in
/// the other, this is where it shows.
#[test]
fn the_itemized_aux_matches_the_metered_total() {
    let mut sim = simulation(None);
    run(&mut sim, 86_400);

    let energy = &sim.state().energy;
    let itemized_wh = energy.aux_items.total_wh();
    let residual_wh = energy.aux_wh - itemized_wh;
    let relative = residual_wh.abs() / energy.aux_wh.max(1.0);
    assert!(
        relative < 1.0e-12,
        "metered aux {:.3} Wh, itemized {itemized_wh:.3} Wh, residual {residual_wh:.6} Wh",
        energy.aux_wh
    );
    assert!(energy.aux_wh > 0.0, "the plant consumed nothing all day");
}

/// The whole waterfall, not just its auxiliary column: what crossed the POI
/// is either in the batteries or in one of the named loss items. This is the
/// site energy balance stated the way the study will publish it.
#[test]
fn every_watt_has_an_address() {
    let mut sim = simulation(None);
    let stored_start_wh = sim.stored_energy_wh();
    run(&mut sim, 86_400);

    let state = sim.state();
    let items = &state.energy.aux_items;
    let accounted_wh = state.energy.battery_loss_wh
        + state.energy.pcs_loss_wh
        + state.energy.transformer_loss_wh
        + items.hvac_wh
        + items.bms_wh
        + items.pcs_standby_wh
        + items.controls_wh
        + items.misc_wh;
    let delta_stored_wh = sim.stored_energy_wh() - stored_start_wh;
    let net_poi_wh = state.substation.import_wh - state.substation.export_wh;
    let residual_wh = net_poi_wh - delta_stored_wh - accounted_wh;
    let throughput_wh = state.substation.import_wh + state.substation.export_wh;
    assert!(
        residual_wh.abs() / throughput_wh.max(1.0) < 2.0e-3,
        "unaddressed energy {residual_wh:.1} Wh over {throughput_wh:.0} Wh of throughput"
    );
}

/// A plant doing nothing is not free, and before M1 the emulator could not
/// say what it was paying for. Now it can, item by item.
#[test]
fn an_idle_plant_still_pays_for_itself() {
    let mut sim = simulation(Some(0.0));
    run(&mut sim, 3_600);

    let state = sim.state();
    let items = &state.energy.aux_items;
    assert!(items.bms_wh > 0.0, "rack electronics drew nothing");
    assert!(items.controls_wh > 0.0, "controls drew nothing");
    assert!(items.misc_wh > 0.0, "lighting and small power drew nothing");
    assert!(
        items.pcs_standby_wh > 0.0,
        "every converter was idle, yet none paid its standby tare"
    );
    assert!(
        state.energy.pcs_loss_wh.abs() < 1.0,
        "an idle plant converted {:.1} Wh worth of losses",
        state.energy.pcs_loss_wh
    );
    assert!(
        state.substation.import_wh > 0.0,
        "the house load came from nowhere"
    );
}

/// The other failure mode: billing the same watts twice. A converting unit
/// supplies itself out of its conversion loss, which the efficiency curve
/// already carries, so the standby tare has to stop the moment the unit
/// starts.
#[test]
fn a_converting_block_stops_paying_the_standby_tare() {
    let mut sim = simulation(Some(0.0));
    run(&mut sim, 60);
    let idle_wh = sim.state().energy.aux_items.pcs_standby_wh;
    assert!(idle_wh > 0.0, "an idle hour of converters cost nothing");

    sim.set_external_setpoint_w(Some(-50.0e6));
    run(&mut sim, 60);

    let state = sim.state();
    assert!(
        state
            .blocks
            .iter()
            .all(|b| b.pcs.op_state == PcsOpState::Run),
        "the plant did not take the setpoint, so this proves nothing"
    );
    assert!(
        state.aux.pcs_standby_w.abs() < f64::EPSILON,
        "every converter is running, yet the tare still reads {:.1} W",
        state.aux.pcs_standby_w
    );
    assert!(
        (state.energy.aux_items.pcs_standby_wh - idle_wh).abs() < 1.0e-9,
        "the tare kept accumulating while the converters were running"
    );
}

/// CALIBRATION.md publishes how a summer day's auxiliary energy divides
/// between the five items. Same rule as the HVAC record: a figure CI cannot
/// falsify is a claim, not a measurement.
#[test]
fn the_published_item_split_still_holds() {
    let mut sim = started_at(JULY_S, None);
    run(&mut sim, 86_400);

    let items = &sim.state().energy.aux_items;
    let total = items.total_wh();
    let share = |wh: f64| wh / total * 100.0;
    let checks = [
        ("HVAC", share(items.hvac_wh), 60.0, 80.0, "71%"),
        ("rack electronics", share(items.bms_wh), 10.0, 22.0, "15%"),
        ("PCS standby", share(items.pcs_standby_wh), 1.0, 5.0, "2%"),
        ("controls", share(items.controls_wh), 4.0, 10.0, "7%"),
        ("lighting and misc", share(items.misc_wh), 3.0, 7.0, "4%"),
    ];
    for (name, measured, lo, hi, recorded) in checks {
        assert!(
            (lo..hi).contains(&measured),
            "{name} took {measured:.1}% of the July auxiliary energy, recorded as {recorded}"
        );
    }
}

/// Meters accumulate; they do not report a moment. A negative contribution
/// anywhere, in any tick, would fail the same accounting the waterfall is
/// built on.
#[test]
fn the_item_meters_never_run_backwards() {
    let mut sim = simulation(None);
    let weather = gw01_weather();
    let mut last = [0.0f64; 5];
    for tick in 0..7_200u64 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        let items = &sim.state().energy.aux_items;
        let now = [
            items.hvac_wh,
            items.bms_wh,
            items.pcs_standby_wh,
            items.controls_wh,
            items.misc_wh,
        ];
        for (idx, (now, last)) in now.iter().zip(last.iter()).enumerate() {
            assert!(now >= last, "aux item {idx} went backwards at tick {tick}");
        }
        last = now;
    }
}
