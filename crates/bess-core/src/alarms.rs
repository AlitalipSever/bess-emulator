//! The alarm vocabulary: three words, two severities, three behaviors.
//!
//! Alarms live where their cause lives, so there are three words: the rack
//! word (BMS and cells, evaluated by `BmsLogic`), the block word (PCS and
//! containers) and the site word (plant and protection), both evaluated by
//! the kernel. The low byte of each word is warnings, the high byte trips,
//! and every bit behaves one of three ways: it follows a continuous
//! quantity with hysteresis, it mirrors a discrete state exactly, or it
//! latches until an operator resets it. There is no fourth behavior.
//!
//! Bit positions are frozen here (M2 phase 2) and documented on the
//! constants in [`layout`]. After 1.0 a bit's meaning never changes and a
//! bit is never reused; gaps are headroom.

pub mod layout;
pub mod log;
pub mod thresholds;

use serde::{Deserialize, Serialize};

pub use log::EventLog;
pub use thresholds::{Band, BlockSiteThresholds};

/// How much an alarm matters, read off its bit position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Low byte: self-clearing.
    Warning,
    /// High byte: latched, or mirroring a latched state machine.
    Trip,
}

impl Severity {
    /// Severity of a bit position, by the byte it sits in.
    pub fn of_bit(bit: u8) -> Self {
        if bit < 8 {
            Self::Warning
        } else {
            Self::Trip
        }
    }
}

/// The node an alarm word belongs to, as indices into the state tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlarmNode {
    /// The site word.
    Site,
    /// A block word.
    Block {
        /// Block index.
        block: usize,
    },
    /// A rack word.
    Rack {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
    },
}

/// Which alarm words an operator reset reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetScope {
    /// Every word on site, and every PCS in fault.
    Site,
    /// One block word, its racks, and its PCS if it is in fault.
    Block(usize),
    /// One rack word.
    Rack {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
    },
}

/// Why an operator reset was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResetError {
    /// The scope names a block, container or rack the site does not have.
    #[error("no such node on this site: {0:?}")]
    NoSuchNode(ResetScope),
}

/// Mask of the latched (trip) half of a word.
pub const TRIP_MASK: u32 = 0xff00;

/// Next state of a bit that raises when `value` climbs to `band.raise`
/// and clears only once it falls below `band.clear`.
pub fn rising(raised: bool, value: f64, band: Band) -> bool {
    if raised {
        value > band.clear
    } else {
        value >= band.raise
    }
}

/// Next state of a bit that raises when `value` falls to `band.raise`
/// and clears only once it climbs above `band.clear`.
pub fn falling(raised: bool, value: f64, band: Band) -> bool {
    if raised {
        value < band.clear
    } else {
        value <= band.raise
    }
}

/// Set or clear `bit` in `word`.
pub fn with_bit(word: u32, bit: u8, on: bool) -> u32 {
    if on {
        word | (1 << bit)
    } else {
        word & !(1 << bit)
    }
}

/// Whether `bit` is set in `word`.
pub fn has_bit(word: u32, bit: u8) -> bool {
    word & (1 << bit) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAND: Band = Band {
        raise: 50.0,
        clear: 47.0,
    };

    /// A monotone ramp up and back down crosses each edge exactly once:
    /// the no-chatter property, on the helper every continuous bit uses.
    #[test]
    fn a_ramp_through_the_band_raises_once_and_clears_once() {
        let mut raised = false;
        let mut edges = 0;
        let up = (0..=200).map(|i| 40.0 + f64::from(i) * 0.1);
        let down = (0..=200).map(|i| 60.0 - f64::from(i) * 0.1);
        for v in up.chain(down) {
            let next = rising(raised, v, BAND);
            if next != raised {
                edges += 1;
            }
            raised = next;
        }
        assert_eq!(edges, 2);
    }

    #[test]
    fn inside_the_band_the_bit_keeps_what_it_was() {
        assert!(rising(true, 48.0, BAND));
        assert!(!rising(false, 48.0, BAND));
        let cold = Band {
            raise: 0.0,
            clear: 2.0,
        };
        assert!(falling(true, 1.0, cold));
        assert!(!falling(false, 1.0, cold));
    }

    #[test]
    fn severity_follows_the_byte() {
        assert_eq!(Severity::of_bit(7), Severity::Warning);
        assert_eq!(Severity::of_bit(8), Severity::Trip);
    }
}
