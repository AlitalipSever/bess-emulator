//! The causal chain, end to end: heat, derate, missed setpoint, limited
//! plant, each link raised by its own layer and in physical order.
//!
//! A July afternoon with one block whose HVAC has already been down for
//! hours in both containers: its racks start at 54 C, the site is asked for
//! full discharge, and the units stay failed. Nothing is scripted after the
//! first tick. The cells heat themselves across the EVE MB31 hot shoulder,
//! and the thermal mass is what spaces the alarms minutes apart.
//!
//! Both containers, because one is not enough to limit the site: as its
//! racks derate the block delivers less, makes less heat, and settles near
//! 59 C about 1.6 MW short, under the 2 MW the site bit waits for. The
//! physics limits itself, and the test has to ask for a failure big enough
//! to show on the site word.

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::{AlarmNode, Severity};
use bess_core::kernel::Event;
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 14:00 UTC, the warmest afternoon of the replayed year.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;
const HOT_BLOCK: usize = 2;

fn hot_plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, JULY_AFTERNOON_S);
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
    sim.set_hvac_failed(HOT_BLOCK, 0, true);
    sim.set_hvac_failed(HOT_BLOCK, 1, true);
    sim.set_external_setpoint_w(Some(100.0e6));
    sim
}

/// Tick of the first raise matching `want`, if any.
fn first_raise(log: &[(u64, Event)], want: impl Fn(AlarmNode, u8) -> bool) -> Option<u64> {
    log.iter().find_map(|(tick, e)| match *e {
        Event::AlarmRaised { node, bit, .. } if want(node, bit) => Some(*tick),
        _ => None,
    })
}

#[test]
fn a_failed_hvac_raises_derate_then_miss_then_limit() {
    let mut sim = hot_plant();
    let weather = gw01_weather();
    let mut log = Vec::new();
    for tick in 0..3600u64 {
        let events = sim.step(&weather.inputs_at(sim.unix_time_s()));
        log.extend(events.iter().map(|e| (tick, *e)));
    }

    let failure = first_raise(&log, |n, bit| {
        n == AlarmNode::Block { block: HOT_BLOCK } && bit == b::HVAC_FAILURE
    })
    .expect("the failed unit never showed on the block word");
    let derate = first_raise(&log, |n, bit| {
        matches!(
            n,
            AlarmNode::Rack {
                block: HOT_BLOCK,
                ..
            }
        ) && bit == r::DERATE_ACTIVE
    })
    .expect("no hot rack reported derating");
    let miss = first_raise(&log, |n, bit| {
        n == AlarmNode::Block { block: HOT_BLOCK } && bit == b::SETPOINT_NOT_MET
    })
    .expect("the hot block never missed its setpoint");
    let limited = first_raise(&log, |n, bit| {
        n == AlarmNode::Site && bit == s::POWER_LIMITED
    })
    .expect("the site never reported being limited");

    assert_eq!(failure, 0, "the HVAC failure shows on the first tick");
    assert!(
        derate < miss && miss < limited,
        "order: derate {derate}, miss {miss}, limited {limited}"
    );
    assert!(
        miss - derate >= 60,
        "derate {derate} and miss {miss} are not minutes apart"
    );
    assert!(
        limited - miss >= 60,
        "miss {miss} and limited {limited} are not minutes apart"
    );

    // Only the hot block is in trouble: no other block misses its setpoint.
    assert!(first_raise(&log, |n, bit| {
        matches!(n, AlarmNode::Block { block } if block != HOT_BLOCK) && bit == b::SETPOINT_NOT_MET
    })
    .is_none());
    // And nothing in the chain is a trip until the cells reach 60 C.
    assert!(log.iter().all(|(tick, e)| match e {
        Event::AlarmRaised {
            severity: Severity::Trip,
            bit,
            ..
        } => {
            *bit == r::OVER_TEMP_TRIP && *tick > limited
        }
        _ => true,
    }));
}
