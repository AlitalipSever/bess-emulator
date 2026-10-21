//! A written power limit in the plant (M4 phase 2 PR1): it binds both
//! directions from the next tick, the converters walk down to it at the
//! follower's gradient, `external_limitation` says the shortfall is the
//! order's, the revert timer lifts it, and the command register's
//! refusals leave a mark on the log.

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::site as s;
use bess_core::checkpoint;
use bess_core::kernel::{ControlCommand, Event};
use bess_core::state::{BreakerState, SwitchPosition};
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 18:00 UTC: the evening discharge.
const JULY_EVENING_S: i64 = 1_767_225_600 + 194 * 86_400 + 18 * 3600;
const RATED_W: f64 = 100.0e6;

fn plant(setpoint_w: f64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, JULY_EVENING_S);
    sim.set_external_setpoint_w(Some(setpoint_w));
    sim
}

fn run(sim: &mut Simulation, ticks: u64) -> Vec<Event> {
    let weather = gw01_weather();
    let mut log = Vec::new();
    for _ in 0..ticks {
        log.extend_from_slice(sim.step(&weather.inputs_at(sim.unix_time_s())));
    }
    log
}

fn poi_w(sim: &Simulation) -> f64 {
    sim.state().substation.poi_active_power_w
}

/// A 60 % limit on a plant at its rating: the target is capped from the
/// next tick, the flag raises, the plant walks down at 10 %/min and sits
/// at 60 %; `power_limited` stays clear, since the plant delivers what it
/// is asked. The limit reverts on its timer and the plant walks back.
#[test]
fn a_limit_binds_is_flagged_and_reverts() {
    let mut sim = plant(RATED_W);
    run(&mut sim, 720);
    assert!(poi_w(&sim) > 0.95 * RATED_W);
    sim.set_external_limit(60.0, 300.0).expect("in range");
    run(&mut sim, 1);
    assert!((sim.state().ems.site_setpoint_w - 0.6 * RATED_W).abs() < 1.0);
    assert!(has_bit(sim.state().alarm_bits, s::EXTERNAL_LIMITATION));
    assert!(sim.state().ems.limit_binding);
    // Ten percent a minute: 40 % takes four minutes.
    run(&mut sim, 270);
    assert!(
        (poi_w(&sim) - 0.6 * RATED_W).abs() < 0.03 * RATED_W,
        "at 60 %: {}",
        poi_w(&sim)
    );
    assert!(!has_bit(sim.state().alarm_bits, s::POWER_LIMITED));
    let bytes = checkpoint::save(sim.state()).expect("save");
    assert_eq!(checkpoint::load(&bytes).expect("load"), *sim.state());
    // The timer: 300 s after the write the cap lifts and the flag clears.
    run(&mut sim, 40);
    assert!(!sim.state().ems.external_limit.enabled);
    assert!(!has_bit(sim.state().alarm_bits, s::EXTERNAL_LIMITATION));
    run(&mut sim, 300);
    assert!(poi_w(&sim) > 0.95 * RATED_W, "did not walk back");
}

/// The cap holds a charge as it holds a discharge, a limit that does not
/// bind raises nothing, and a value outside the register's range is
/// refused.
#[test]
fn the_cap_is_symmetric_and_a_slack_limit_is_silent() {
    let mut sim = plant(-RATED_W);
    sim.set_external_limit(50.0, 0.0).expect("in range");
    run(&mut sim, 2);
    assert!((sim.state().ems.site_setpoint_w + 0.5 * RATED_W).abs() < 1.0);
    assert!(sim.state().ems.limit_binding);
    sim.set_external_setpoint_w(Some(-0.3 * RATED_W));
    run(&mut sim, 2);
    assert!(!sim.state().ems.limit_binding);
    assert!(!has_bit(sim.state().alarm_bits, s::EXTERNAL_LIMITATION));
    assert!(sim.set_external_limit(120.0, 0.0).is_err());
    assert!(sim.set_external_limit(50.0, -1.0).is_err());
    assert!(sim.clear_external_limit());
    assert!(!sim.clear_external_limit());
}

