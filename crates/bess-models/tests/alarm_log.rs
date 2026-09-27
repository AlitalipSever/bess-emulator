//! The event log and the alarm words never disagree, and an operator reset
//! does what an HMI reset button does.

use std::collections::HashMap;

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::{has_bit, AlarmNode, ResetError, ResetScope};
use bess_core::kernel::Event;
use bess_core::state::PcsOpState;
use bess_core::{checkpoint, PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 14:00 UTC.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;

fn sim_from(state: SiteState) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::from_state(cfg, models, state)
}

/// Every word in the tree, keyed by node.
fn words(state: &SiteState) -> HashMap<AlarmNode, u32> {
    let mut out = HashMap::new();
    out.insert(AlarmNode::Site, state.alarm_bits);
    for (bi, blk) in state.blocks.iter().enumerate() {
        out.insert(AlarmNode::Block { block: bi }, blk.alarm_bits);
        for (ci, c) in blk.containers.iter().enumerate() {
            for (ri, rack) in c.racks.iter().enumerate() {
                let node = AlarmNode::Rack {
                    block: bi,
                    container: ci,
                    rack: ri,
                };
                out.insert(node, rack.alarm_bits);
            }
        }
    }
    out
}

/// Replay events onto a set of words; panics on a raise of a set bit or a
/// clear of a clear one, which is the log contradicting itself.
fn apply(words: &mut HashMap<AlarmNode, u32>, events: &[Event]) {
    for e in events {
        match *e {
            Event::AlarmRaised { node, bit, .. } => {
                let w = words.entry(node).or_insert(0);
                assert!(!has_bit(*w, bit), "{node:?} bit {bit} raised twice");
                *w |= 1 << bit;
            }
            Event::AlarmCleared { node, bit, .. } => {
                let w = words.entry(node).or_insert(0);
                assert!(has_bit(*w, bit), "{node:?} bit {bit} cleared while clear");
                *w &= !(1 << bit);
            }
            Event::PcsStateChanged { .. } => {}
        }
    }
}

/// A hot block with failed HVAC, full discharge, and a site-wide reset in
/// the middle: every tick, the words rebuilt from the event log alone match
/// the words in the tree, and the log's count matches what was handed out.
#[test]
fn the_log_rebuilds_every_word_every_tick() {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, JULY_AFTERNOON_S);
    for c in &mut state.blocks[2].containers {
        c.air_temp_c = 45.0;
        for rack in &mut c.racks {
            rack.cell_temp_c = 58.0;
        }
    }
    let mut sim = sim_from(state);
    sim.set_hvac_failed(2, 0, true);
    sim.set_hvac_failed(2, 1, true);
    sim.set_external_setpoint_w(Some(100.0e6));
    let weather = gw01_weather();

    let mut rebuilt = words(sim.state());
    let mut handed_out = 0u64;
    let mut raises = 0;
    for tick in 0..2400u64 {
        if tick == 1200 {
            sim.reset_alarms(ResetScope::Site).expect("site scope");
        }
        let events = sim.step(&weather.inputs_at(sim.unix_time_s())).to_vec();
        raises += events
            .iter()
            .filter(|e| matches!(e, Event::AlarmRaised { .. }))
            .count();
        handed_out += events.len() as u64;
        apply(&mut rebuilt, &events);
        assert_eq!(
            rebuilt,
            words(sim.state()),
            "log and tree disagree at tick {tick}"
        );
    }
    assert!(
        raises > 20,
        "only {raises} raises: the fixture is not exercising the tree"
    );
    assert_eq!(sim.state().event_log.count, handed_out);
}

/// Trip on, reset while the cause is still there: the reset reports it and
/// the next tick raises it again. Cause gone: the bit stays latched until a
/// reset, which then clears it for good.
#[test]
fn a_reset_clears_only_what_has_cooled() {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, JULY_AFTERNOON_S);
    state.blocks[0].containers[0].racks[0].cell_temp_c = 62.0;
    let mut sim = sim_from(state);
    sim.set_external_setpoint_w(Some(0.0));
    let weather = gw01_weather();
    let rack0 = AlarmNode::Rack {
        block: 0,
        container: 0,
        rack: 0,
    };
    let trip_of = |sim: &Simulation| {
        has_bit(
            sim.state().blocks[0].containers[0].racks[0].alarm_bits,
            r::OVER_TEMP_TRIP,
        )
    };

    sim.step(&weather.inputs_at(sim.unix_time_s()));
    assert!(trip_of(&sim));

    let still = sim
        .reset_alarms(ResetScope::Rack {
            block: 0,
            container: 0,
            rack: 0,
        })
        .expect("rack 0 exists");
    assert_eq!(still, vec![(rack0, r::OVER_TEMP_TRIP)]);
    assert!(!trip_of(&sim));
    let events = sim.step(&weather.inputs_at(sim.unix_time_s())).to_vec();
    assert!(events.iter().any(|e| matches!(e,
        Event::AlarmCleared { node, bit, .. } if *node == rack0 && *bit == r::OVER_TEMP_TRIP)));
    assert!(events.iter().any(|e| matches!(e,
        Event::AlarmRaised { node, bit, .. } if *node == rack0 && *bit == r::OVER_TEMP_TRIP)));

    // The idle rack cools through its own air in a few minutes.
    for _ in 0..1800 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
    assert!(sim.state().blocks[0].containers[0].racks[0].cell_temp_c < 59.0);
    assert!(trip_of(&sim), "the trip cleared without a reset");

    let still = sim
        .reset_alarms(ResetScope::Block(0))
        .expect("block 0 exists");
    assert!(still.is_empty());
    sim.step(&weather.inputs_at(sim.unix_time_s()));
    assert!(!trip_of(&sim));
}

