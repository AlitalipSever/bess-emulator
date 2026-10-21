//! The plant at the grid's frequency (M4 phase 1): the FCR droop meets
//! the SOGL activation shape, the limited-frequency-sensitive modes cut
//! and raise at the standard's gradient without the follower's ramp, an
//! excursion outside the connection band trips the site, and inside the
//! band the grid-code block is inert.

use bess_core::alarms::layout::site as s;
use bess_core::alarms::{has_bit, ResetScope};
use bess_core::checkpoint::{self, state_digest};
use bess_core::kernel::{FcrDroop, GridCode, Inputs};
use bess_core::state::BreakerState;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather, SyntheticFrequency};

/// 2026-07-14 00:00 UTC: the warmest day.
const JULY_MIDNIGHT_S: i64 = 1_767_225_600 + 194 * 86_400;
const RATED_W: f64 = 100.0e6;

fn plant(start_unix_s: i64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 7, start_unix_s)
}

/// The driver's inputs with the frequency replaced.
fn at(sim: &Simulation, frequency_hz: f64) -> Inputs {
    Inputs {
        grid_frequency_hz: frequency_hz,
        ..gw01_weather().inputs_at(sim.unix_time_s())
    }
}

fn run_at(sim: &mut Simulation, frequency_hz: f64, ticks: u64) {
    for _ in 0..ticks {
        let inputs = at(sim, frequency_hz);
        sim.step(&inputs);
    }
}

fn poi_w(sim: &Simulation) -> f64 {
    sim.state().substation.poi_active_power_w
}

/// A −200 mHz step on an idle plant with 20 MW prequalified: at least
/// half the power at 15 s, all of it at 30 s, held after; back inside
/// the dead band it unwinds at the same rate.
#[test]
fn the_fcr_droop_meets_the_activation_shape_in_the_plant() {
    let mut sim = plant(JULY_MIDNIGHT_S + 3 * 3600);
    sim.set_external_setpoint_w(Some(0.0));
    run_at(&mut sim, 50.0, 60);
    assert!(poi_w(&sim).abs() < 1.0e6, "not idle: {}", poi_w(&sim));
    let idle_w = poi_w(&sim);
    sim.set_fcr_prequalified_w(20.0e6);
    run_at(&mut sim, 49.8, 15);
    let at_15 = poi_w(&sim) - idle_w;
    assert!(at_15 >= 0.5 * 20.0e6 * 0.9, "at 15 s: {at_15}");
    run_at(&mut sim, 49.8, 15);
    let at_30 = poi_w(&sim) - idle_w;
    assert!((at_30 - 20.0e6).abs() < 1.5e6, "at 30 s: {at_30}");
    assert_eq!(sim.state().ems.fcr_output_w, 20.0e6);
    run_at(&mut sim, 49.8, 60);
    assert!((poi_w(&sim) - idle_w - 20.0e6).abs() < 1.5e6);
    run_at(&mut sim, 50.0, 40);
    assert_eq!(sim.state().ems.fcr_output_w, 0.0);
    assert!((poi_w(&sim) - idle_w).abs() < 1.5e6);
    // The droop's state rides in the checkpoint.
    run_at(&mut sim, 49.9, 10);
    let bytes = checkpoint::save(sim.state()).expect("save");
    assert_eq!(checkpoint::load(&bytes).expect("load"), *sim.state());
}

/// At 50.5 Hz a plant discharging at its rating delivers 12 % less, from
/// the tick after the excursion (no ramp); at 49.7 Hz a plant charging at
/// its rating absorbs 8 % less; the `lfsm_active` bit mirrors both.
#[test]
fn lfsm_cuts_and_raises_at_the_gradient_without_a_ramp() {
    let mut sim = plant(JULY_MIDNIGHT_S + 18 * 3600);
    sim.set_external_setpoint_w(Some(RATED_W));
    run_at(&mut sim, 50.0, 720);
    let full_w = poi_w(&sim);
    assert!(full_w > 0.95 * RATED_W, "not at rating: {full_w}");
    assert!(!has_bit(sim.state().alarm_bits, s::LFSM_ACTIVE));
    run_at(&mut sim, 50.5, 3);
    let cut_w = full_w - poi_w(&sim);
    assert!(
        (cut_w - 0.12 * RATED_W).abs() < 0.02 * RATED_W,
        "cut {cut_w}"
    );
    assert!(has_bit(sim.state().alarm_bits, s::LFSM_ACTIVE));
    assert!((sim.state().ems.lfsm_cut_w + 0.12 * RATED_W).abs() < 1.0);
    run_at(&mut sim, 50.0, 3);
    assert!((poi_w(&sim) - full_w).abs() < 0.02 * RATED_W);
    assert!(!has_bit(sim.state().alarm_bits, s::LFSM_ACTIVE));

    let mut sim = plant(JULY_MIDNIGHT_S + 3 * 3600);
    sim.set_external_setpoint_w(Some(-RATED_W));
    run_at(&mut sim, 50.0, 720);
    let full_w = poi_w(&sim);
    assert!(full_w < -0.9 * RATED_W, "not charging at rating: {full_w}");
    run_at(&mut sim, 49.7, 3);
    let raised_w = poi_w(&sim) - full_w;
    assert!(
        (raised_w - 0.04 * RATED_W).abs() < 0.015 * RATED_W,
        "raised {raised_w}"
    );
}

