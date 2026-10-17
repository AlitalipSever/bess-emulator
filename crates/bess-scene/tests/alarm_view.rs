//! The alarm view against the kernel, on the plant M2 phase 2 built its
//! causal chain on: a July afternoon, both HVAC units of one block failed,
//! the site asked for full discharge.
//!
//! Two claims. The summary, derived from the words, and the journal, kept
//! from the events, agree: every bit the summary shows has a raise in the
//! journal and no clear after it. And the journal reads the chain in the
//! order the physics produces it, numbered the kernel's way.

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::AlarmNode;
use bess_core::kernel::Event;
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};
use bess_scene::alarms::{self, Journal};

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
    sim.set_hvac_failed(HOT_BLOCK, 0, true).expect("container");
    sim.set_hvac_failed(HOT_BLOCK, 1, true).expect("container");
    sim.set_external_setpoint_w(Some(100.0e6));
    sim
}

/// Seq of the first raise matching `want`, read from the journal.
fn first_raise(journal: &Journal, want: impl Fn(AlarmNode, u8) -> bool) -> Option<u64> {
    journal
        .iter_newest_first()
        .filter(|e| matches!(e.event, Event::AlarmRaised { node, bit, .. } if want(node, bit)))
        .map(|e| e.seq)
        .min()
}

#[test]
fn the_summary_and_the_journal_agree_and_read_the_chain_in_order() {
    let mut sim = hot_plant();
    let weather = gw01_weather();
    let mut journal = Journal::new();
    for _ in 0..3600 {
        let count_before = sim.state().event_log.count;
        let ts = sim.unix_time_s() + 1;
        let events = sim.step(&weather.inputs_at(sim.unix_time_s()));
        journal.record(events, count_before + events.len() as u64, ts);

        // Checked every tick, not only at the end: an hour is 3600 chances
        // for the two halves to disagree.
        for alarm in alarms::active(sim.state()) {
            assert!(
                journal.raised_at(alarm.node, alarm.bit).is_some(),
                "{} {} is set but the journal has no open raise",
                alarm.node.path(),
                alarm.name()
            );
        }
    }

    // Nothing wrapped, and the numbering is the kernel's, gapless.
    let count = sim.state().event_log.count;
    assert_eq!(journal.len() as u64, count);
    assert_eq!(journal.dropped(), 0);
    let seqs: Vec<u64> = journal.iter_newest_first().map(|e| e.seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] == w[1] + 1));

    let failure = first_raise(&journal, |n, bit| {
        n == AlarmNode::Block { block: HOT_BLOCK } && bit == b::HVAC_FAILURE
    })
    .expect("hvac_failure raised");
    let derate = first_raise(&journal, |n, bit| {
        matches!(
            n,
            AlarmNode::Rack {
                block: HOT_BLOCK,
                ..
            }
        ) && bit == r::DERATE_ACTIVE
    })
    .expect("derate_active raised");
    let miss = first_raise(&journal, |n, bit| {
        n == AlarmNode::Block { block: HOT_BLOCK } && bit == b::SETPOINT_NOT_MET
    })
    .expect("setpoint_not_met raised");
    let limited = first_raise(&journal, |n, bit| {
        n == AlarmNode::Site && bit == s::POWER_LIMITED
    })
    .expect("power_limited raised");
    assert!(
        failure < derate && derate < miss && miss < limited,
        "chain out of order: failure {failure}, derate {derate}, miss {miss}, limited {limited}"
    );

    // The summary at the end shows the chain's head, trips first.
    let names: Vec<String> = alarms::active(sim.state())
        .iter()
        .map(|a| a.name())
        .collect();
    assert!(names.contains(&"block.hvac_failure".to_owned()));
    assert!(names.contains(&"site.power_limited".to_owned()));
}
