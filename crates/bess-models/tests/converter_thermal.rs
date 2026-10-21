//! The warm converter: the overload budget covers a lost block for its
//! published time and no longer, the node settles where the datasheet
//! puts it, and with the limits switched off the plant is phase 2's.

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::{block as b, site as s};
use bess_core::checkpoint;
use bess_core::{PlantConfig, Simulation};
use bess_models::pcs::SurfacePcs;
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 17:00 UTC: the warmest evening.
const JULY_EVENING_S: i64 = 1_767_225_600 + 194 * 86_400 + 17 * 3600;

fn plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, JULY_EVENING_S);
    sim.set_external_setpoint_w(Some(100.0e6));
    sim
}

fn run(sim: &mut Simulation, ticks: u64) {
    let weather = gw01_weather();
    for _ in 0..ticks {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
}

fn delivered_w(sim: &Simulation) -> f64 {
    sim.state().blocks.iter().map(|blk| blk.pcs.p_ac_w).sum()
}

/// Two blocks trip; the other eighteen run at 110 % on their budgets for
/// ten minutes, the site reports no limit, then every budget is spent,
/// the converters fall to their rating in one tick and the site reports
/// its limit. The blocks themselves report no miss: each delivers what
/// it follows, and what it follows is bounded by what it permits.
#[test]
fn the_overload_budget_covers_two_lost_blocks_for_ten_minutes() {
    let mut sim = plant();
    run(&mut sim, 720);
    assert!(delivered_w(&sim) > 98.0e6);
    assert_eq!(sim.trip_pcs(3), Ok(true));
    assert_eq!(sim.trip_pcs(11), Ok(true));
    // The remaining blocks ramp from 5.0 to 5.5 MW in a minute.
    run(&mut sim, 90);
    let st = sim.state();
    let p = delivered_w(&sim);
    assert!(p > 98.0e6 && p < 100.0e6, "{p} W on the overload");
    for (k, blk) in st.blocks.iter().enumerate() {
        if k == 3 || k == 11 {
            continue;
        }
        assert!(blk.pcs.p_ac_w > 5.4e6, "block {k} at {} W", blk.pcs.p_ac_w);
        assert!(blk.pcs.overload_budget < 1.0 && blk.pcs.overload_budget > 0.8);
        assert!(!has_bit(blk.alarm_bits, b::SETPOINT_NOT_MET));
    }
    assert!(!has_bit(st.alarm_bits, s::POWER_LIMITED));

    // The budgets run out around the tenth minute of overload.
    let mut spent_at = None;
    for tick in 90..900u64 {
        run(&mut sim, 1);
        if sim.state().blocks[0].pcs.overload_budget <= 0.0 {
            spent_at = Some(tick);
            break;
        }
    }
    let spent = spent_at.expect("the budget never ran out");
    assert!((560..=640).contains(&spent), "spent at {spent} s");
    run(&mut sim, 30);
    let st = sim.state();
    let p = delivered_w(&sim);
    assert!(p > 89.0e6 && p < 91.0e6, "{p} W after the budget");
    assert!(st.blocks[0].pcs.ac_limit_w <= sim.config().pcs_rated_w);
    assert!(!st.blocks[0].pcs.overload_armed);
    // A converter at its rating asked for more is delivering what it
    // follows, so the block does not call it a miss; the site, short of
    // its target with nothing left to ramp, calls it a limit.
    assert!(!has_bit(st.blocks[0].alarm_bits, b::SETPOINT_NOT_MET));
    assert!(has_bit(st.alarm_bits, s::POWER_LIMITED));
    // Not derated: the budget is the limit, not the heat.
    assert!(!has_bit(st.blocks[0].alarm_bits, b::PCS_DERATED));
    assert!(!has_bit(st.blocks[0].alarm_bits, b::PCS_OVER_TEMPERATURE));

    // Asked for less, the budget refills, and the overload arms again
    // only when it is full.
    sim.set_external_setpoint_w(Some(50.0e6));
    run(&mut sim, 900);
    let pcs = &sim.state().blocks[0].pcs;
    assert!(pcs.overload_budget > 0.4 && pcs.overload_budget < 0.6);
    assert!(!pcs.overload_armed);
    run(&mut sim, 1000);
    assert!(sim.state().blocks[0].pcs.overload_armed);
}

/// An hour into the evening discharge at full power the node sits at the
/// ambient plus a rise under the rated 20 K, far from the knee: the
/// replayed year never derates a converter.
#[test]
fn the_node_settles_under_the_knee_on_the_warmest_evening() {
    let mut sim = plant();
    run(&mut sim, 3600);
    let st = sim.state();
    let ambient = st.weather.ambient_c;
    for blk in &st.blocks {
        let t = blk.pcs.temperature_c;
        assert!(
            t > ambient + 5.0 && t < ambient + 20.0,
            "{t} °C at {ambient} °C"
        );
        assert!(!has_bit(blk.alarm_bits, b::PCS_DERATED));
        assert_eq!(blk.pcs.ac_limit_w, 1.1 * sim.config().pcs_rated_w);
    }
}

/// A converter made hot derates: the limit falls along the curve, the
/// warning precedes the derate, and a block that cannot deliver its share
/// says so.
#[test]
fn a_hot_converter_derates_and_says_so() {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut state = bess_core::SiteState::new(&cfg, 7, JULY_EVENING_S);
    state.blocks[2].pcs.temperature_c = 80.0;
    let mut sim = Simulation::from_state(cfg, models, state);
    sim.set_external_setpoint_w(Some(100.0e6));
    run(&mut sim, 1);
    let blk = &sim.state().blocks[2];
    assert!(has_bit(blk.alarm_bits, b::PCS_OVER_TEMPERATURE));
    assert!(has_bit(blk.alarm_bits, b::PCS_DERATED));
    assert!(blk.pcs.ac_limit_w < 0.6 * sim.config().pcs_rated_w);
    // It cools toward the air over the next hour and the bits clear in
    // order: derated first, then the warning.
    let mut derate_cleared = None;
    let mut warning_cleared = None;
    for tick in 0..7200u64 {
        run(&mut sim, 1);
        let blk = &sim.state().blocks[2];
        if derate_cleared.is_none() && !has_bit(blk.alarm_bits, b::PCS_DERATED) {
            derate_cleared = Some(tick);
        }
        if warning_cleared.is_none() && !has_bit(blk.alarm_bits, b::PCS_OVER_TEMPERATURE) {
            warning_cleared = Some(tick);
        }
    }
    let (d, w) = (
        derate_cleared.expect("derate"),
        warning_cleared.expect("warning"),
    );
    assert!(d < w, "derate cleared at {d}, warning at {w}");
}

/// With the curve flat and the budget never spending, the plant is the
/// phase 2 plant: the same trajectory, the overload simply unbounded.
#[test]
fn without_limits_the_overload_is_unbounded() {
    let cfg = PlantConfig::gw01();
    let mut models = gw01_models(&cfg);
    models.pcs = Box::new(SurfacePcs::without_thermal_limits(cfg.pcs_rated_w));
    let mut sim = Simulation::new(cfg, models, 7, JULY_EVENING_S);
    sim.set_external_setpoint_w(Some(100.0e6));
    run(&mut sim, 60);
    sim.trip_pcs(3).expect("block 3");
    sim.trip_pcs(11).expect("block 11");
    run(&mut sim, 1200);
    let st = sim.state();
    assert_eq!(st.blocks[0].pcs.overload_budget, 1.0);
    assert_eq!(st.blocks[0].pcs.ac_limit_w, sim.config().pcs_rated_w);
    // No overload at all: the limit is the rating.
    assert!(st.blocks[0].pcs.p_ac_w <= sim.config().pcs_rated_w + 1.0);
}

/// The node and the budget ride in the checkpoint.
#[test]
fn the_node_and_the_budget_survive_a_checkpoint() {
    let mut sim = plant();
    run(&mut sim, 720);
    sim.trip_pcs(3).expect("block 3");
    sim.trip_pcs(11).expect("block 11");
    run(&mut sim, 300);
    let bytes = checkpoint::save(sim.state()).expect("save");
    let loaded = checkpoint::load(&bytes).expect("load");
    assert_eq!(loaded, *sim.state());
    assert!(loaded.blocks[0].pcs.overload_budget < 1.0);
    assert!(loaded.blocks[0].pcs.temperature_c > loaded.weather.ambient_c);
}
