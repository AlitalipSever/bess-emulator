//! The main transformer in the plant: its temperatures follow the day's
//! load, its tap follows the grid's swing, its hot spot never sits below
//! its top oil, and the kernel trips the site at the emergency limit.

use bess_core::alarms::layout::site as s;
use bess_core::alarms::{has_bit, ResetScope};
use bess_core::checkpoint;
use bess_core::state::{BreakerState, PcsOpState};
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 00:00 UTC: the warmest day.
const JULY_MIDNIGHT_S: i64 = 1_767_225_600 + 194 * 86_400;

fn plant(start_unix_s: i64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 7, start_unix_s)
}

fn run(sim: &mut Simulation, ticks: u64) {
    let weather = gw01_weather();
    for _ in 0..ticks {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
}

/// A day on the plan: the top oil and the hot spot swing with the
/// charge and the discharge, the hot spot stays above the top oil and
/// under the normal-cyclic limit, the tap stays in range and the MV bus
/// inside the regulator's band most of the day.
#[test]
fn a_summer_day_swings_the_hot_spot_under_the_limit() {
    let mut sim = plant(JULY_MIDNIGHT_S);
    let weather = gw01_weather();
    let (mut hs_min, mut hs_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut inside_band = 0u64;
    let mut tap_moves = 0u64;
    let mut last_tap = sim.state().substation.tap_position;
    for _ in 0..86_400u64 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        let sub = &sim.state().substation;
        assert!(sub.hot_spot_c >= sub.top_oil_c - 1.0e-9);
        assert!(sub.tap_position.abs() <= 8);
        hs_min = hs_min.min(sub.hot_spot_c);
        hs_max = hs_max.max(sub.hot_spot_c);
        let mv_pu = sub.mv_bus_voltage_kv / sim.config().grid.mv_nominal_kv;
        if (mv_pu - 1.0).abs() <= 0.015 {
            inside_band += 1;
        }
        if sub.tap_position != last_tap {
            tap_moves += 1;
            last_tap = sub.tap_position;
        }
    }
    assert!(hs_max - hs_min > 10.0, "no daily swing: {hs_min}..{hs_max}");
    assert!(
        hs_max < 120.0,
        "the hot spot reached {hs_max} °C on the plan"
    );
    assert!(!has_bit(
        sim.state().alarm_bits,
        s::TRANSFORMER_OVER_TEMPERATURE
    ));
    assert!(tap_moves >= 2, "the tap never followed the grid's swing");
    assert!(
        inside_band > 80_000,
        "the MV bus sat outside the band {} s",
        86_400 - inside_band
    );
}

/// A hot spot pushed to the emergency limit trips the site from inside
/// the tick: the latched bit, the breaker open, every converter in fault,
/// and a site reset brings it back through the closing sequence.
#[test]
fn the_emergency_limit_trips_the_site_and_the_reset_returns_it() {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut state = bess_core::SiteState::new(&cfg, 7, JULY_MIDNIGHT_S);
    state.substation.top_oil_c = 130.0;
    state.substation.hot_spot_rise_1_k = 24.0;
    let mut sim = Simulation::from_state(cfg, models, state);
    run(&mut sim, 1);
    let st = sim.state();
    assert!(st.substation.hot_spot_c >= 140.0);
    assert!(has_bit(st.alarm_bits, s::TRANSFORMER_TRIP));
    assert!(has_bit(st.alarm_bits, s::TRANSFORMER_OVER_TEMPERATURE));
    assert!(has_bit(st.alarm_bits, s::HV_BREAKER_OPEN));
    assert_eq!(st.substation.hv_breaker, BreakerState::Open);
    assert!(st
        .blocks
        .iter()
        .all(|b| b.pcs.op_state == PcsOpState::Fault));
    // The trip fires at the end of the tick; the de-energized bus reads
    // zero from the next one.
    run(&mut sim, 1);
    assert_eq!(sim.state().substation.mv_bus_voltage_kv, 0.0);
    // Still hot: a reset clears the latch, the breaker closes, and the
    // next tick trips it again, as it should.
    sim.reset_alarms(ResetScope::Site).expect("site");
    run(&mut sim, 2);
    assert!(has_bit(sim.state().alarm_bits, s::TRANSFORMER_TRIP));
    // Cooled (de-energized for hours), the reset holds.
    run(&mut sim, 6 * 3600);
    assert!(sim.state().substation.hot_spot_c < 110.0);
    sim.reset_alarms(ResetScope::Site).expect("site");
    run(&mut sim, 30);
    let st = sim.state();
    assert!(!has_bit(st.alarm_bits, s::TRANSFORMER_TRIP));
    assert_eq!(st.substation.hv_breaker, BreakerState::Closed);
    assert!(st.blocks[0].pcs.op_state.is_connected());
}

/// The temperatures and the tap ride in the checkpoint.
#[test]
fn the_transformer_state_survives_a_checkpoint() {
    let mut sim = plant(JULY_MIDNIGHT_S);
    run(&mut sim, 5 * 3600);
    let bytes = checkpoint::save(sim.state()).expect("save");
    let loaded = checkpoint::load(&bytes).expect("load");
    assert_eq!(loaded, *sim.state());
    assert!(loaded.substation.top_oil_c > loaded.weather.ambient_c);
}
