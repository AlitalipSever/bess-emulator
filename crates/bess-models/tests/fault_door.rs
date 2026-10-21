//! The kernel's fault door: `Simulation::apply` is the commands phases 2
//! and 3 built, plus a mark. For every action it leaves the tree exactly as
//! the direct command does, with `Event::FaultApplied` ahead of whatever the
//! command left pending; it answers `false` on a no-op and still marks it;
//! and it refuses a node the site does not have with nothing changed and no
//! mark. The tests live here rather than beside `kernel/fault.rs` because a
//! `Simulation` needs models, and `bess-core` has none.

use bess_core::alarms::{AlarmNode, ResetScope};
use bess_core::kernel::{CommandError, Event, FaultAction, FaultError};
use bess_core::state::{BlockMode, PcsOpState};
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 14:00 UTC.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;

fn plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, JULY_AFTERNOON_S);
    sim.set_external_setpoint_w(Some(50.0e6));
    step(&mut sim);
    sim
}

fn step(sim: &mut Simulation) -> Vec<Event> {
    let inputs = gw01_weather().inputs_at(sim.unix_time_s());
    sim.step(&inputs).to_vec()
}

/// A plant with something to reset: block 3's PCS tripped and its bit
/// latched by a tick.
fn tripped_plant() -> Simulation {
    let mut sim = plant();
    sim.trip_pcs(3).unwrap();
    step(&mut sim);
    sim
}

/// The tree with the between-ticks queue set aside.
fn without_pending(state: &SiteState) -> SiteState {
    let mut s = state.clone();
    s.pending_events.clear();
    s
}

/// `apply(action)` against the direct command on a twin plant: same tree,
/// and the same pending events behind the mark.
fn assert_twin(
    make: fn() -> Simulation,
    action: FaultAction,
    direct: impl FnOnce(&mut Simulation),
) {
    let mut via_door = make();
    let mut via_command = make();
    assert_eq!(via_door.apply(action), Ok(true), "{action:?}");
    direct(&mut via_command);

    let door = via_door.state();
    let command = via_command.state();
    assert_eq!(
        without_pending(door),
        without_pending(command),
        "{action:?}"
    );
    assert_eq!(door.pending_events[0], Event::FaultApplied(action));
    assert_eq!(door.pending_events[1..], command.pending_events[..]);
}

#[test]
fn every_command_action_is_its_command_plus_a_mark() {
    assert_twin(
        plant,
        FaultAction::HvacFailure {
            block: 2,
            container: 1,
        },
        |s| s.set_hvac_failed(2, 1, true).unwrap(),
    );
    assert_twin(
        || {
            let mut s = plant();
            s.set_hvac_failed(2, 1, true).unwrap();
            s
        },
        FaultAction::HvacRepair {
            block: 2,
            container: 1,
        },
        |s| s.set_hvac_failed(2, 1, false).unwrap(),
    );
    assert_twin(plant, FaultAction::PcsTrip { block: 4 }, |s| {
        assert!(s.trip_pcs(4).unwrap());
    });
    assert_twin(plant, FaultAction::ProtectionTrip, |s| {
        assert!(s.trip_protection());
    });
    let rack = (2, 1, 5);
    assert_twin(
        plant,
        FaultAction::RackIsolate {
            block: rack.0,
            container: rack.1,
            rack: rack.2,
        },
        |s| {
            s.set_rack_in_service(rack.0, rack.1, rack.2, false)
                .unwrap();
        },
    );
    assert_twin(
        || {
            let mut s = plant();
            s.set_rack_in_service(2, 1, 5, false).unwrap();
            s
        },
        FaultAction::RackRestore {
            block: rack.0,
            container: rack.1,
            rack: rack.2,
        },
        |s| s.set_rack_in_service(rack.0, rack.1, rack.2, true).unwrap(),
    );
    assert_twin(
        plant,
        FaultAction::BlockMaintenanceEnter { block: 9 },
        |s| {
            s.set_block_mode(9, BlockMode::Maintenance).unwrap();
        },
    );
    assert_twin(
        || {
            let mut s = plant();
            s.set_block_mode(9, BlockMode::Maintenance).unwrap();
            // Entering maintenance queues the converter's Off transition;
            // one tick drains it so the exit starts from an empty queue.
            step(&mut s);
            s
        },
        FaultAction::BlockMaintenanceExit { block: 9 },
        |s| s.set_block_mode(9, BlockMode::InService).unwrap(),
    );
    assert_twin(
        tripped_plant,
        FaultAction::AlarmReset(ResetScope::Block(3)),
        |s| {
            s.reset_alarms(ResetScope::Block(3)).unwrap();
        },
    );
}

