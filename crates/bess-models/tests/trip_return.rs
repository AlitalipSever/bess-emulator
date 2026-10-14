//! Phase 3's acceptance: a protection trip on a busy afternoon, and the
//! staggered way back.
//!
//! GW-01 at full discharge on the replayed July afternoon. The trip takes
//! the POI to zero in one tick and the house load onto the station-service
//! meter; the site reset closes the breaker and the blocks come back one
//! every 30 s, each through its own transition, while the availability
//! points and the site word tell the story. Nothing after the trip and the
//! reset is scripted.

use bess_core::alarms::layout::site as s;
use bess_core::alarms::{AlarmNode, ResetScope, Severity};
use bess_core::checkpoint;
use bess_core::kernel::Event;
use bess_core::state::{BreakerState, PcsOpState, SubstationState};
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 14:00 UTC, the warmest afternoon of the replayed year.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;
const BLOCKS: usize = 20;
const SPACING_TICKS: u64 = 30;

fn busy_plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, JULY_AFTERNOON_S);
    sim.set_external_setpoint_w(Some(100.0e6));
    sim
}

fn step(sim: &mut Simulation) -> Vec<Event> {
    let inputs = gw01_weather().inputs_at(sim.unix_time_s());
    sim.step(&inputs).to_vec()
}

fn raised(events: &[Event], node: AlarmNode, bit: u8) -> bool {
    events
        .iter()
        .any(|e| matches!(*e, Event::AlarmRaised { node: n, bit: b, .. } if n == node && b == bit))
}

fn cleared(events: &[Event], node: AlarmNode, bit: u8) -> bool {
    events
        .iter()
        .any(|e| matches!(*e, Event::AlarmCleared { node: n, bit: b, .. } if n == node && b == bit))
}

/// A plant at full discharge, tripped, and one tick past the trip.
struct Tripped {
    sim: Simulation,
    stored_start_wh: f64,
    /// The substation on the last tick before the trip.
    before: SubstationState,
    /// What the trip tick handed out.
    events: Vec<Event>,
}

fn trip_at_full_discharge() -> Tripped {
    let mut sim = busy_plant();
    let stored_start_wh = sim.stored_energy_wh();
    for _ in 0..120 {
        step(&mut sim);
    }
    let before = sim.state().substation.clone();
    assert!(
        before.poi_active_power_w > 95.0e6,
        "the plant was not at full discharge: {} W",
        before.poi_active_power_w
    );
    assert!(sim.trip_protection());
    assert!(!sim.trip_protection(), "a second trip changed something");
    let events = step(&mut sim);
    Tripped {
        sim,
        stored_start_wh,
        before,
        events,
    }
}

#[test]
fn a_trip_goes_dark_in_one_tick() {
    let Tripped {
        mut sim,
        before,
        events,
        ..
    } = trip_at_full_discharge();
    let st = sim.state();
    assert_eq!(
        events.first(),
        Some(&Event::AlarmRaised {
            node: AlarmNode::Site,
            bit: s::PROTECTION_TRIP,
            severity: Severity::Trip,
        }),
        "the trip is not first in its batch"
    );
    let to_fault = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::PcsStateChanged {
                    from: PcsOpState::Run,
                    to: PcsOpState::Fault,
                    ..
                }
            )
        })
        .count();
    assert_eq!(to_fault, BLOCKS);
    assert!(raised(&events, AlarmNode::Site, s::HV_BREAKER_OPEN));
    assert!(st.substation.poi_active_power_w.abs() < f64::EPSILON);
    assert!(st.substation.transformer_loss_w.abs() < f64::EPSILON);
    assert!(st.ems.available_discharge_w.abs() < f64::EPSILON);
    assert!(st.ems.available_charge_w.abs() < f64::EPSILON);
    assert!((st.substation.import_wh - before.import_wh).abs() < f64::EPSILON);
    assert!((st.substation.export_wh - before.export_wh).abs() < f64::EPSILON);
    assert!(st.substation.aux_power_w > 0.0, "the house load went dark");
    let one_tick_wh = st.substation.aux_power_w / 3600.0;
    let metered_wh = st.substation.station_service_wh - before.station_service_wh;
    assert!((metered_wh - one_tick_wh).abs() < 1.0e-9);

    // A minute dark: the POI meters stand, the station-service meter runs.
    for _ in 0..60 {
        step(&mut sim);
    }
    let dark = &sim.state().substation;
    assert!((dark.export_wh - before.export_wh).abs() < f64::EPSILON);
    assert!((dark.import_wh - before.import_wh).abs() < f64::EPSILON);
    assert!(dark.station_service_wh > 60.0 * one_tick_wh * 0.9);
}

/// What a watched return looked like, tick by tick from the reset.
struct Return {
    first_run: [Option<u64>; BLOCKS],
    limited_raised: Option<u64>,
    limited_cleared: Option<u64>,
    partial_cleared: Option<u64>,
    climbs: usize,
}

