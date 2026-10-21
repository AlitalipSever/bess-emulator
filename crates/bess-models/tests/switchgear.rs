//! The substation's switchgear: the interlocks refuse every forbidden
//! move, the breaker closes through its sequence and the return waits for
//! it, and an opened site comes back the way a tripped one does.

use bess_core::alarms::layout::site as s;
use bess_core::alarms::{has_bit, ResetScope};
use bess_core::kernel::{BreakerSequence, CommandError, Event, Switch};
use bess_core::state::{BreakerState, PcsOpState, SwitchPosition};
use bess_core::{checkpoint, PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

const START_UNIX_S: i64 = 1_767_225_600 + 68 * 86_400 + 2 * 3600;

fn plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 11, START_UNIX_S);
    sim.set_external_setpoint_w(Some(20.0e6));
    sim
}

fn step(sim: &mut Simulation) -> Vec<Event> {
    let inputs = gw01_weather().inputs_at(sim.unix_time_s());
    sim.step(&inputs).to_vec()
}

fn interlocked(r: Result<bool, CommandError>) -> bool {
    matches!(r, Err(CommandError::Interlocked(_)))
}

/// Under a closed breaker nothing else moves; on an isolated bus the
/// earthing switch closes and then the disconnectors cannot; the breaker
/// never closes onto an open disconnector or a closed earthing switch.
#[test]
fn the_interlocks_refuse_every_forbidden_move() {
    let mut sim = plant();
    step(&mut sim);
    assert!(interlocked(sim.set_disconnectors(false)));
    assert!(interlocked(sim.set_earthing_switch(true)));
    assert_eq!(sim.close_hv_breaker(), Ok(false));

    assert!(sim.open_hv_breaker());
    assert!(!sim.open_hv_breaker());
    assert!(
        interlocked(sim.set_earthing_switch(true)),
        "earthed under closed disconnectors"
    );
    assert_eq!(sim.set_disconnectors(false), Ok(true));
    assert_eq!(sim.set_disconnectors(false), Ok(false));
    assert!(
        interlocked(sim.close_hv_breaker()),
        "closed onto open disconnectors"
    );
    assert_eq!(sim.set_earthing_switch(true), Ok(true));
    assert!(
        interlocked(sim.set_disconnectors(true)),
        "disconnectors closed onto earth"
    );
    assert!(
        interlocked(sim.close_hv_breaker()),
        "closed onto the earthing switch"
    );
    assert_eq!(sim.set_earthing_switch(false), Ok(true));
    assert_eq!(sim.set_disconnectors(true), Ok(true));
    assert_eq!(sim.close_hv_breaker(), Ok(true));
    assert_eq!(
        sim.close_hv_breaker(),
        Ok(false),
        "a second close while closing"
    );
    assert!(
        interlocked(sim.set_disconnectors(false)),
        "moved under a closing breaker"
    );

    let st = sim.state();
    assert_eq!(st.substation.disconnectors, SwitchPosition::Closed);
    assert_eq!(st.substation.earthing_switch, SwitchPosition::Open);
    assert_eq!(st.substation.sequence, BreakerSequence::Closing);
}

