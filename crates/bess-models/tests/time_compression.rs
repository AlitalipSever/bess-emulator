//! M5 phase 2 gate: a run can skip days. A fast-forward of zero days is a
//! no-op; a fresh plant fast-forwarded over a ticked week's stress lands
//! on the ticked plant's aging states; a plant aged at commissioning has
//! the health the typical day implies with the spread of the racks' own
//! rates; the clock moves and the meters do not.

use bess_core::kernel::Event;
use bess_core::state::aging::StressDay;
use bess_core::{checkpoint, PlantConfig, Simulation, StressYear};
use bess_models::{gw01_models, gw01_weather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;
const DAY_TICKS: u64 = 86_400;

fn fresh() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 7, START_UNIX_S)
}

fn tick(sim: &mut Simulation, year: &mut StressYear, ticks: u64) {
    let weather = gw01_weather();
    for _ in 0..ticks {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
        year.observe(sim.state());
    }
}

#[test]
fn zero_days_is_a_no_op() {
    let mut sim = fresh();
    let before = checkpoint::state_digest(sim.state());
    let year = StressYear::repeated(sim.config().total_racks(), StressDay::gw01_typical());
    assert_eq!(sim.fast_forward(&year, 0), &[]);
    assert_eq!(checkpoint::state_digest(sim.state()), before);
}

/// Three ticked days, their stress replayed over a fresh plant: the same
/// aging states to the integration's last digit, the ledgers aside.
#[test]
fn replaying_a_ticked_stretch_lands_on_the_ticked_states() {
    let mut ticked = fresh();
    let mut year = StressYear::new(ticked.config().total_racks());
    tick(&mut ticked, &mut year, 3 * DAY_TICKS);
    assert_eq!(year.days(), 3);

    let mut replayed = fresh();
    let events = replayed.fast_forward(&year, 3).to_vec();
    assert_eq!(events, vec![Event::FastForwarded { days: 3 }]);
    assert_eq!(replayed.unix_time_s(), START_UNIX_S + 3 * 86_400);
    assert_eq!(replayed.state().event_log.count, 1);

    for (a, b) in ticked.state().racks().zip(replayed.state().racks()) {
        assert!((a.age.q_loss_cal - b.age.q_loss_cal).abs() < 1.0e-12);
        assert!((a.age.q_loss_cyc - b.age.q_loss_cyc).abs() < 1.0e-12);
        assert!((a.age.r_gain_cal - b.age.r_gain_cal).abs() < 1.0e-12);
        assert!((a.age.r_gain_cyc - b.age.r_gain_cyc).abs() < 1.0e-12);
        assert!((a.age.efc - b.age.efc).abs() < 1.0e-9);
        assert_eq!(b.age.skipped_days, 3.0);
        assert_eq!(a.age.skipped_days, 0.0);
        assert!((a.soh - b.soh).abs() < 1.0e-12);
    }
    // Nothing was metered on the skipped days.
    assert_eq!(replayed.state().substation.import_wh, 0.0);
    assert_eq!(replayed.state().substation.revenue.periods_closed, 0);
}

/// A five-year-old plant: health in the band the README derives, the
/// clock at the start, the racks spread by their rates, and the plant
/// still ticks.
#[test]
fn a_plant_aged_at_commissioning() {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new_aged(cfg, models, 7, START_UNIX_S, 5);
    assert_eq!(sim.unix_time_s(), START_UNIX_S);
    let state = sim.state();
    let n = state.racks().count() as f64;
    let mean = state.racks().map(|r| r.soh).sum::<f64>() / n;
    let min = state.racks().map(|r| r.soh).fold(f64::INFINITY, f64::min);
    let max = state
        .racks()
        .map(|r| r.soh)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(mean > 0.82 && mean < 0.93, "mean {mean}");
    assert!(max - min > 0.005, "spread {min} to {max}");
    for rack in state.racks() {
        assert!((rack.age.days - 1825.0).abs() < 1.0e-9);
        assert_eq!(rack.age.skipped_days, 1825.0);
        assert!((rack.age.efc - 1825.0 * 0.785).abs() < 1.0e-6);
    }
    assert!(state.energy.aging_wh > 0.0);
    assert_eq!(state.event_log.count, 1);

    let weather = gw01_weather();
    for _ in 0..600 {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
    }
    assert_eq!(sim.unix_time_s(), START_UNIX_S + 600);
    let fresh_plant = fresh();
    assert_eq!(
        Simulation::new_aged(
            PlantConfig::gw01(),
            gw01_models(&PlantConfig::gw01()),
            7,
            START_UNIX_S,
            0
        )
        .state(),
        fresh_plant.state(),
        "zero years is the fresh plant"
    );
}

/// The projection is the same arithmetic on both platforms: pinned here
/// so the determinism job holds it.
#[test]
fn a_projected_decade_is_deterministic() {
    let cfg = PlantConfig::gw01();
    let a = Simulation::new_aged(cfg.clone(), gw01_models(&cfg), 7, START_UNIX_S, 10);
    let b = Simulation::new_aged(cfg.clone(), gw01_models(&cfg), 7, START_UNIX_S, 10);
    assert_eq!(
        checkpoint::state_digest(a.state()),
        checkpoint::state_digest(b.state())
    );
}
