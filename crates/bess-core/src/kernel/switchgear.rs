//! The substation's switchgear: the HV breaker, the line disconnectors and
//! the earthing switch, their interlocks, and the breaker's closing
//! sequence (M3 phase 4).
//!
//! Positions change between ticks through the commands below, each checked
//! against the interlocks a substation enforces mechanically: the
//! disconnectors never move under a closed breaker, the earthing switch
//! closes only on an isolated bus, and the breaker closes only onto a
//! bus that is connected and not earthed. A trip opens the breaker at
//! once; closing it is a sequence, `Idle -> Closing -> Idle`, that takes
//! the published closing time and ends with the breaker closed and the
//! staggered return started. The sequence is the kernel's, like the
//! return and the converter startup: it is the observable order of a
//! close, and no trait owns it.

use serde::{Deserialize, Serialize};

use crate::state::{BreakerState, PcsOpState, SubstationState, SwitchPosition};

use super::{restart, CommandError, Event, Simulation};

/// Where the breaker is in its closing sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BreakerSequence {
    /// No sequence running: the breaker is where it is.
    Idle,
    /// The close command is in, the breaker's mechanism is travelling.
    Closing,
}

impl BreakerSequence {
    /// The name MQTT, REST and the viewer publish.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Closing => "closing",
        }
    }

    /// The enum point's value: 0 idle, 1 closing.
    pub fn code(self) -> u16 {
        match self {
            Self::Idle => 0,
            Self::Closing => 1,
        }
    }
}

/// A switch an operator moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Switch {
    /// The line disconnectors on the HV side of the breaker.
    Disconnectors,
    /// The earthing switch on the isolated bus.
    EarthingSwitch,
}

impl Switch {
    /// The name the surfaces publish.
    pub fn name(self) -> &'static str {
        match self {
            Self::Disconnectors => "disconnectors",
            Self::EarthingSwitch => "earthing_switch",
        }
    }
}

/// The breaker's closing time, s.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchgearTimes {
    /// From the close command to the contacts closed.
    pub breaker_close_s: f64,
}

impl Default for SwitchgearTimes {
    /// One tick. IEC 62271-100 breakers close in tens of milliseconds;
    /// at the kernel's one-second tick the close lands on the next tick,
    /// which is the finest the plant can say. The disconnectors and the
    /// earthing switch move between ticks (their seconds of travel are a
    /// known gap, CALIBRATION.md).
    fn default() -> Self {
        Self {
            breaker_close_s: 1.0,
        }
    }
}

impl SwitchgearTimes {
    /// Advance the breaker's sequence by `dt_s`: a `Closing` breaker closes
    /// when its time is up, and the staggered return starts over from the
    /// closed breaker. Pushes the sequence's transition.
    pub(super) fn advance(
        &self,
        sub: &mut SubstationState,
        ems: &mut crate::state::EmsState,
        dt_s: f64,
        events: &mut Vec<Event>,
    ) {
        if sub.sequence != BreakerSequence::Closing {
            return;
        }
        sub.sequence_elapsed_s += dt_s;
        if sub.sequence_elapsed_s >= self.breaker_close_s {
            sub.hv_breaker = BreakerState::Closed;
            sub.sequence = BreakerSequence::Idle;
            sub.sequence_elapsed_s = 0.0;
            restart::start_over(ems);
            events.push(Event::BreakerSequenceChanged {
                from: BreakerSequence::Closing,
                to: BreakerSequence::Idle,
            });
        }
    }
}

impl Simulation {
    /// Open the HV breaker by hand: the site disconnects at once, every
    /// connected converter opens its contactors (`Off`), and the return
    /// starts over so a later close brings the blocks back staggered.
    /// Returns `false` on a breaker that is already open.
    pub fn open_hv_breaker(&mut self) -> bool {
        let state = &mut self.state;
        if state.substation.hv_breaker == BreakerState::Open
            && state.substation.sequence == BreakerSequence::Idle
        {
            return false;
        }
        if state.substation.sequence == BreakerSequence::Closing {
            state.substation.sequence = BreakerSequence::Idle;
            state.substation.sequence_elapsed_s = 0.0;
            state.pending_events.push(Event::BreakerSequenceChanged {
                from: BreakerSequence::Closing,
                to: BreakerSequence::Idle,
            });
        }
        state.substation.hv_breaker = BreakerState::Open;
        restart::start_over(&mut state.ems);
        for (block, b) in state.blocks.iter_mut().enumerate() {
            let from = b.pcs.op_state;
            if from != PcsOpState::Fault && from != PcsOpState::Off {
                b.pcs.op_state = PcsOpState::Off;
                b.pcs.state_elapsed_s = 0.0;
                state.pending_events.push(Event::PcsStateChanged {
                    block,
                    from,
                    to: PcsOpState::Off,
                });
            }
        }
        true
    }

