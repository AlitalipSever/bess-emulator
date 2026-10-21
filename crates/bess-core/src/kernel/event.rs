//! What a tick reports: the kernel's discrete events.
//!
//! Facts about a tick, emitted exactly once and folded into the event log's
//! count and digest; the shells fan them out to MQTT and the stream. Most
//! are the plant speaking (a state transition, an alarm edge); one is what
//! was done to it, so the log records cause as well as effect.

use serde::{Deserialize, Serialize};

use crate::alarms::{AlarmNode, Severity};
use crate::state::PcsOpState;
use crate::traits::SocResnap;

use super::FaultAction;

/// Discrete events emitted by the kernel: state transitions, alarm edges,
/// and the fault actions applied to the plant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// A PCS changed operating state.
    PcsStateChanged {
        /// Block index.
        block: usize,
        /// Previous state.
        from: PcsOpState,
        /// New state.
        to: PcsOpState,
    },
    /// An alarm bit went from clear to set.
    AlarmRaised {
        /// Which word.
        node: AlarmNode,
        /// Bit position, per `alarms::layout`.
        bit: u8,
        /// Read off the bit position.
        severity: Severity,
    },
    /// An alarm bit went from set to clear.
    AlarmCleared {
        /// Which word.
        node: AlarmNode,
        /// Bit position, per `alarms::layout`.
        bit: u8,
        /// Read off the bit position.
        severity: Severity,
    },
    /// A fault action was applied between ticks (`Simulation::apply`). It
    /// goes out ahead of the events it causes on the same tick, so the log
    /// reads cause, then effect, and a fault whose consequences take days
    /// (a leaking rack) still has a mark on the tick it happened.
    FaultApplied(FaultAction),
    /// A rack's BMS corrected its reported SoC by a jump: the estimator
    /// admitting the error it had accumulated, at a knee of the OCV curve
    /// where voltage finally tells it so. Real BMS logs show these, and
    /// they are what make the reported-SoC sawtooth a sequence of facts.
    SocResnapped {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
        /// The jump.
        jump: SocResnap,
    },
}
