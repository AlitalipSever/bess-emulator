//! The converter's startup sequence: how a released block gets from its
//! contactors open to converting (M3 phase 2).
//!
//! The plant controller's return (`restart.rs`) says *when* a block is
//! cleared; this says what the converter does with the clearance. A
//! cleared block walks `Off -> Precharge -> ContactorClose -> Synchronize
//! -> Ramp`, each transitional state for its published duration, and only
//! from `Ramp` on does it take a share of the site target. A block whose
//! clearance is withdrawn mid-sequence (a second trip, maintenance) goes
//! back to `Off`; a trip from any state goes to `Fault` through the
//! command that trips it, not through here.
//!
//! Like the return policy, the sequence is the kernel's, not a layer
//! model's: it is the observable order of a start, and the durations are
//! the plant's parameters.

use crate::state::{PcsOpState, PcsState};

use super::Event;

/// Durations of the transitional startup states, s.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupSequence {
    /// DC-link precharge through the precharge resistor.
    pub precharge_s: f64,
    /// Main contactor close and check.
    pub contactor_close_s: f64,
    /// Grid synchronization before the gate drives enable.
    pub synchronize_s: f64,
}

impl Default for StartupSequence {
    /// GW-01 defaults: 10 s precharge, 1 s contactor close, 5 s
    /// synchronization, 16 s in all. Labeled estimates in CALIBRATION.md:
    /// utility converter manuals state the sequence and give seconds for
    /// each step, but none found public gives the three for one unit, so
    /// the figures are the order of magnitude the manuals describe, not a
    /// cited column.
    fn default() -> Self {
        Self {
            precharge_s: 10.0,
            contactor_close_s: 1.0,
            synchronize_s: 5.0,
        }
    }
}

impl StartupSequence {
    /// Seconds from clearance to the first tick in `Ramp`.
    pub fn total_s(&self) -> f64 {
        self.precharge_s + self.contactor_close_s + self.synchronize_s
    }

    /// Advance one converter by `dt_s` given whether its block is cleared
    /// to run (in service, released, site connected). Pushes the
    /// transition it made, if any.
    pub(super) fn advance(
        &self,
        pcs: &mut PcsState,
        block: usize,
        cleared: bool,
        dt_s: f64,
        events: &mut Vec<Event>,
    ) {
        let from = pcs.op_state;
        let next = match from {
            PcsOpState::Off if cleared => Some(PcsOpState::Precharge),
            PcsOpState::Precharge | PcsOpState::ContactorClose | PcsOpState::Synchronize
                if !cleared =>
            {
                Some(PcsOpState::Off)
            }
            PcsOpState::Precharge => after(pcs, self.precharge_s, dt_s, PcsOpState::ContactorClose),
            PcsOpState::ContactorClose => {
                after(pcs, self.contactor_close_s, dt_s, PcsOpState::Synchronize)
            }
            PcsOpState::Synchronize => after(pcs, self.synchronize_s, dt_s, PcsOpState::Ramp),
            _ => None,
        };
        if let Some(to) = next {
            pcs.op_state = to;
            pcs.state_elapsed_s = 0.0;
            events.push(Event::PcsStateChanged { block, from, to });
        }
    }
}

/// Count the state's time and name the next one when it is up.
fn after(pcs: &mut PcsState, duration_s: f64, dt_s: f64, then: PcsOpState) -> Option<PcsOpState> {
    pcs.state_elapsed_s += dt_s;
    (pcs.state_elapsed_s >= duration_s).then_some(then)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcs(state: PcsOpState) -> PcsState {
        PcsState {
            op_state: state,
            p_ac_setpoint_w: 0.0,
            p_ac_w: 0.0,
            p_dc_w: 0.0,
            loss_w: 0.0,
            dc_voltage_v: 1_331.2,
            state_elapsed_s: 0.0,
        }
    }

    /// The walk from clearance to `Ramp` takes the three durations, to the
    /// tick, through the three states in order.
    #[test]
    fn a_cleared_converter_reaches_ramp_after_the_published_durations() {
        let seq = StartupSequence::default();
        let mut p = pcs(PcsOpState::Off);
        let mut events = Vec::new();
        let mut path = Vec::new();
        for tick in 0..=20u64 {
            seq.advance(&mut p, 0, true, 1.0, &mut events);
            if let Some(Event::PcsStateChanged { to, .. }) = events.last() {
                if path.last().map(|(_, s)| s) != Some(to) {
                    path.push((tick, *to));
                }
            }
        }
        assert_eq!(
            path,
            vec![
                (0, PcsOpState::Precharge),
                (10, PcsOpState::ContactorClose),
                (11, PcsOpState::Synchronize),
                (16, PcsOpState::Ramp),
            ]
        );
        assert_eq!(seq.total_s(), 16.0);
    }

    #[test]
    fn withdrawing_the_clearance_mid_sequence_returns_to_off() {
        let seq = StartupSequence::default();
        let mut p = pcs(PcsOpState::Off);
        let mut events = Vec::new();
        for _ in 0..5 {
            seq.advance(&mut p, 0, true, 1.0, &mut events);
        }
        assert_eq!(p.op_state, PcsOpState::Precharge);
        seq.advance(&mut p, 0, false, 1.0, &mut events);
        assert_eq!(p.op_state, PcsOpState::Off);
        assert_eq!(p.state_elapsed_s, 0.0);
    }

    /// Connected and faulted states are not the sequence's business.
    #[test]
    fn connected_and_faulted_states_are_left_alone() {
        let seq = StartupSequence::default();
        let mut events = Vec::new();
        for s in [
            PcsOpState::Standby,
            PcsOpState::Run,
            PcsOpState::Ramp,
            PcsOpState::Fault,
        ] {
            let mut p = pcs(s);
            seq.advance(&mut p, 0, true, 1.0, &mut events);
            seq.advance(&mut p, 0, false, 1.0, &mut events);
            assert_eq!(p.op_state, s);
        }
        assert_eq!(events.len(), 0);
        // And an uncleared Off stays Off.
        let mut p = pcs(PcsOpState::Off);
        seq.advance(&mut p, 0, false, 1.0, &mut events);
        assert_eq!(p.op_state, PcsOpState::Off);
    }
}
