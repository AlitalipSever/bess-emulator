//! The plant partly there: a rack isolated, a block in maintenance, one PCS
//! tripped. Each command changes one thing between ticks; the next tick's
//! models and words say the rest.

/// Ticks a cleared converter spends in its startup sequence.
const STARTUP_TICKS: u64 = 16;

use bess_core::alarms::layout::{block as b, rack as r, site as s};
use bess_core::alarms::{AlarmNode, ResetScope};
use bess_core::kernel::{CommandError, Event};
use bess_core::state::{BlockMode, PcsOpState};
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 14:00 UTC, the warmest afternoon of the replayed year.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;

fn plant(setpoint_w: f64, prepare: impl FnOnce(&mut SiteState)) -> Simulation {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 11, JULY_AFTERNOON_S);
    prepare(&mut state);
    let models = gw01_models(&cfg);
    let mut sim = Simulation::from_state(cfg, models, state);
    sim.set_external_setpoint_w(Some(setpoint_w));
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

fn edge(events: &[Event], node: AlarmNode, bit: u8, up: bool) -> bool {
    events.iter().any(|e| match *e {
        Event::AlarmRaised {
            node: n, bit: x, ..
        } => up && n == node && x == bit,
        Event::AlarmCleared {
            node: n, bit: x, ..
        } => !up && n == node && x == bit,
        Event::PcsStateChanged { .. }
        | Event::FaultApplied(_)
        | Event::SocResnapped { .. }
        | Event::BreakerSequenceChanged { .. }
        | Event::SwitchOperated { .. }
        | Event::CommandRefused { .. }
        | Event::FastForwarded { .. } => false,
    })
}

#[test]
fn an_isolated_rack_stops_at_once_and_its_neighbours_carry_its_share() {
    let mut sim = plant(50.0e6, |_| {});
    run(&mut sim, 60);
    let rack_node = AlarmNode::Rack {
        block: 0,
        container: 0,
        rack: 0,
    };
    let neighbour_before = sim.state().blocks[0].containers[0].racks[1].current_a;
    assert!(sim.state().blocks[0].containers[0].racks[0].current_a > 0.0);

    sim.set_rack_in_service(0, 0, 0, false)
        .expect("rack exists");
    let events = step(&mut sim);
    let racks = &sim.state().blocks[0].containers[0].racks;
    assert!(racks[0].current_a.abs() < f64::EPSILON);
    assert!(racks[1].current_a > neighbour_before);
    assert!(edge(&events, rack_node, r::ISOLATED, true));
    assert!(edge(
        &events,
        AlarmNode::Site,
        s::PARTIAL_AVAILABILITY,
        true
    ));

    sim.set_rack_in_service(0, 0, 0, true).expect("rack exists");
    let events = step(&mut sim);
    assert!(sim.state().blocks[0].containers[0].racks[0].current_a > 0.0);
    assert!(edge(&events, rack_node, r::ISOLATED, false));
    assert!(edge(
        &events,
        AlarmNode::Site,
        s::PARTIAL_AVAILABILITY,
        false
    ));

    let before = sim.state().clone();
    assert_eq!(
        sim.set_rack_in_service(0, 0, 99, false),
        Err(CommandError::NoSuchNode(AlarmNode::Rack {
            block: 0,
            container: 0,
            rack: 99
        }))
    );
    assert!(sim.set_rack_in_service(99, 0, 0, false).is_err());
    assert_eq!(
        sim.set_hvac_failed(0, 9, true),
        Err(CommandError::NoSuchContainer {
            block: 0,
            container: 9
        })
    );
    assert_eq!(sim.state(), &before, "a refused command changed the plant");
}

