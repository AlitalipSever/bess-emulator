//! The fiscal meter on the whole plant: quarter-hour periods over a
//! replayed day, agreement with the SCADA meters, a trip it sits through,
//! and a checkpoint that resumes the open period.

use bess_core::alarms::ResetScope;
use bess_core::checkpoint;
use bess_core::state::{BreakerState, RevenueMeter};
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-01-01 00:00 UTC.
const NEW_YEAR_S: i64 = 1_767_225_600;
/// 2026-07-14 14:00 UTC, the PR1 trip fixture's busy afternoon.
const JULY_AFTERNOON_S: i64 = NEW_YEAR_S + 194 * 86_400 + 14 * 3600;

fn plant(start_unix_s: i64, setpoint_w: Option<f64>) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, start_unix_s);
    sim.set_external_setpoint_w(setpoint_w);
    sim
}

fn step(sim: &mut Simulation) {
    let inputs = gw01_weather().inputs_at(sim.unix_time_s());
    sim.step(&inputs);
}

fn meter(sim: &Simulation) -> &RevenueMeter {
    &sim.state().substation.revenue
}

/// A replayed day under the plan closes 96 periods, and at every tick the
/// fiscal totals are the SCADA totals: same power, same arithmetic.
#[test]
fn a_replayed_day_closes_96_periods_and_agrees_with_scada() {
    let mut sim = plant(NEW_YEAR_S, None);
    for _ in 0..86_400 {
        step(&mut sim);
        let sub = &sim.state().substation;
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
        assert!(close(sub.revenue.import_wh, sub.import_wh), "import");
        assert!(close(sub.revenue.export_wh, sub.export_wh), "export");
    }
    let m = meter(&sim);
    assert_eq!(m.periods_closed, 96);
    assert_eq!(m.period_start_s, NEW_YEAR_S + 86_400);
    assert!(
        m.import_wh > 0.0 && m.export_wh > 0.0,
        "the plan charged and discharged"
    );
}

/// Through a protection trip the POI reads zero, so the fiscal totals do
/// not move by a single bit until the breaker closes again, while periods
/// keep closing on the clock.
#[test]
fn the_fiscal_meter_stands_still_while_the_breaker_is_open() {
    let mut sim = plant(JULY_AFTERNOON_S, Some(100.0e6));
    for _ in 0..120 {
        step(&mut sim);
    }
    assert!(sim.trip_protection());
    step(&mut sim);
    let frozen = meter(&sim).clone();
    for _ in 0..1800 {
        step(&mut sim);
        assert_eq!(sim.state().substation.hv_breaker, BreakerState::Open);
        assert_eq!(meter(&sim).import_wh.to_bits(), frozen.import_wh.to_bits());
        assert_eq!(meter(&sim).export_wh.to_bits(), frozen.export_wh.to_bits());
    }
    assert_eq!(meter(&sim).periods_closed, frozen.periods_closed + 2);
    assert_eq!(meter(&sim).last_export_wh, 0.0, "a quarter-hour of nothing");

    sim.reset_alarms(ResetScope::Site).expect("site scope");
    for _ in 0..900 {
        step(&mut sim);
    }
    assert!(
        meter(&sim).export_wh > frozen.export_wh,
        "the return exports again"
    );
}

/// A checkpoint taken mid-period resumes the same period: same start, same
/// partial energy, and the close lands on the same tick as the run that
/// never stopped.
#[test]
fn a_checkpoint_mid_period_closes_the_period_on_the_same_tick() {
    let mut sim = plant(NEW_YEAR_S + 3 * 3600, None);
    for _ in 0..(3600 + 400) {
        step(&mut sim);
    }
    let open_since = meter(&sim).period_start_s;
    assert_ne!(
        sim.unix_time_s(),
        open_since,
        "the checkpoint is mid-period"
    );
    let bytes = checkpoint::save(sim.state()).expect("save");
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let state = checkpoint::load(&bytes).expect("load");
    let mut resumed = Simulation::from_state(cfg, models, state);
    assert_eq!(meter(&resumed).period_start_s, open_since);
    let closed = meter(&sim).periods_closed;
    loop {
        step(&mut sim);
        step(&mut resumed);
        assert_eq!(meter(&sim), meter(&resumed));
        if meter(&sim).periods_closed > closed {
            break;
        }
    }
    assert_eq!(meter(&resumed).period_start_s, open_since + 900);
}
