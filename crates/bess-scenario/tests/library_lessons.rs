//! What two library cases are for, held directly. A record holds a case
//! to its own past, so it cannot tell when the plan under a case moves
//! and the case stops showing its lesson: both of these did once, when
//! the SMARD plan of M2 phase 3 left their faults in idle hours. These
//! tests play the files and check the story their headers tell.

#[path = "support/play.rs"]
mod support;

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::{has_bit, AlarmNode};
use bess_core::kernel::{BreakerSequence, Event, FaultAction, Switch};
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

/// Two converters trip; the eighteen left carry the site on their overload
/// budgets for ten minutes, then the site reports its limit, and no block
/// ever calls it a miss.
#[test]
fn the_overload_budget_carries_the_site_for_ten_minutes_then_the_limit() {
    let (sim, log) = played("pcs-overload-budget");
    let trip = log
        .iter()
        .position(|(_, e)| *e == Event::FaultApplied(FaultAction::PcsTrip { block: 3 }))
        .expect("the trip fired");
    let trip_tick = log[trip].0;
    let limit = raise_after(&log, trip_tick, |n, bit| {
        n == AlarmNode::Site && bit == s::POWER_LIMITED
    })
    .expect("the site never reported its limit");
    assert!(
        (trip_tick + 560..=trip_tick + 700).contains(&limit),
        "the limit came at {limit}, {} s after the trip",
        limit - trip_tick
    );
    // No block calls the budget's end a miss; the misses that come later
    // are the racks running empty at the end of the discharge.
    let misses = log
        .iter()
        .filter(|(tick, e)| {
            *tick <= limit + 300
                && matches!(e, Event::AlarmRaised { node: AlarmNode::Block { .. }, bit, .. } if *bit == b::SETPOINT_NOT_MET)
        })
        .count();
    assert_eq!(misses, 0, "a block called the budget's end a miss");
    // An hour and a half at the rating refills the budget and re-arms
    // the overload by the end of the case.
    let st = sim.state();
    assert_eq!(st.blocks[0].pcs.overload_budget, 1.0);
    assert!(st.blocks[0].pcs.overload_armed);
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

/// The night of work on the bus: the breaker opens by hand and every
/// converter goes to off, the disconnectors and the earthing switch move
/// in the only order the interlocks allow, and the close lands on the
/// tick after the command, the return following.
#[test]
fn the_earthing_work_runs_the_switchgear_in_order() {
    let (sim, log) = played("substation-earthing-work");
    let open = log
        .iter()
        .position(|(_, e)| *e == Event::FaultApplied(FaultAction::HvBreakerOpen))
        .expect("the open fired");
    let offs = log[open..]
        .iter()
        .take_while(|(t, _)| *t == log[open].0)
        .filter(|(_, e)| {
            matches!(
                e,
                Event::PcsStateChanged {
                    to: PcsOpState::Off,
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        offs, 20,
        "every converter opened its contactors on the open"
    );
    let order: Vec<&Event> = log
        .iter()
        .filter_map(|(_, e)| {
            matches!(
                e,
                Event::SwitchOperated { .. } | Event::BreakerSequenceChanged { .. }
            )
            .then_some(e)
        })
        .collect();
    assert_eq!(
        order,
        vec![
            &Event::SwitchOperated {
                switch: Switch::Disconnectors,
                closed: false
            },
            &Event::SwitchOperated {
                switch: Switch::EarthingSwitch,
                closed: true
            },
            &Event::SwitchOperated {
                switch: Switch::EarthingSwitch,
                closed: false
            },
            &Event::SwitchOperated {
                switch: Switch::Disconnectors,
                closed: true
            },
            &Event::BreakerSequenceChanged {
                from: BreakerSequence::Idle,
                to: BreakerSequence::Closing
            },
            &Event::BreakerSequenceChanged {
                from: BreakerSequence::Closing,
                to: BreakerSequence::Idle
            },
        ]
    );
    let close = log
        .iter()
        .position(|(_, e)| *e == Event::FaultApplied(FaultAction::HvBreakerClose))
        .expect("the close fired");
    let closed = log
        .iter()
        .position(|(_, e)| {
            *e == Event::BreakerSequenceChanged {
                from: BreakerSequence::Closing,
                to: BreakerSequence::Idle,
            }
        })
        .expect("the breaker closed");
    assert_eq!(
        log[closed].0, log[close].0,
        "the close lands on the command's tick"
    );
    let st = sim.state();
    assert_eq!(st.ems.blocks_released, 20);
    assert!(st.blocks.iter().all(|b| b.pcs.op_state.is_connected()));
}

/// `frequency-event`: the dip raises `lfsm_active` and four megawatts at
/// once, the rise cuts eight, the excursion trips the site, and the reset
/// after the recovery returns it through the staggered return.
#[test]
fn the_frequency_event_answers_the_grid_and_survives_the_excursion() {
    let (sim, log) = played("frequency-event");
    let site_bit = |bit: u8| move |node: AlarmNode, b: u8| node == AlarmNode::Site && b == bit;
    let dip = raise_after(&log, 0, site_bit(s::LFSM_ACTIVE)).expect("lfsm on the dip");
    assert!((600..=602).contains(&dip), "lfsm at {dip}");
    let trip =
        raise_after(&log, 2000, site_bit(s::GRID_FREQUENCY_TRIP)).expect("the excursion trips");
    assert!((2700..=2702).contains(&trip), "trip at {trip}");
    assert!(log.iter().any(|(tick, e)| *tick >= 3300
        && matches!(
            e,
            Event::FaultApplied(FaultAction::AlarmReset(bess_core::alarms::ResetScope::Site))
        )));
    let st = sim.state();
    assert!(!has_bit(st.alarm_bits, s::GRID_FREQUENCY_TRIP));
    assert_eq!(
        st.substation.hv_breaker,
        bess_core::state::BreakerState::Closed
    );
    assert_eq!(st.ems.blocks_released, 20);
    assert!(st.blocks.iter().all(|b| b.pcs.op_state.is_connected()));
}

/// `curtailment-order`: the limit binds from the write, `external_
/// limitation` raises at once and `power_limited` never does, the plant
/// sits at 60 % after its walk down, and the revert lifts the flag.
#[test]
fn the_curtailment_order_is_obeyed_flagged_and_lifted() {
    let (sim, log) = played("curtailment-order");
    let site_bit = |bit: u8| move |node: AlarmNode, b: u8| node == AlarmNode::Site && b == bit;
    let flagged = raise_after(&log, 0, site_bit(s::EXTERNAL_LIMITATION)).expect("flagged");
    assert!((600..=602).contains(&flagged), "flag at {flagged}");
    assert!(
        raise_after(&log, 0, site_bit(s::POWER_LIMITED)).is_none(),
        "the plant delivered what it was asked"
    );
    let lifted = log
        .iter()
        .find_map(|(tick, e)| match *e {
            Event::AlarmCleared {
                node: AlarmNode::Site,
                bit,
                ..
            } if bit == s::EXTERNAL_LIMITATION => Some(*tick),
            _ => None,
        })
        .expect("lifted");
    assert!((3000..=3002).contains(&lifted), "lifted at {lifted}");
    let st = sim.state();
    assert!(!st.ems.external_limit.enabled);
    assert!(st.substation.poi_active_power_w > 90.0e6);
}

/// `protection-trip-staggered-return`: the period the trip lands in
/// misses its schedule by more than the frame, so `schedule_deviation`
/// raises on its close and clears once the return has the plant back on
/// plan.
#[test]
fn the_staggered_return_shows_on_the_quarter_hour_report() {
    let (sim, log) = played("protection-trip-staggered-return");
    let site_bit = |bit: u8| move |node: AlarmNode, b: u8| node == AlarmNode::Site && b == bit;
    let raised = raise_after(&log, 0, site_bit(s::SCHEDULE_DEVIATION)).expect("the trip's period");
    assert!(
        (raised + 1).is_multiple_of(900),
        "on a period close, not at {raised}"
    );
    assert!(
        log.iter().any(|(tick, e)| *tick > raised
            && matches!(
                e,
                Event::AlarmCleared {
                    node: AlarmNode::Site,
                    bit,
                    ..
                } if *bit == s::SCHEDULE_DEVIATION
            )),
        "never cleared"
    );
    // The case ends on the plan's last discharge hour, where the plant
    // runs down; whether that last period is over the frame is the
    // plan's business, not the trip's.
    let _ = sim;
}