/// An operator open disconnects the site at once and sends every
/// converter to `Off`; the close lands on the next tick, the return
/// starts from it, and the journal carries the sequence and the switches.
#[test]
fn an_open_and_a_close_are_a_sequence_the_log_shows() {
    let mut sim = plant();
    for _ in 0..700 {
        step(&mut sim);
    }
    assert!(sim
        .state()
        .blocks
        .iter()
        .all(|b| b.pcs.op_state == PcsOpState::Run));

    assert!(sim.open_hv_breaker());
    let events = step(&mut sim);
    let st = sim.state();
    assert_eq!(st.substation.hv_breaker, BreakerState::Open);
    assert!(has_bit(st.alarm_bits, s::HV_BREAKER_OPEN));
    assert!(st.blocks.iter().all(|b| b.pcs.op_state == PcsOpState::Off));
    assert!(st.substation.poi_active_power_w.abs() < f64::EPSILON);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                Event::PcsStateChanged {
                    to: PcsOpState::Off,
                    ..
                }
            ))
            .count(),
        20
    );

    sim.set_disconnectors(false).expect("open");
    sim.set_earthing_switch(true).expect("earth");
    let events = step(&mut sim);
    assert!(events.contains(&Event::SwitchOperated {
        switch: Switch::Disconnectors,
        closed: false
    }));
    assert!(events.contains(&Event::SwitchOperated {
        switch: Switch::EarthingSwitch,
        closed: true
    }));
    sim.set_earthing_switch(false).expect("unearth");
    sim.set_disconnectors(true).expect("close");
    step(&mut sim);

    assert_eq!(sim.close_hv_breaker(), Ok(true));
    let events = step(&mut sim);
    assert!(events.contains(&Event::BreakerSequenceChanged {
        from: BreakerSequence::Idle,
        to: BreakerSequence::Closing
    }));
    // The breaker closes on this tick: the sequence ends, the site is
    // connected, and block 0 is released into its startup.
    let st = sim.state();
    assert_eq!(st.substation.hv_breaker, BreakerState::Closed);
    assert_eq!(st.substation.sequence, BreakerSequence::Idle);
    assert!(events.contains(&Event::BreakerSequenceChanged {
        from: BreakerSequence::Closing,
        to: BreakerSequence::Idle
    }));
    assert!(!has_bit(st.alarm_bits, s::HV_BREAKER_OPEN));
    assert_eq!(st.ems.blocks_released, 1);
    assert_eq!(st.blocks[0].pcs.op_state, PcsOpState::Precharge);
    for _ in 0..16 {
        step(&mut sim);
    }
    assert!(sim.state().blocks[0].pcs.op_state.is_connected());
}

/// A site reset on a tripped site closes the breaker through the sequence
/// (the trip is cleared first, so the interlock on it passes), and a
/// reset with the bus earthed clears the trip but leaves the breaker open.
#[test]
fn the_reset_closes_through_the_sequence_unless_interlocked() {
    let mut sim = plant();
    step(&mut sim);
    assert!(sim.trip_protection());
    step(&mut sim);
    assert!(
        interlocked(sim.close_hv_breaker()),
        "closed under a latched trip"
    );
    sim.reset_alarms(ResetScope::Site).expect("site");
    assert_eq!(sim.state().substation.sequence, BreakerSequence::Closing);
    step(&mut sim);
    assert_eq!(sim.state().substation.hv_breaker, BreakerState::Closed);

    let mut sim = plant();
    step(&mut sim);
    sim.trip_protection();
    sim.set_disconnectors(false).expect("open");
    sim.set_earthing_switch(true).expect("earth");
    sim.reset_alarms(ResetScope::Site).expect("site");
    step(&mut sim);
    let st = sim.state();
    assert!(!has_bit(st.alarm_bits, s::PROTECTION_TRIP));
    assert_eq!(st.substation.hv_breaker, BreakerState::Open);
    assert_eq!(st.substation.sequence, BreakerSequence::Idle);
}

/// The positions and the sequence ride in the checkpoint: a run saved
/// mid-close resumes and closes on the same tick.
#[test]
fn a_closing_breaker_survives_a_checkpoint() {
    let mut sim = plant();
    step(&mut sim);
    sim.open_hv_breaker();
    step(&mut sim);
    sim.close_hv_breaker().expect("close");
    let bytes = checkpoint::save(sim.state()).expect("save");
    let cfg = PlantConfig::gw01();
    let mut resumed = Simulation::from_state(
        cfg.clone(),
        gw01_models(&cfg),
        checkpoint::load(&bytes).expect("load"),
    );
    resumed.set_external_setpoint_w(Some(20.0e6));
    assert_eq!(
        resumed.state().substation.sequence,
        BreakerSequence::Closing
    );
    step(&mut sim);
    step(&mut resumed);
    assert_eq!(sim.state(), resumed.state());
    assert_eq!(resumed.state().substation.hv_breaker, BreakerState::Closed);
}
