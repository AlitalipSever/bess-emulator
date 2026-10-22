//! The determinism contract over a scenario: one (seed, scenario, dataset)
//! is one run, byte for byte, with or without a checkpoint in the middle; a
//! checkpoint refuses to continue under another story; and a scenario with
//! nothing in it costs nothing.

#[path = "support/play.rs"]
mod support;

use bess_core::checkpoint;
use bess_core::kernel::Event;
use bess_core::{PlantConfig, Simulation};
use bess_models::gw01_models;
use bess_scenario::{load, Player, Resolved, ResumeError, RunParams};
use support::play;

/// An hour with one of nearly everything: a failure and its repair, a
/// drawn leak, a trip and its reset, maintenance, an isolation, and a data
/// fault that the kernel must not notice.
const BUSY_HOUR: &[u8] = br"
schema: 1
name: busy-hour
seed: 42
start: 2026-07-14T12:00:00Z
events:
  - at: PT0S
    target: block[2].container[0].hvac
    fault: failure
  - at: PT5M
    target: block[3].container[1].rack[4]
    fault: self_discharge
    rate_pct_per_month: [10, 30]
  - at: PT10M
    target: block[5].pcs
    fault: trip
  - at: PT12M
    surface: mqtt
    fault: dropout
    duration: PT5M
  - at: PT20M
    target: block[5]
    fault: alarm_reset
  - at: PT30M
    target: block[7]
    fault: maintenance_enter
  - at: PT40M
    target: block[1].container[0].rack[0]
    fault: isolate
  - at: PT50M
    target: block[2].container[0].hvac
    fault: repair
";
const HOUR: u64 = 3600;
/// PT20M: the reset is due on the tick the resumed run starts with.
const CHECKPOINT_TICK: u64 = 1200;

fn resolved(bytes: &[u8]) -> Resolved {
    let s = load(bytes, &PlantConfig::gw01()).unwrap();
    let params = RunParams {
        seed: s.seed.unwrap_or(42),
        start_unix_s: s.start_unix_s.unwrap_or(1_784_030_400),
        speed: 60.0,
        age_years: 0,
    };
    s.resolve(params)
}

fn fresh(r: &Resolved) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    r.simulation(cfg, models)
}

fn run(r: &Resolved, ticks: u64) -> (Simulation, Vec<(u64, Event)>) {
    let mut sim = fresh(r);
    let log = play(&mut sim, &mut Player::new(r), ticks);
    (sim, log)
}

#[test]
fn one_tuple_is_one_run() {
    let r = resolved(BUSY_HOUR);
    let (a, log_a) = run(&r, HOUR);
    let (b, log_b) = run(&resolved(BUSY_HOUR), HOUR);
    assert_eq!(a.state(), b.state());
    assert_eq!(log_a, log_b);
    assert_eq!(a.state().event_log, b.state().event_log);

    // Seven physical events, seven marks; the data fault leaves none.
    let marks = log_a
        .iter()
        .filter(|(_, e)| matches!(e, Event::FaultApplied(_)))
        .count();
    assert_eq!(marks, 7);
    assert_eq!(a.state().meta.scenario.as_ref(), Some(&r.id));
}

#[test]
fn a_checkpoint_mid_scenario_continues_the_same_run() {
    let r = resolved(BUSY_HOUR);
    let (continuous, log) = run(&r, HOUR);

    let (first, first_log) = run(&r, CHECKPOINT_TICK);
    let bytes = checkpoint::save(first.state()).unwrap();
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut resumed = r
        .resume(cfg, models, checkpoint::load(&bytes).unwrap())
        .unwrap();
    let mut player = Player::new(&r);
    player.catch_up(resumed.state().tick);
    let rest = play(&mut resumed, &mut player, HOUR - CHECKPOINT_TICK);

    assert_eq!(continuous.state(), resumed.state());
    assert_eq!([first_log, rest].concat(), log);
}

#[test]
fn a_checkpoint_refuses_another_story() {
    let busy = resolved(BUSY_HOUR);
    let (sim, _) = run(&busy, 60);
    let tree = || checkpoint::load(&checkpoint::save(sim.state()).unwrap()).unwrap();
    let cfg = PlantConfig::gw01;
    let models = || gw01_models(&PlantConfig::gw01());

    let other = resolved(include_bytes!("fixtures/empty.yaml"));
    assert!(matches!(
        other.resume(cfg(), models(), tree()),
        Err(ResumeError::OtherScenario { .. })
    ));

    let plain = Simulation::new(cfg(), models(), 42, busy.params.start_unix_s);
    assert!(matches!(
        busy.resume(cfg(), models(), plain.state().clone()),
        Err(ResumeError::NoScenario(_))
    ));

    let reseeded = Resolved {
        params: RunParams {
            seed: 43,
            ..busy.params
        },
        ..busy.clone()
    };
    assert_eq!(
        reseeded.resume(cfg(), models(), tree()).err(),
        Some(ResumeError::OtherSeed {
            expected: 43,
            found: 42
        })
    );
}

/// A loaded scenario with no events is the plain run: the same log, and
/// the same tree once the scenario's name in run identity is set aside.
#[test]
fn an_empty_scenario_costs_nothing() {
    let r = resolved(include_bytes!("fixtures/empty.yaml"));
    let (played, log) = run(&r, HOUR);

    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut plain = Simulation::new(cfg, models, r.params.seed, r.params.start_unix_s);
    let plain_log = play(&mut plain, &mut Player::new(&r), HOUR);

    assert_eq!(log, plain_log);
    let mut tree = played.state().clone();
    assert!(tree.meta.scenario.take().is_some());
    assert_eq!(&tree, plain.state());
}
