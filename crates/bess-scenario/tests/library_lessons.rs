//! What two library cases are for, held directly. A record holds a case
//! to its own past, so it cannot tell when the plan under a case moves
//! and the case stops showing its lesson: both of these did once, when
//! the SMARD plan of M2 phase 3 left their faults in idle hours. These
//! tests play the files and check the story their headers tell.

#[path = "support/play.rs"]
mod support;

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::AlarmNode;
use bess_core::kernel::{Event, FaultAction};
use bess_core::state::PcsOpState;
use bess_core::{PlantConfig, Simulation};
use bess_models::gw01_models;
use bess_scenario::{library, Player, RunParams};
use support::play;

fn played(name: &str) -> (Simulation, Vec<(u64, Event)>) {
    let cfg = PlantConfig::gw01();
    let scenario = library::find(name)
        .expect("in the library")
        .load(&cfg)
        .expect("loads");
    let ticks = scenario.length_s.expect("a length");
    let resolved = scenario.clone().resolve(RunParams {
        seed: scenario.seed.expect("a seed"),
        start_unix_s: scenario.start_unix_s.expect("a start"),
        speed: 60.0,
    });
    let mut sim = resolved.simulation(cfg.clone(), gw01_models(&cfg));
    let mut player = Player::new(&resolved);
    let log = play(&mut sim, &mut player, ticks);
    (sim, log)
}

/// Tick of the first raise matching `want` after tick `after`, if any.
fn raise_after(
    log: &[(u64, Event)],
    after: u64,
    want: impl Fn(AlarmNode, u8) -> bool,
) -> Option<u64> {
    log.iter().find_map(|(tick, e)| match *e {
        Event::AlarmRaised { node, bit, .. } if *tick > after && want(node, bit) => Some(*tick),
        _ => None,
    })
}

/// The chain of `alarm_chain.rs`, from a fresh plant and its plan: the
/// block's racks derate, then the block misses its setpoint, then the
/// site reports its limit, within one discharge. The block and the site
/// also miss at the end of every charge, when the plant is full; the
/// misses that count are the ones after the derate.
#[test]
fn both_units_down_shows_the_causal_chain() {
    let (_, log) = played("hvac-failure-both-units");
    let in_block_2 = |n| matches!(n, AlarmNode::Rack { block: 2, .. });
    let derate = raise_after(&log, 0, |n, bit| in_block_2(n) && bit == r::DERATE_ACTIVE)
        .expect("block 2's racks never derated");
    let warned = raise_after(&log, 0, |n, bit| {
        in_block_2(n) && bit == r::OVER_TEMP_WARNING
    })
    .expect("block 2's racks never warned");
    let miss = raise_after(&log, derate, |n, bit| {
        n == AlarmNode::Block { block: 2 } && bit == b::SETPOINT_NOT_MET
    })
    .expect("the block never missed its setpoint after the derate");
    let limit = raise_after(&log, miss, |n, bit| {
        n == AlarmNode::Site && bit == s::POWER_LIMITED
    })
    .expect("the site never reported its limit after the miss");
    assert!(
        warned < derate,
        "the warning comes first: {warned}, {derate}"
    );
    assert!(limit - derate < 3600, "one discharge: {derate}..{limit}");
}

/// The converter trips while it is running, and the reset brings it back
/// to running through its startup sequence, not straight to standby.
#[test]
fn the_pcs_trips_mid_discharge_and_the_reset_returns_it_to_run() {
    let (sim, log) = played("pcs-trip-and-reset");
    let trip = log
        .iter()
        .position(|(_, e)| *e == Event::FaultApplied(FaultAction::PcsTrip { block: 5 }))
        .expect("the trip fired");
    let changes: Vec<(PcsOpState, PcsOpState)> = log[trip..]
        .iter()
        .filter_map(|(_, e)| match *e {
            Event::PcsStateChanged { block: 5, from, to } => Some((from, to)),
            _ => None,
        })
        .collect();
    assert_eq!(
        changes[..7],
        [
            (PcsOpState::Run, PcsOpState::Fault),
            (PcsOpState::Fault, PcsOpState::Off),
            (PcsOpState::Off, PcsOpState::Precharge),
            (PcsOpState::Precharge, PcsOpState::ContactorClose),
            (PcsOpState::ContactorClose, PcsOpState::Synchronize),
            (PcsOpState::Synchronize, PcsOpState::Ramp),
            (PcsOpState::Ramp, PcsOpState::Run),
        ]
    );
    let partial = raise_after(&log, 0, |n, bit| {
        n == AlarmNode::Site && bit == s::PARTIAL_AVAILABILITY
    });
    assert_eq!(
        partial,
        Some(log[trip].0),
        "the site loses a block at the trip"
    );
    assert_eq!(sim.state().blocks[5].alarm_bits & (1 << b::PCS_FAULT), 0);
}
