//! The event log's footprint in the state tree.
//!
//! The kernel hands each tick's events to the shells and keeps only two
//! numbers about them: how many there have been, which a SCADA poller reads
//! as a wrapping counter to learn it missed some, and a running digest,
//! which puts the whole log under the determinism contract without storing
//! it.

use serde::{Deserialize, Serialize};

use crate::digest;
use crate::kernel::Event;

/// Count and digest of every event the kernel has emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EventLog {
    /// Events emitted since tick 0.
    pub count: u64,
    /// FNV-1a over the canonical encoding of every event, in order.
    pub digest: u64,
}

impl EventLog {
    /// Fold one event into the log.
    pub fn record(&mut self, event: &Event) {
        if self.count == 0 {
            self.digest = digest::OFFSET;
        }
        let bytes = serde_json::to_vec(event).expect("event serialization is infallible");
        self.digest = digest::fold(self.digest, &bytes);
        self.count += 1;
    }

    /// The wrapping u16 a Modbus poller reads.
    pub fn counter_u16(&self) -> u16 {
        (self.count & 0xffff) as u16
    }
}
