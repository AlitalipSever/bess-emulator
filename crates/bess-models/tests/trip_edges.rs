//! The trip's corners, each a sentence of the design held to the plant: a
//! block reset during a trip, a second trip in the middle of the return,
//! and a block that sits out the whole story in maintenance.

use bess_core::alarms::layout::{block as b, site as s};
use bess_core::alarms::{has_bit, AlarmNode, ResetScope};
use bess_core::kernel::Event;
use bess_core::state::{BlockMode, BreakerState, PcsOpState};
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 14:00 UTC, the warmest afternoon of the replayed year.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;
const BLOCKS: usize = 20;
const SPACING_TICKS: u64 = 30;
/// Ticks from a block's release to its first running tick: the converter's
/// startup sequence (precharge, contactor close, synchronization).
const STARTUP_TICKS: u64 = 16;

fn busy_plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, JULY_AFTERNOON_S);
    sim.set_external_setpoint_w(Some(100.0e6));
    run(&mut sim, 60);
    sim
}

fn step(sim: &mut Simulation) -> Vec<Event> {
    let inputs = gw01_weather().inputs_at(sim.unix_time_s());
    sim.step(&inputs).to_vec()
}

fn run(sim: &mut Simulation, ticks: u64) {
    for _ in 0..ticks {
        step(sim);
    }
}

/// The tick, counted from the call, on which each block first runs.
fn first_runs(sim: &mut Simulation, ticks: u64) -> [Option<u64>; BLOCKS] {
    let mut first = [None; BLOCKS];
    for tick in 0..ticks {
        step(sim);
        for (k, blk) in sim.state().blocks.iter().enumerate() {
            if blk.pcs.op_state == PcsOpState::Run && first[k].is_none() {
                first[k] = Some(tick);
            }
        }
    }
    first
}

fn transitions(events: &[Event], from: PcsOpState, to: PcsOpState) -> usize {
    events
        .iter()
        .filter(|e| matches!(**e, Event::PcsStateChanged { from: f, to: t, .. } if f == from && t == to))
        .count()
}

/// Design D9: a block reset during a site trip takes that PCS out of fault
/// and nothing more. The breaker is open and the block is not released, so
/// it cannot run; `pcs_fault` clears while the trip and the breaker stay.
#[test]
fn a_block_reset_during_a_trip_clears_the_block_and_nothing_else() {
    const BLK: usize = 3;
    let mut sim = busy_plant();
    assert!(sim.trip_protection());
    step(&mut sim);

    let still = sim
        .reset_alarms(ResetScope::Block(BLK))
        .expect("block scope");
    assert_eq!(still, Vec::new(), "a block reset left a cause behind");
    let events = step(&mut sim);
    assert!(events.contains(&Event::PcsStateChanged {
        block: BLK,
        from: PcsOpState::Fault,
        to: PcsOpState::Off,
    }));
    assert_eq!(transitions(&events, PcsOpState::Fault, PcsOpState::Off), 1);

    for _ in 0..60 {
        step(&mut sim);
        let st = sim.state();
        assert_eq!(st.substation.hv_breaker, BreakerState::Open);
        assert_eq!(st.ems.blocks_released, 0);
        assert_eq!(st.blocks[BLK].pcs.op_state, PcsOpState::Off);
        assert!(st.substation.poi_active_power_w.abs() < f64::EPSILON);
        assert!(!has_bit(st.blocks[BLK].alarm_bits, b::PCS_FAULT));
        assert!(has_bit(st.blocks[BLK + 1].alarm_bits, b::PCS_FAULT));
        assert!(has_bit(st.alarm_bits, s::PROTECTION_TRIP));
        assert!(has_bit(st.alarm_bits, s::HV_BREAKER_OPEN));
    }
}