fn watch_return(sim: &mut Simulation, ticks: u64) -> Return {
    let station_wh = sim.state().substation.station_service_wh;
    let mut seen = Return {
        first_run: [None; BLOCKS],
        limited_raised: None,
        limited_cleared: None,
        partial_cleared: None,
        climbs: 0,
    };
    let mut last_poi = f64::NEG_INFINITY;
    for tick in 0..ticks {
        let events = step(sim);
        let st = sim.state();
        if tick == 0 {
            assert!(cleared(&events, AlarmNode::Site, s::PROTECTION_TRIP));
            assert!(cleared(&events, AlarmNode::Site, s::HV_BREAKER_OPEN));
        }
        for (k, b) in st.blocks.iter().enumerate() {
            if b.pcs.op_state == PcsOpState::Run && seen.first_run[k].is_none() {
                seen.first_run[k] = Some(tick);
            }
        }
        if raised(&events, AlarmNode::Site, s::POWER_LIMITED) {
            seen.limited_raised.get_or_insert(tick);
        }
        if cleared(&events, AlarmNode::Site, s::POWER_LIMITED) {
            seen.limited_cleared = Some(tick);
        }
        if cleared(&events, AlarmNode::Site, s::PARTIAL_AVAILABILITY) {
            seen.partial_cleared.get_or_insert(tick);
        }
        if st.substation.poi_active_power_w > last_poi + 1.0e6 {
            seen.climbs += 1;
        }
        last_poi = st.substation.poi_active_power_w;
        assert!(
            (st.substation.station_service_wh - station_wh).abs() < f64::EPSILON,
            "the station-service meter ran on a closed breaker"
        );
    }
    seen
}

#[test]
fn a_site_reset_brings_the_blocks_back_one_at_a_time() {
    let Tripped {
        mut sim,
        stored_start_wh,
        ..
    } = trip_at_full_discharge();
    for _ in 0..60 {
        step(&mut sim);
    }
    sim.reset_alarms(ResetScope::Site).expect("site scope");
    assert_eq!(sim.state().substation.hv_breaker, BreakerState::Closed);
    let seen = watch_return(&mut sim, BLOCKS as u64 * SPACING_TICKS + 60);

    let want: Vec<Option<u64>> = (0..BLOCKS as u64)
        .map(|k| Some(k * SPACING_TICKS))
        .collect();
    assert_eq!(
        seen.first_run.to_vec(),
        want,
        "blocks did not return 30 s apart"
    );
    assert_eq!(
        seen.climbs, BLOCKS,
        "the POI did not climb one step per block"
    );
    let raised_at = seen
        .limited_raised
        .expect("the return never reported a limited site");
    let cleared_at = seen
        .limited_cleared
        .expect("the site stayed limited after the return");
    assert!(
        raised_at < SPACING_TICKS,
        "limited raised late, at {raised_at}"
    );
    assert!(
        (19 * SPACING_TICKS..19 * SPACING_TICKS + 30).contains(&cleared_at),
        "limited cleared at {cleared_at}, not with the last block"
    );
    // Partly there for exactly as long as some block is still out: the
    // bit the trip raised clears with the last release, not with the reset.
    assert_eq!(
        seen.partial_cleared,
        Some((BLOCKS as u64 - 1) * SPACING_TICKS),
        "partial availability did not clear with the last block"
    );
    let st = sim.state();
    assert!(st.blocks.iter().all(|b| b.pcs.op_state == PcsOpState::Run));
    assert!(st.ems.available_discharge_w > 95.0e6);

    // Everything that came in, through the POI or the station-service
    // feed, is stored or lost, and the house load's two books agree.
    let e = &st.energy;
    let losses_wh = e.battery_loss_wh + e.pcs_loss_wh + e.transformer_loss_wh + e.aux_wh;
    let sub = &st.substation;
    let net_in_wh = sub.import_wh + sub.station_service_wh - sub.export_wh;
    let residual_wh = net_in_wh - (sim.stored_energy_wh() - stored_start_wh) - losses_wh;
    assert!(
        residual_wh.abs() / (sub.import_wh + sub.export_wh).max(1.0) < 2.0e-3,
        "energy balance residual {residual_wh:.1} Wh across a trip"
    );
    assert!(
        (e.aux_wh - e.aux_items.total_wh()).abs() <= 1.0e-9 * e.aux_wh,
        "the metered and itemized house load parted across the trip"
    );
}

/// A checkpoint taken halfway through the return resumes the same sequence
/// and the same log as a run that never stopped.
#[test]
fn a_checkpoint_mid_return_resumes_the_same_return() {
    let mut sim = busy_plant();
    for _ in 0..60 {
        step(&mut sim);
    }
    sim.trip_protection();
    for _ in 0..30 {
        step(&mut sim);
    }
    sim.reset_alarms(ResetScope::Site).expect("site scope");
    for _ in 0..200 {
        step(&mut sim);
    }
    let bytes = checkpoint::save(sim.state()).expect("save");
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut resumed = Simulation::from_state(cfg, models, checkpoint::load(&bytes).expect("load"));
    resumed.set_external_setpoint_w(Some(100.0e6));
    assert!(sim.state().ems.blocks_released > 1);
    assert!(sim.state().ems.blocks_released < BLOCKS);
    for _ in 0..400 {
        step(&mut sim);
        step(&mut resumed);
    }
    assert_eq!(sim.state(), resumed.state());
    assert_eq!(sim.state().ems.blocks_released, BLOCKS);
}
