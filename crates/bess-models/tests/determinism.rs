//! Determinism contract: (seed, config, dataset) fully determines the state,
//! byte for byte, and a checkpoint resume continues the exact same run.
//!
//! Since M1 this runs on the replayed weather year rather than a synthetic
//! function of the clock, so the golden digest below covers the dataset and
//! its interpolation as well as the kernel: a dataset that is regenerated
//! without the pin moving with it cannot reach this assertion, and one that
//! is replayed differently fails it.

use bess_core::checkpoint;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

/// Four hours: covers idle and the first hour of the night charge, which
/// starts at 03:00 UTC under the replayed 2024-01-01 prices as it did under
/// the M0 synthetic curve.
const TICKS: u64 = 4 * 3600;

/// Golden digest of the state tree after `TICKS` ticks with seed 42.
///
/// If a deliberate model or state-schema change moves this value, update it
/// from the number printed by the failing assertion. Any other change that
/// moves it is a broken determinism contract. Last regenerated in M3
/// phase 4 PR2, for schema and the substation's own physics: the grid
/// voltage input swings, the POI voltage follows it, the MV bus and the
/// tap position are state, and the transformer's two temperatures move
/// with the night charge; the racks and converters did not move
/// (0x1610_fdd6_d23a_caf1 before). Before that, M3
/// phase 4 PR1, for schema alone: the substation carries its disconnector
/// and earthing-switch positions and the breaker's closing sequence,
/// none of which move in these four hours; 0xbcc3_d38e_983b_1b37 before.
/// Before that, M3 phase 3, for schema alone: `PcsState` carries the converter's thermal
/// node, its overload budget with the arming latch and the permitted AC
/// power; in these four hours the night charge asks each converter for
/// its rating and no more, so the budget stays full and the physics is
/// phase 2's, 0x25a8_3c2f_d958_c932. Before that, M3
/// phase 2 PR2, for physics and schema: the converters follow their
/// setpoints through a ramp limit and a lag, so the night charge starts
/// ten minutes later than the plan's step, and `PcsState` carries the
/// follower's two values; 0xa2fe_2b19_360f_b636 before. Before that, M3
/// phase 2 PR1, for schema alone: `PcsState` gained `state_elapsed_s`
/// (zero on a plant that starts connected) and `PcsOpState` its new
/// variants, none visited in these four hours; the physics is phase 1's,
/// 0x4d61_5d7b_b58a_1f8a. Before that, M3 phase 1, for physics and schema: the converter converts on the Sandia
/// surface at the racks' DC voltage, rack limits are current-bounded at
/// the voltage the rack presents, and `PcsState` carries `dc_voltage_v`.
/// Earlier value: 0xa57c_9428_cb1e_3071 (M2.5 phase 2 PR1 after the
/// hysteresis step went rational; before that 0xe46c_f1ee_8656_cf35 with
/// the exp cut: schema and the event log, every rack gained the reported
/// SoC and its sensor's two error parameters, the estimator's corrections
/// are events).
/// Before that: M2.5 phase 2 PR1, for schema and the event log: every rack gained the
/// reported SoC and its sensor's two error parameters, and the estimator's
/// corrections are events, so the log's count and digest move with them.
/// The physics did not: the decoupling proof in `reported_soc.rs` holds
/// the truth trajectory to phase 1's value with the new fields cut out.
/// Earlier values: 0xe70d_52fc_5ad0_8f93 (M4 phase 2 PR1, before PR2 added
/// the quarter-hour report's accumulators to the tree),
/// 0x2dc6_1238_f107_e3ae (M4 phase 1 PR2, before phase 2
/// PR1 added the external limit and the operator's reactive settings to
/// the tree, at rest here), 0x2006_7e9a_3e7a_0417 (M4 phase 1 PR1, before PR2 added
/// each converter's reactive power, the controller's mode and target and
/// the POI's power factor to the tree, zero and unity in a plant left at
/// its default), 0x6944_3f7f_0627_570e (M3 phase 4 PR2, the value before
/// M4 phase 1 PR1 replaced the frequency sinusoid with the synthetic
/// Continental European series and added the grid-code state; inside the
/// band the plant's energy figures do not move, the frequency in the tree
/// does), 0x95c3_aa5c_e7bb_3679 (M2.5 phase 1, a physics and a
/// schema change at once: every rack gained the OCV hysteresis state and
/// the cell model moves it, so voltages, currents and the night charge's
/// heat differ from the first charging tick; the switch-off proof in
/// `ocv_hysteresis.rs` holds the value before it with the field cut out),
/// 0xf3fb_8ad4_75c9_980f (M2 phase 4 PR1, for schema
/// alone: run identity gained the scenario (`meta.scenario`, `null` here)
/// and every rack the excess self-discharge rate (`0.0` here)),
/// 0xa24f_1646_1b6a_d90e (M2 phase 3 PR2, for schema
/// alone: the substation gained the fiscal revenue meter; the switch to
/// replayed day-ahead prices in the same PR did not move it, since in these
/// four hours both price curves charge in the same hour),
/// 0x5c71_c797_1ee9_3f0c (M2 phase 3 PR1, the block
/// mode, the return sequencer's count and countdown, and the
/// station-service meter), 0x4795_fbb4_0c5e_a021 (M2 phase 2 PR1, the alarm words,
/// the setpoint-miss timer, the HVAC failure flag, the event log, and the
/// queue of events a reset leaves for the next tick),
/// 0x449f_9d2c_d5ef_3461 (M0.5), 0xdb39_2afb_95e7_3220 (M1 PR2, the
/// switch to replayed weather), 0xe545_c288_049e_6096 (M1 PR3, the
/// two-node thermal model), 0x0817_b721_5412_38c8 (M1 PR4, staged HVAC),
/// 0x1446_6f26_c751_bc02 (M1 PR5, the auxiliary inventory),
/// 0xcd26_9803_d062_e1ad (M2 phase 1 PR2, cell spread and balancing).
const GOLDEN_DIGEST: u64 = 0xefe9_68cf_1f43_0433;

fn run(seed: u64, ticks: u64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, seed, START_UNIX_S);
    let weather = gw01_weather();
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
    let weather = gw01_weather();
    for _ in 0..(TICKS - half) {
        let inputs = weather.inputs_at(resumed.unix_time_s());
        resumed.step(&inputs);
    }

    assert_eq!(continuous.state(), resumed.state());
}