/// A PCS in fault stays there tick after tick, shows on the block and
/// site words, and leaves only through a reset.
#[test]
fn a_faulted_pcs_waits_for_the_operator() {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, JULY_AFTERNOON_S);
    state.blocks[3].pcs.op_state = PcsOpState::Fault;
    let mut sim = sim_from(state);
    let weather = gw01_weather();
    for _ in 0..60 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
    let st = sim.state();
    assert_eq!(st.blocks[3].pcs.op_state, PcsOpState::Fault);
    assert!(has_bit(st.blocks[3].alarm_bits, b::PCS_FAULT));
    assert!(has_bit(st.alarm_bits, s::PARTIAL_AVAILABILITY));

    sim.reset_alarms(ResetScope::Block(3))
        .expect("block 3 exists");
    let events = sim.step(&weather.inputs_at(sim.unix_time_s())).to_vec();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::PcsStateChanged {
                block: 3,
                from: PcsOpState::Fault,
                to: PcsOpState::Standby
            }
        )),
        "the PCS left fault without saying so: {events:?}"
    );
    let st = sim.state();
    assert_ne!(st.blocks[3].pcs.op_state, PcsOpState::Fault);
    assert!(!has_bit(st.blocks[3].alarm_bits, b::PCS_FAULT));
    assert!(!has_bit(st.alarm_bits, s::PARTIAL_AVAILABILITY));
}

/// A reset refuses a node the site does not have, and changes nothing.
#[test]
fn a_reset_for_a_node_that_does_not_exist_is_refused() {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, JULY_AFTERNOON_S);
    state.blocks[0].pcs.op_state = PcsOpState::Fault;
    let mut sim = sim_from(state);
    let before = sim.state().clone();
    for scope in [
        ResetScope::Block(99),
        ResetScope::Rack {
            block: 0,
            container: 7,
            rack: 0,
        },
        ResetScope::Rack {
            block: 0,
            container: 0,
            rack: 99,
        },
    ] {
        assert_eq!(sim.reset_alarms(scope), Err(ResetError::NoSuchNode(scope)));
    }
    assert_eq!(*sim.state(), before);
}

/// A checkpoint taken between a reset and the next tick keeps the clears:
/// the resumed run hands them out, its words still rebuild from the log,
/// and the counter matches what went out.
#[test]
fn a_reset_survives_a_checkpoint_before_the_next_tick() {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, JULY_AFTERNOON_S);
    state.blocks[0].containers[0].racks[0].cell_temp_c = 62.0;
    state.blocks[1].pcs.op_state = PcsOpState::Fault;
    let mut sim = sim_from(state);
    let weather = gw01_weather();
    let mut rebuilt = words(sim.state());
    let mut handed_out = 0u64;
    for _ in 0..600 {
        let events = sim.step(&weather.inputs_at(sim.unix_time_s())).to_vec();
        handed_out += events.len() as u64;
        apply(&mut rebuilt, &events);
    }
    sim.reset_alarms(ResetScope::Site).expect("site scope");

    let bytes = checkpoint::save(sim.state()).expect("save");
    let mut resumed = sim_from(checkpoint::load(&bytes).expect("load"));
    let events = resumed
        .step(&weather.inputs_at(resumed.unix_time_s()))
        .to_vec();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::AlarmCleared { .. })),
        "the resumed run lost the reset's clears"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::PcsStateChanged {
            from: PcsOpState::Fault,
            ..
        }
    )));
    handed_out += events.len() as u64;
    apply(&mut rebuilt, &events);
    assert_eq!(rebuilt, words(resumed.state()));
    assert_eq!(resumed.state().event_log.count, handed_out);
}
