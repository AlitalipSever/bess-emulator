//! The converter follows a setpoint the way one does: an external step
//! over the control surface is reached after the dead time and the ramp,
//! without the block or the site calling it a miss on the way.

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::{block as b, site as s};
use bess_core::kernel::SetpointDynamics;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

const START_UNIX_S: i64 = 1_767_225_600;

fn sim() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 42, START_UNIX_S)
}

fn site_ac_w(sim: &Simulation) -> f64 {
    sim.state().blocks.iter().map(|blk| blk.pcs.p_ac_w).sum()
}

/// A full discharge step from idle: the plant climbs at the published
/// gradient, never overshoots, is there inside eleven minutes, and neither
/// `setpoint_not_met` nor `power_limited` says a word while it climbs.
#[test]
fn a_step_is_reached_after_the_ramp_without_an_alarm() {
    let mut sim = sim();
    let weather = gw01_weather();
    let rated = sim.config().grid.site_rated_w;
    let gradient_w_per_s = 0.10 / 60.0 * rated;
    sim.set_external_setpoint_w(Some(rated));

    let mut last = 0.0;
    let mut reached_at = None;
    for tick in 0..720u64 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        let st = sim.state();
        let p = site_ac_w(&sim);
        assert!(
            p >= last - 1.0,
            "tick {tick}: delivery fell from {last} to {p}"
        );
        assert!(
            p - last <= gradient_w_per_s * 1.05,
            "tick {tick}: climbed {} W in a second, over the gradient",
            p - last
        );
        assert!(
            !has_bit(st.alarm_bits, s::POWER_LIMITED),
            "tick {tick}: the site called its own ramp a limitation"
        );
        for (k, blk) in st.blocks.iter().enumerate() {
            assert!(
                !has_bit(blk.alarm_bits, b::SETPOINT_NOT_MET),
                "tick {tick}: block {k} called its own ramp a miss"
            );
        }
        if reached_at.is_none() && p > 0.98 * rated {
            reached_at = Some(tick);
        }
        last = p;
    }
    let reached = reached_at.expect("the plant never reached its setpoint");
    assert!(
        (560..=620).contains(&reached),
        "reached 98 % at tick {reached}, expected around the ten-minute ramp"
    );
    // At the end every block's followed target is at its rating and the
    // delivery matches it to the efficiency's own slack.
    let st = sim.state();
    for blk in &st.blocks {
        assert!((blk.pcs.setpoint_followed_w - sim.config().pcs_rated_w).abs() < 1.0);
    }
}

/// With the follower switched off the plant is at its setpoint on the
/// first tick, as it was before the follower existed.
#[test]
fn switched_off_the_plant_is_at_its_setpoint_at_once() {
    let mut sim = sim();
    sim.set_setpoint_dynamics(SetpointDynamics::off());
    let weather = gw01_weather();
    let rated = sim.config().grid.site_rated_w;
    sim.set_external_setpoint_w(Some(0.5 * rated));
    sim.step(&weather.inputs_at(sim.unix_time_s()));
    let p = site_ac_w(&sim);
    assert!(
        p > 0.45 * rated,
        "{p} W after one tick with the follower off"
    );
}

/// A checkpoint taken mid-ramp resumes on the same ramp: the resumed run
/// and the uninterrupted one deliver the same power tick for tick.
#[test]
fn a_resumed_run_continues_the_ramp() {
    let mut a = sim();
    let weather = gw01_weather();
    let rated = a.config().grid.site_rated_w;
    a.set_external_setpoint_w(Some(rated));
    for _ in 0..200 {
        a.step(&weather.inputs_at(a.unix_time_s()));
    }
    let bytes = bess_core::checkpoint::save(a.state()).expect("save");
    let cfg = PlantConfig::gw01();
    let mut b = Simulation::from_state(
        cfg.clone(),
        gw01_models(&cfg),
        bess_core::checkpoint::load(&bytes).expect("load"),
    );
    b.set_external_setpoint_w(Some(rated));
    for _ in 0..100 {
        a.step(&weather.inputs_at(a.unix_time_s()));
        b.step(&weather.inputs_at(b.unix_time_s()));
        assert_eq!(site_ac_w(&a).to_bits(), site_ac_w(&b).to_bits());
    }
    assert!(site_ac_w(&a) > 0.45 * rated && site_ac_w(&a) < 0.55 * rated);
}