    /// Close the HV breaker: starts the closing sequence, which closes it
    /// after the published time and starts the staggered return. Refused
    /// while the disconnectors are open, the earthing switch closed, or a
    /// protection trip still latched (the reset clears it first). Returns
    /// `false` on a breaker already closed or already closing.
    pub fn close_hv_breaker(&mut self) -> Result<bool, CommandError> {
        let sub = &self.state.substation;
        if sub.hv_breaker == BreakerState::Closed || sub.sequence == BreakerSequence::Closing {
            return Ok(false);
        }
        if sub.disconnectors == SwitchPosition::Open {
            return Err(CommandError::Interlocked(
                "the breaker closes only with the disconnectors closed",
            ));
        }
        if sub.earthing_switch == SwitchPosition::Closed {
            return Err(CommandError::Interlocked(
                "the breaker closes only with the earthing switch open",
            ));
        }
        if crate::alarms::has_bit(
            self.state.alarm_bits,
            crate::alarms::layout::site::PROTECTION_TRIP,
        ) {
            return Err(CommandError::Interlocked(
                "the breaker closes only after the protection trip is reset",
            ));
        }
        let sub = &mut self.state.substation;
        sub.sequence = BreakerSequence::Closing;
        sub.sequence_elapsed_s = 0.0;
        self.state
            .pending_events
            .push(Event::BreakerSequenceChanged {
                from: BreakerSequence::Idle,
                to: BreakerSequence::Closing,
            });
        Ok(true)
    }

    /// Close or open the line disconnectors. Refused under a closed or
    /// closing breaker (they never break current), and closing is refused
    /// with the earthing switch closed. Returns `false` when already in
    /// the requested position.
    pub fn set_disconnectors(&mut self, closed: bool) -> Result<bool, CommandError> {
        let sub = &self.state.substation;
        let want = position(closed);
        if sub.disconnectors == want {
            return Ok(false);
        }
        if sub.hv_breaker == BreakerState::Closed || sub.sequence == BreakerSequence::Closing {
            return Err(CommandError::Interlocked(
                "the disconnectors move only under an open breaker",
            ));
        }
        if closed && sub.earthing_switch == SwitchPosition::Closed {
            return Err(CommandError::Interlocked(
                "the disconnectors close only with the earthing switch open",
            ));
        }
        self.state.substation.disconnectors = want;
        self.state.pending_events.push(Event::SwitchOperated {
            switch: Switch::Disconnectors,
            closed,
        });
        Ok(true)
    }

    /// Close or open the earthing switch. Closing is refused unless the
    /// bus is isolated: breaker open and disconnectors open. Opening is
    /// always allowed. Returns `false` when already in the requested
    /// position.
    pub fn set_earthing_switch(&mut self, closed: bool) -> Result<bool, CommandError> {
        let sub = &self.state.substation;
        let want = position(closed);
        if sub.earthing_switch == want {
            return Ok(false);
        }
        if closed
            && (sub.hv_breaker == BreakerState::Closed
                || sub.sequence == BreakerSequence::Closing
                || sub.disconnectors == SwitchPosition::Closed)
        {
            return Err(CommandError::Interlocked(
                "the earthing switch closes only on an isolated bus: breaker and disconnectors open",
            ));
        }
        self.state.substation.earthing_switch = want;
        self.state.pending_events.push(Event::SwitchOperated {
            switch: Switch::EarthingSwitch,
            closed,
        });
        Ok(true)
    }
}

fn position(closed: bool) -> SwitchPosition {
    if closed {
        SwitchPosition::Closed
    } else {
        SwitchPosition::Open
    }
}
