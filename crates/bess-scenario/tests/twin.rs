//! The acceptance of phase 4 PR1: the phase 2 causal chain, three ways.
//!
//! The plant is the one `bess-models/tests/alarm_chain.rs` prepares: a July
//! afternoon, block 2's racks at 54 C and its containers at 45 C, full
//! discharge asked. Its two HVAC units fail before the first tick, once by
//! the direct commands (the log that test holds today), once through
//! `Simulation::apply`, once by `hot-afternoon.yaml` played by the player.
//! Door and file produce one log; that log without its two marks is the
//! direct one. The engine adds sequencing and a mark, never physics.

#[path = "support/play.rs"]
mod support;

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::AlarmNode;
use bess_core::kernel::{Event, FaultAction};
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};
use bess_scenario::{load, Player, RunParams};
use support::play;

/// 2026-07-14 14:00 UTC, the warmest afternoon of the replayed year.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;
const SEED: u64 = 7;
const HOT_BLOCK: usize = 2;
const TICKS: u64 = 3600;

/// `alarm_chain.rs`'s plant, without its HVAC failures.
fn hot_plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, SEED, JULY_AFTERNOON_S);
    for rack in state
        .blocks
        .iter_mut()
        .flat_map(|blk| blk.containers.iter_mut().flat_map(|c| c.racks.iter_mut()))
    {
        rack.soc = 0.9;
        rack.cell_temp_c = 30.0;
    }
    for hot in &mut state.blocks[HOT_BLOCK].containers {
        hot.air_temp_c = 45.0;
        for rack in &mut hot.racks {
            rack.cell_temp_c = 54.0;
        }
    }
    let models = gw01_models(&cfg);
    let mut sim = Simulation::from_state(cfg, models, state);
    sim.set_external_setpoint_w(Some(100.0e6));
    sim
}

fn step_all(sim: &mut Simulation) -> Vec<(u64, Event)> {
    let weather = gw01_weather();
    let mut log = Vec::new();
    for tick in 0..TICKS {
        let events = sim.step(&weather.inputs_at(sim.unix_time_s()));
        log.extend(events.iter().map(|e| (tick, *e)));
    }
    log
}

fn first_raise(log: &[(u64, Event)], want: impl Fn(AlarmNode, u8) -> bool) -> Option<u64> {
    log.iter().find_map(|(tick, e)| match *e {
        Event::AlarmRaised { node, bit, .. } if want(node, bit) => Some(*tick),
        _ => None,
    })
}

#[test]
fn the_chain_played_from_a_file_is_the_chain_scripted() {
    let mut direct = hot_plant();
    for container in 0..2 {
        direct.set_hvac_failed(HOT_BLOCK, container, true).unwrap();
    }
    let direct_log = step_all(&mut direct);

    let mut door = hot_plant();
    for container in 0..2 {
        let failure = FaultAction::HvacFailure {
            block: HOT_BLOCK,
            container,
        };
        assert_eq!(door.apply(failure), Ok(true));
    }
    let door_log = step_all(&mut door);

    let scenario = load(
        include_bytes!("fixtures/hot-afternoon.yaml"),
        &PlantConfig::gw01(),
    )
    .unwrap();
    assert_eq!(
        (scenario.seed, scenario.start_unix_s),
        (Some(SEED), Some(JULY_AFTERNOON_S))
    );
    let resolved = scenario.resolve(RunParams {
        seed: SEED,
        start_unix_s: JULY_AFTERNOON_S,
        speed: 60.0,
        age_years: 0,
    });
    let mut file = hot_plant();
    let file_log = play(&mut file, &mut Player::new(&resolved), TICKS);

    // Door and file: one log, one tree.
    assert_eq!(file_log, door_log);
    assert_eq!(file.state().event_log, door.state().event_log);
    assert_eq!(file.state(), door.state());

    // Without the two marks, which open tick 0, the log the direct
    // commands produce.
    let marks: Vec<_> = file_log
        .iter()
        .filter(|(_, e)| matches!(e, Event::FaultApplied(_)))
        .collect();
    assert_eq!(marks.len(), 2);
    assert!(file_log[..2]
        .iter()
        .all(|(t, e)| *t == 0 && matches!(e, Event::FaultApplied(_))));
    let unmarked: Vec<_> = file_log
        .iter()
        .filter(|(_, e)| !matches!(e, Event::FaultApplied(_)))
        .copied()
        .collect();
    assert_eq!(unmarked, direct_log);

    // So the chain still holds under the engine: failure on the first
    // tick, then derate, miss and limit, minutes apart.
    let failure = first_raise(&file_log, |n, bit| {
        n == AlarmNode::Block { block: HOT_BLOCK } && bit == b::HVAC_FAILURE
    });
    let derate = first_raise(&file_log, |n, bit| {
        matches!(
            n,
            AlarmNode::Rack {
                block: HOT_BLOCK,
                ..
            }
        ) && bit == r::DERATE_ACTIVE
    })
    .unwrap();
    let miss = first_raise(&file_log, |n, bit| {
        n == AlarmNode::Block { block: HOT_BLOCK } && bit == b::SETPOINT_NOT_MET
    })
    .unwrap();
    let limited = first_raise(&file_log, |n, bit| {
        n == AlarmNode::Site && bit == s::POWER_LIMITED
    })
    .unwrap();
    assert_eq!(failure, Some(0));
    assert!(derate + 60 <= miss && miss + 60 <= limited);
}
