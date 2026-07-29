//! Determinism contract: (seed, config, inputs) fully determines the state,
//! byte for byte, and a checkpoint resume continues the exact same run.

use bess_core::checkpoint;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, SyntheticWeather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

/// Four hours: covers idle and the night charge window of the default plan.
const TICKS: u64 = 4 * 3600;

/// Golden digest of the state tree after `TICKS` ticks with seed 42.
///
/// If a deliberate model or state-schema change moves this value, update it
/// from the number printed by the failing assertion. Any other change that
/// moves it is a broken determinism contract.
const GOLDEN_DIGEST: u64 = 0xcb4b_b95e_4afa_67a3;

fn run(seed: u64, ticks: u64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, seed, START_UNIX_S);
    let weather = SyntheticWeather::default();
    for _ in 0..ticks {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
    }
    sim
}

#[test]
fn same_seed_produces_identical_state() {
    let a = run(42, TICKS);
    let b = run(42, TICKS);
    assert_eq!(a.state(), b.state());
    assert_eq!(
        checkpoint::state_digest(a.state()),
        checkpoint::state_digest(b.state())
    );
}

#[test]
fn different_seed_produces_different_state() {
    let a = run(42, 600);
    let b = run(43, 600);
    assert_ne!(
        checkpoint::state_digest(a.state()),
        checkpoint::state_digest(b.state())
    );
}

#[test]
fn golden_snapshot_digest_is_stable() {
    let digest = checkpoint::state_digest(run(42, TICKS).state());
    assert_eq!(
        digest, GOLDEN_DIGEST,
        "state digest changed: {digest:#018x}. If the model change is \
         deliberate, update GOLDEN_DIGEST; otherwise determinism is broken."
    );
}

#[test]
fn checkpoint_resume_matches_continuous_run() {
    let continuous = run(42, TICKS);

    let half = TICKS / 2;
    let first = run(42, half);
    let bytes = checkpoint::save(first.state()).unwrap();
    let restored = checkpoint::load(&bytes).unwrap();

    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut resumed = Simulation::from_state(cfg, models, restored);
    let weather = SyntheticWeather::default();
    for _ in 0..(TICKS - half) {
        let inputs = weather.inputs_at(resumed.unix_time_s());
        resumed.step(&inputs);
    }

    assert_eq!(continuous.state(), resumed.state());
}