/// A second trip in the middle of the return starts the sequence over: the
/// blocks already back fall from `Run`, the one mid-startup from its
/// startup state, the rest from `Off`, and the next reset releases block 0
/// first again, on the same spacing plus the startup.
#[test]
fn a_second_trip_mid_return_starts_the_return_over() {
    let mut sim = busy_plant();
    sim.trip_protection();
    step(&mut sim);
    sim.reset_alarms(ResetScope::Site).expect("site scope");
    run(&mut sim, 100);
    let back = sim.state().ems.blocks_released;
    assert_eq!(back, 4, "blocks 0 to 3 are released 100 s after the reset");

    assert!(sim.trip_protection(), "a reset site trips again");
    let events = step(&mut sim);
    // Blocks 0 to 2 are running (released at 0, 30, 60 s, running 16 s
    // later); block 3, released at 90 s, is nine seconds into precharge.
    assert_eq!(
        transitions(&events, PcsOpState::Run, PcsOpState::Fault),
        back - 1
    );
    assert_eq!(
        transitions(&events, PcsOpState::Precharge, PcsOpState::Fault),
        1
    );
    assert_eq!(
        transitions(&events, PcsOpState::Off, PcsOpState::Fault),
        BLOCKS - back
    );
    assert_eq!(sim.state().ems.blocks_released, 0);
    assert!(sim.state().substation.poi_active_power_w.abs() < f64::EPSILON);

    sim.reset_alarms(ResetScope::Site).expect("site scope");
    let first = first_runs(&mut sim, BLOCKS as u64 * SPACING_TICKS + 30);
    let want: Vec<Option<u64>> = (0..BLOCKS as u64)
        .map(|k| Some(k * SPACING_TICKS + STARTUP_TICKS))
        .collect();
    assert_eq!(first.to_vec(), want);
}

/// A block in maintenance when protection trips goes to fault with the rest,
/// comes out of fault with the reset, and sits out the return: its slot is
/// released on time, the sequencer releases positions, not power, and the
/// blocks after it keep their spacing. It stays in maintenance throughout
/// and starts its sequence the tick it is handed back.
#[test]
fn a_maintained_block_sits_out_the_trip_and_the_return() {
    const BLK: usize = 2;
    let mut sim = busy_plant();
    sim.set_block_mode(BLK, BlockMode::Maintenance)
        .expect("block exists");
    run(&mut sim, 5);
    assert_eq!(sim.state().blocks[BLK].pcs.op_state, PcsOpState::Off);

    sim.trip_protection();
    let events = step(&mut sim);
    assert!(events.contains(&Event::PcsStateChanged {
        block: BLK,
        from: PcsOpState::Off,
        to: PcsOpState::Fault,
    }));
    sim.reset_alarms(ResetScope::Site).expect("site scope");
    let first = first_runs(&mut sim, BLOCKS as u64 * SPACING_TICKS + 30);

    for (k, got) in first.iter().enumerate() {
        let want = (k != BLK).then(|| k as u64 * SPACING_TICKS + STARTUP_TICKS);
        assert_eq!(*got, want, "block {k}");
    }
    let st = sim.state();
    assert_eq!(st.blocks[BLK].mode, BlockMode::Maintenance);
    assert_eq!(st.blocks[BLK].pcs.op_state, PcsOpState::Off);
    assert!(has_bit(st.blocks[BLK].alarm_bits, b::MAINTENANCE));
    assert!(has_bit(st.alarm_bits, s::PARTIAL_AVAILABILITY));
    assert!(!has_bit(st.alarm_bits, s::PROTECTION_TRIP));
    let lost_w = 100.0e6 - st.ems.available_discharge_w;
    assert!(
        (lost_w - 5.0e6).abs() < 0.1e6,
        "available short by {lost_w} W"
    );

    sim.set_block_mode(BLK, BlockMode::InService)
        .expect("block exists");
    let events = step(&mut sim);
    assert!(events.contains(&Event::PcsStateChanged {
        block: BLK,
        from: PcsOpState::Off,
        to: PcsOpState::Precharge,
    }));
    for _ in 0..STARTUP_TICKS {
        step(&mut sim);
    }
    assert_eq!(sim.state().blocks[BLK].pcs.op_state, PcsOpState::Run);
    assert!(events.iter().any(|e| matches!(
        *e,
        Event::AlarmCleared { node: AlarmNode::Block { block: BLK }, bit, .. } if bit == b::MAINTENANCE
    )));
}