/// A block under service on a hot afternoon: out of the plant, its
/// converter off, its containers still cooled, its racks still balancing.
#[test]
fn maintenance_takes_a_block_out_and_keeps_it_cool() {
    const BLK: usize = 3;
    let mut sim = plant(100.0e6, |st| {
        for c in &mut st.blocks[BLK].containers {
            c.air_temp_c = 35.0;
            for rack in &mut c.racks {
                // At the top of the window with a wide spread, so the cells
                // sit on the OCV knee where the bleeders switch on.
                rack.soc = 0.95;
                rack.cell_dsoc = 0.08;
            }
        }
    });
    run(&mut sim, 30);
    let avail_before = sim.state().ems.available_discharge_w;
    assert_eq!(sim.state().blocks[BLK].pcs.op_state, PcsOpState::Run);

    sim.set_block_mode(BLK, BlockMode::Maintenance)
        .expect("block exists");
    let events = step(&mut sim);
    let st = sim.state();
    // Maintenance opens the contactors: Off, not an idle Standby.
    assert!(events.contains(&Event::PcsStateChanged {
        block: BLK,
        from: PcsOpState::Run,
        to: PcsOpState::Off,
    }));
    assert!(edge(
        &events,
        AlarmNode::Block { block: BLK },
        b::MAINTENANCE,
        true
    ));
    assert!(edge(
        &events,
        AlarmNode::Site,
        s::PARTIAL_AVAILABILITY,
        true
    ));
    let lost_w = avail_before - st.ems.available_discharge_w;
    assert!(
        (lost_w - 5.0e6).abs() < 0.1e6,
        "availability fell by {lost_w} W, not one block"
    );
    // Every other converter is running, and the maintained one is locked
    // out, so nobody pays a standby tare.
    assert!(st.aux.pcs_standby_w.abs() < f64::EPSILON);

    let mut cooled = false;
    let mut balanced = false;
    for _ in 0..600 {
        step(&mut sim);
        let blk = &sim.state().blocks[BLK];
        assert!(blk.pcs.p_ac_w.abs() < f64::EPSILON);
        cooled |= blk.containers.iter().any(|c| c.hvac.electrical_w > 0.0);
        balanced |= blk
            .containers
            .iter()
            .flat_map(|c| c.racks.iter())
            .any(|r| r.balancing_active);
    }
    assert!(cooled, "the maintained block's HVAC never ran");
    assert!(balanced, "the maintained block's racks never balanced");

    sim.set_block_mode(BLK, BlockMode::InService)
        .expect("block exists");
    let events = step(&mut sim);
    // Handed back, the converter starts its sequence; it runs 16 s later.
    assert!(events.contains(&Event::PcsStateChanged {
        block: BLK,
        from: PcsOpState::Off,
        to: PcsOpState::Precharge,
    }));
    assert!(edge(
        &events,
        AlarmNode::Block { block: BLK },
        b::MAINTENANCE,
        false
    ));
    // Partial availability clears when the converter is back in the
    // connected set, at the end of its startup sequence.
    let mut later = Vec::new();
    for _ in 0..STARTUP_TICKS {
        later.extend(step(&mut sim));
    }
    assert_eq!(sim.state().blocks[BLK].pcs.op_state, PcsOpState::Run);
    assert!(edge(
        &later,
        AlarmNode::Site,
        s::PARTIAL_AVAILABILITY,
        false
    ));
    assert_eq!(
        sim.set_block_mode(20, BlockMode::Maintenance),
        Err(CommandError::NoSuchNode(AlarmNode::Block { block: 20 }))
    );
}

/// One converter trips: the others carry the site, and a block reset
/// brings it back without the site sequencer ever being involved.
#[test]
fn a_tripped_pcs_leaves_the_rest_delivering() {
    const BLK: usize = 5;
    let mut sim = plant(50.0e6, |_| {});
    // Through the ramp to the setpoint, half the site's rating.
    run(&mut sim, 660);
    let avail_before = sim.state().ems.available_discharge_w;

    assert_eq!(sim.trip_pcs(BLK), Ok(true));
    assert_eq!(sim.trip_pcs(BLK), Ok(false));
    let events = step(&mut sim);
    let st = sim.state();
    assert_eq!(
        events.first(),
        Some(&Event::PcsStateChanged {
            block: BLK,
            from: PcsOpState::Run,
            to: PcsOpState::Fault,
        })
    );
    assert!(edge(
        &events,
        AlarmNode::Block { block: BLK },
        b::PCS_FAULT,
        true
    ));
    let lost_w = avail_before - st.ems.available_discharge_w;
    assert!((lost_w - 5.0e6).abs() < 0.1e6, "lost {lost_w} W");
    // The trip tick loses the tripped block's share; the others take it
    // up at the gradient, so the site is back at its setpoint half a
    // minute later.
    let delivered_w: f64 = st.blocks.iter().map(|blk| blk.pcs.p_ac_w).sum();
    assert!(
        (delivered_w - 47.5e6).abs() < 0.5e6,
        "delivered {delivered_w} W on the trip tick"
    );
    run(&mut sim, 30);
    let delivered_w: f64 = sim.state().blocks.iter().map(|blk| blk.pcs.p_ac_w).sum();
    assert!(
        (delivered_w - 50.0e6).abs() < 0.5e6,
        "delivered {delivered_w} W after the others ramped"
    );

    sim.reset_alarms(ResetScope::Block(BLK))
        .expect("block scope");
    let events = step(&mut sim);
    assert!(edge(
        &events,
        AlarmNode::Block { block: BLK },
        b::PCS_FAULT,
        false
    ));
    // Out of fault into the startup sequence, running once it is through.
    assert_eq!(sim.state().blocks[BLK].pcs.op_state, PcsOpState::Precharge);
    assert_eq!(sim.state().ems.blocks_released, 20);
    run(&mut sim, STARTUP_TICKS);
    assert_eq!(sim.state().blocks[BLK].pcs.op_state, PcsOpState::Run);
    assert_eq!(
        sim.trip_pcs(20),
        Err(CommandError::NoSuchNode(AlarmNode::Block { block: 20 }))
    );
}