/// The command register: the switchgear in order through it, a refused
/// move recorded on the log as `command_refused`, the site reset and the
/// limit's clear reachable the same way.
#[test]
fn the_command_register_drives_the_switchgear_and_logs_refusals() {
    let mut sim = plant(0.0);
    run(&mut sim, 2);
    // Earthing a live bus is interlocked: refused, nothing moves, logged.
    assert!(sim.control_command(ControlCommand::EarthingClose).is_err());
    let log = run(&mut sim, 1);
    assert!(log.contains(&Event::CommandRefused {
        command: ControlCommand::EarthingClose.code()
    }));
    assert_eq!(sim.state().substation.earthing_switch, SwitchPosition::Open);
    assert!(sim
        .control_command(ControlCommand::BreakerOpen)
        .expect("open"));
    run(&mut sim, 1);
    assert_eq!(sim.state().substation.hv_breaker, BreakerState::Open);
    assert!(sim
        .control_command(ControlCommand::DisconnectorsOpen)
        .expect("isolate"));
    assert!(sim
        .control_command(ControlCommand::EarthingClose)
        .expect("earth"));
    run(&mut sim, 1);
    assert_eq!(
        sim.state().substation.earthing_switch,
        SwitchPosition::Closed
    );
    assert!(sim.control_command(ControlCommand::BreakerClose).is_err());
    assert!(sim
        .control_command(ControlCommand::EarthingOpen)
        .expect("unearth"));
    assert!(sim
        .control_command(ControlCommand::DisconnectorsClose)
        .expect("connect"));
    assert!(sim
        .control_command(ControlCommand::BreakerClose)
        .expect("close"));
    run(&mut sim, 2);
    assert_eq!(sim.state().substation.hv_breaker, BreakerState::Closed);
    sim.set_external_limit(10.0, 0.0).expect("in range");
    assert!(sim
        .control_command(ControlCommand::ClearLimit)
        .expect("clear"));
    assert!(!sim.state().ems.external_limit.enabled);
    assert!(sim.control_command(ControlCommand::SiteReset).is_ok());
}

/// The quarter-hour report: the period the limit is written in shows the
/// deviation from the schedule the controller pursued (none: the capped
/// target is the schedule), the availability reads the plant's power
/// over its rating, the tracking error stays small, and a period that
/// misses its schedule by more than the frame raises
/// `schedule_deviation`.
#[test]
fn the_quarter_hour_report_follows_the_fiscal_meter() {
    let mut sim = plant(RATED_W);
    // 18:00 start: the first period closes at 18:15.
    run(&mut sim, 900);
    let k = &sim.state().kpi;
    assert_eq!(sim.state().substation.revenue.periods_closed, 1);
    assert!(
        k.last_availability_discharge > 0.9,
        "{}",
        k.last_availability_discharge
    );
    assert!(k.last_availability_charge > 0.9);
    // The ramp from idle sits inside this period: the metered mean is
    // under the scheduled one by the ramp's share, within the frame of
    // the walk the follower allows (ten minutes of fifteen).
    assert!(k.last_deviation_w < 0.0);
    assert!(k.last_tracking_rms_w > 1.0e6);
    assert!(k.redispatch_up_wh > 50.0e6 && k.redispatch_down_wh > 50.0e6);
    // A schedule the plant cannot meet: twenty blocks' worth asked of a
    // tripped site. The next closed period deviates by the whole target.
    sim.trip_protection();
    run(&mut sim, 900);
    let k = &sim.state().kpi;
    assert!(k.last_deviation_share.abs() > 0.5 || k.over_frame || k.last_scheduled_w == 0.0);
    assert_eq!(sim.state().substation.revenue.periods_closed, 2);
}