/// Outside 47.5 to 51.5 Hz the kernel trips the site; a reset while the
/// grid is still out trips it again; once the grid is back the reset
/// holds and the return runs.
#[test]
fn an_excursion_outside_the_band_trips_the_site() {
    let mut sim = plant(JULY_MIDNIGHT_S + 18 * 3600);
    run_at(&mut sim, 50.0, 30);
    run_at(&mut sim, 51.6, 1);
    let st = sim.state();
    assert!(has_bit(st.alarm_bits, s::GRID_FREQUENCY_TRIP));
    assert_eq!(st.substation.hv_breaker, BreakerState::Open);
    // The trip fired at the end of the tick; from the next one the open
    // breaker leaves nothing to cut.
    run_at(&mut sim, 51.6, 1);
    assert_eq!(
        sim.state().ems.lfsm_cut_w,
        0.0,
        "nothing to cut on an open breaker"
    );
    sim.reset_alarms(ResetScope::Site).expect("site");
    run_at(&mut sim, 51.6, 2);
    assert!(has_bit(sim.state().alarm_bits, s::GRID_FREQUENCY_TRIP));
    run_at(&mut sim, 50.0, 1);
    sim.reset_alarms(ResetScope::Site).expect("site");
    run_at(&mut sim, 50.0, 30);
    let st = sim.state();
    assert!(!has_bit(st.alarm_bits, s::GRID_FREQUENCY_TRIP));
    assert_eq!(st.substation.hv_breaker, BreakerState::Closed);
    assert!(st.ems.blocks_released >= 1);
    run_at(&mut sim, 47.4, 1);
    assert!(has_bit(sim.state().alarm_bits, s::GRID_FREQUENCY_TRIP));
}

/// Inside the band and with the droop disarmed the grid-code block does
/// nothing: a day on the synthetic series leaves the same tree whether
/// the characteristics are the defaults or pushed out of reach, and the
/// flat series with the defaults is the same plant as the flat series
/// without any grid code at all.
#[test]
fn inside_the_band_the_grid_code_block_is_inert() {
    let flat = SyntheticFrequency::flat();
    let day = |gridcode: Option<GridCode>| {
        let mut sim = plant(JULY_MIDNIGHT_S);
        if let Some(gc) = gridcode {
            sim.set_grid_code(gc, FcrDroop::default());
        }
        for _ in 0..6 * 3600 {
            let inputs = Inputs {
                grid_frequency_hz: flat.frequency_hz(sim.unix_time_s()),
                ..gw01_weather().inputs_at(sim.unix_time_s())
            };
            sim.step(&inputs);
        }
        state_digest(sim.state())
    };
    let out_of_reach = GridCode {
        lfsm_o_hz: 60.0,
        lfsm_u_hz: 40.0,
        disconnect_over_hz: 70.0,
        disconnect_under_hz: 30.0,
        ..GridCode::default()
    };
    assert_eq!(day(None), day(Some(out_of_reach)));
    // And on the synthetic series itself: the spread never reaches the
    // thresholds, so the tree only carries the frequency it read.
    let mut sim = plant(JULY_MIDNIGHT_S);
    let weather = gw01_weather();
    let mut lfsm_ticks = 0u64;
    for _ in 0..6 * 3600 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        lfsm_ticks += u64::from(sim.state().ems.lfsm_cut_w != 0.0);
    }
    assert_eq!(lfsm_ticks, 0);
    assert!((sim.state().substation.frequency_hz - 50.0).abs() < 0.15);
}