/// The one action without a command: the rate lands in the rack, converted
/// once, and nothing else in the tree moves.
#[test]
fn self_discharge_sets_the_rate_and_nothing_else() {
    let mut sim = plant();
    let before = sim.state().clone();
    let action = FaultAction::RackSelfDischarge {
        block: 2,
        container: 1,
        rack: 5,
        excess_ppm_per_day: 6_667,
    };
    assert_eq!(sim.apply(action), Ok(true));

    let mut after = without_pending(sim.state());
    let r = &mut after.blocks[2].containers[1].racks[5];
    assert_eq!(r.self_discharge_excess_per_s, 6_667.0 * 1.0e-6 / 86_400.0);
    r.self_discharge_excess_per_s = 0.0;
    assert_eq!(after, before);
    assert_eq!(
        sim.state().pending_events,
        vec![Event::FaultApplied(action)]
    );

    // The same rate again changes nothing; zero heals it.
    assert_eq!(sim.apply(action), Ok(false));
    let heal = FaultAction::RackSelfDischarge {
        excess_ppm_per_day: 0,
        block: 2,
        container: 1,
        rack: 5,
    };
    assert_eq!(sim.apply(heal), Ok(true));
}

/// An action that finds the plant already where it asks answers `false`
/// and leaves the tree alone, but is still marked: the log records what
/// was done, and a second trip was done.
#[test]
fn a_no_op_answers_false_and_is_still_marked() {
    let no_ops = [
        FaultAction::HvacRepair {
            block: 0,
            container: 0,
        },
        FaultAction::RackRestore {
            block: 0,
            container: 0,
            rack: 0,
        },
        FaultAction::BlockMaintenanceExit { block: 0 },
        FaultAction::AlarmReset(ResetScope::Site),
    ];
    for action in no_ops {
        let mut sim = plant();
        let before = sim.state().clone();
        assert_eq!(sim.apply(action), Ok(false), "{action:?}");
        assert_eq!(without_pending(sim.state()), before, "{action:?}");
        assert_eq!(
            sim.state().pending_events,
            vec![Event::FaultApplied(action)]
        );
    }

    let mut sim = tripped_plant();
    assert_eq!(sim.apply(FaultAction::PcsTrip { block: 3 }), Ok(false));
    assert_eq!(sim.apply(FaultAction::ProtectionTrip), Ok(true));
    assert_eq!(sim.apply(FaultAction::ProtectionTrip), Ok(false));
}

/// A node the site does not have is refused by the command it dispatches
/// to, before anything changes, and leaves no mark.
#[test]
fn a_missing_node_is_refused_with_nothing_changed() {
    let refused = [
        (
            FaultAction::HvacFailure {
                block: 2,
                container: 9,
            },
            FaultError::Command(CommandError::NoSuchContainer {
                block: 2,
                container: 9,
            }),
        ),
        (
            FaultAction::PcsTrip { block: 25 },
            FaultError::Command(CommandError::NoSuchNode(AlarmNode::Block { block: 25 })),
        ),
        (
            FaultAction::RackSelfDischarge {
                block: 2,
                container: 1,
                rack: 12,
                excess_ppm_per_day: 333,
            },
            FaultError::Command(CommandError::NoSuchNode(AlarmNode::Rack {
                block: 2,
                container: 1,
                rack: 12,
            })),
        ),
        (
            FaultAction::BlockMaintenanceEnter { block: 20 },
            FaultError::Command(CommandError::NoSuchNode(AlarmNode::Block { block: 20 })),
        ),
    ];
    for (action, err) in refused {
        let mut sim = plant();
        let before = sim.state().clone();
        assert_eq!(sim.apply(action), Err(err), "{action:?}");
        assert_eq!(*sim.state(), before, "{action:?}");
    }

    let mut sim = plant();
    let before = sim.state().clone();
    let scope = ResetScope::Rack {
        block: 0,
        container: 2,
        rack: 0,
    };
    assert!(matches!(
        sim.apply(FaultAction::AlarmReset(scope)),
        Err(FaultError::Reset(_))
    ));
    assert_eq!(*sim.state(), before);
}

/// On the tick after a trip the mark goes out first, then the PCS
/// transition, then the raise the tick's own evaluation finds, and the log
/// counts all of them.
#[test]
fn the_mark_precedes_its_consequences_on_the_next_tick() {
    let mut sim = plant();
    let count = sim.state().event_log.count;
    let trip = FaultAction::PcsTrip { block: 4 };
    sim.apply(trip).unwrap();
    let events = step(&mut sim);

    assert_eq!(events[0], Event::FaultApplied(trip));
    assert!(matches!(
        events[1],
        Event::PcsStateChanged {
            block: 4,
            to: PcsOpState::Fault,
            ..
        }
    ));
    assert!(events[2..].iter().any(
        |e| matches!(*e, Event::AlarmRaised { node, .. } if node == AlarmNode::Block { block: 4 })
    ));
    assert_eq!(sim.state().event_log.count, count + events.len() as u64);
}
