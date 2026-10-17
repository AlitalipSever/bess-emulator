//! The alarm vocabulary as the view uses it: what is active now, how loud
//! each entry is, and in what order the list reads.
//!
//! The present is derived from the three alarm words every frame and never
//! stored, because the words are the truth every surface projects; a list
//! kept from events would drift after a restart or a skipped frame. The
//! past is the [`journal`], which keeps the events the viewer saw. The two
//! never feed each other.
//!
//! Panel vocabulary, so it builds without the `sim` feature.
//!
//! - [`journal`] the bounded ring of events the viewer has seen
//! - [`row`]     how an event reads as a journal row
//! - [`filter`]  which rows the panel shows

pub mod filter;
pub mod journal;
pub mod row;

use bess_core::alarms::layout::{block, rack, site};
use bess_core::alarms::{has_bit, AlarmNode, Severity};
use bess_core::state::SiteState;

use crate::style::Palette;

pub use filter::Filter;
pub use journal::{Entry, Journal, SeenMark};

/// How an alarm is drawn: the kernel's two severities, plus the status
/// bits that are neither a warning nor a trip but an object out of the
/// plant on purpose.
///
/// The operator standards keep colour for what is abnormal: red for the
/// highest priority, amber for the next, and a muted tone for equipment
/// taken out of service. One bit is promoted: an open HV breaker is a
/// low-byte status, but the site is off the grid while it is set, and that
/// reads red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tone {
    /// Out of service on purpose: a rack isolated, a block in maintenance.
    OutOfService,
    /// A warning: self-clearing, low byte.
    Warning,
    /// A trip, latched; or the site off the grid.
    Trip,
}

impl Tone {
    /// The tone a set bit is drawn in.
    pub fn of(node: AlarmNode, bit: u8) -> Self {
        match node {
            AlarmNode::Rack { .. } if bit == rack::ISOLATED => Self::OutOfService,
            AlarmNode::Block { .. } if bit == block::MAINTENANCE => Self::OutOfService,
            AlarmNode::Site if bit == site::HV_BREAKER_OPEN => Self::Trip,
            _ => match Severity::of_bit(bit) {
                Severity::Warning => Self::Warning,
                Severity::Trip => Self::Trip,
            },
        }
    }

    /// The loudest tone among a word's set bits, or `None` for a clear word.
    pub fn worst(node: AlarmNode, word: u32) -> Option<Self> {
        (0..32u8)
            .filter(|&bit| has_bit(word, bit))
            .map(|bit| Self::of(node, bit))
            .max()
    }

    /// The signal colour of this tone.
    pub fn color(self, palette: &Palette) -> [f32; 3] {
        match self {
            Self::OutOfService => palette.out_of_service,
            Self::Warning => palette.warning,
            Self::Trip => palette.alarm,
        }
    }
}

/// One bit set on site right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveAlarm {
    /// The word it is set in.
    pub node: AlarmNode,
    /// Its position, per `alarms::layout`.
    pub bit: u8,
}

impl ActiveAlarm {
    /// The kernel's severity, read off the bit position.
    pub fn severity(self) -> Severity {
        Severity::of_bit(self.bit)
    }

    /// How it is drawn.
    pub fn tone(self) -> Tone {
        Tone::of(self.node, self.bit)
    }

    /// Its published name, `rack.derate_active`.
    pub fn name(self) -> String {
        self.node.alarm_name(self.bit)
    }
}

/// Every set bit of the three words, trips first, then in tree order
/// (the site, then each block followed by its racks), then by bit.
///
/// The list reads top-down the way the plant is built, which is the order
/// an operator walks it in.
pub fn active(state: &SiteState) -> Vec<ActiveAlarm> {
    let mut out = Vec::new();
    push_word(&mut out, AlarmNode::Site, state.alarm_bits);
    for (b, blk) in state.blocks.iter().enumerate() {
        push_word(&mut out, AlarmNode::Block { block: b }, blk.alarm_bits);
        for (c, cont) in blk.containers.iter().enumerate() {
            for (r, rk) in cont.racks.iter().enumerate() {
                let node = AlarmNode::Rack {
                    block: b,
                    container: c,
                    rack: r,
                };
                push_word(&mut out, node, rk.alarm_bits);
            }
        }
    }
    // A stable sort keeps the tree order the walk produced within each
    // severity.
    out.sort_by_key(|a| a.severity() != Severity::Trip);
    out
}

fn push_word(out: &mut Vec<ActiveAlarm>, node: AlarmNode, word: u32) {
    out.extend(
        (0..32u8)
            .filter(|&bit| has_bit(word, bit))
            .map(|bit| ActiveAlarm { node, bit }),
    );
}

/// The block a node belongs to, `None` for the site.
pub fn block_of(node: AlarmNode) -> Option<usize> {
    match node {
        AlarmNode::Site => None,
        AlarmNode::Block { block } | AlarmNode::Rack { block, .. } => Some(block),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::PlantConfig;

    fn site_state() -> SiteState {
        SiteState::new(&PlantConfig::gw01(), 7, 1_767_225_600)
    }

    #[test]
    fn a_quiet_plant_has_no_active_alarms() {
        assert_eq!(active(&site_state()), Vec::new());
    }

    /// One bit on each word, one of them a trip, and the order is the
    /// documented one: trips first, then site, block, racks in tree order.
    #[test]
    fn the_walk_finds_every_word_and_sorts_trips_first() {
        let mut s = site_state();
        s.alarm_bits = 1 << site::POWER_LIMITED;
        s.blocks[2].alarm_bits = 1 << block::SETPOINT_NOT_MET;
        s.blocks[2].containers[1].racks[5].alarm_bits = 1 << rack::DERATE_ACTIVE;
        s.blocks[19].containers[0].racks[0].alarm_bits = 1 << rack::OVER_TEMP_TRIP;
        s.blocks[0].alarm_bits = 1 << block::PCS_FAULT;

        let names: Vec<(String, String)> = active(&s)
            .into_iter()
            .map(|a| (a.node.path(), a.name()))
            .collect();
        let want = [
            ("block00", "block.pcs_fault"),
            ("block19/container0/rack00", "rack.over_temp_trip"),
            ("site", "site.power_limited"),
            ("block02", "block.setpoint_not_met"),
            ("block02/container1/rack05", "rack.derate_active"),
        ];
        let want: Vec<(String, String)> = want
            .iter()
            .map(|(p, n)| ((*p).to_owned(), (*n).to_owned()))
            .collect();
        assert_eq!(names, want);
    }

    #[test]
    fn status_bits_are_muted_and_an_open_breaker_reads_red() {
        let rk = AlarmNode::Rack {
            block: 0,
            container: 0,
            rack: 0,
        };
        let blk = AlarmNode::Block { block: 0 };
        assert_eq!(Tone::of(rk, rack::ISOLATED), Tone::OutOfService);
        assert_eq!(Tone::of(blk, block::MAINTENANCE), Tone::OutOfService);
        assert_eq!(Tone::of(blk, block::HVAC_FAILURE), Tone::Warning);
        assert_eq!(Tone::of(blk, block::PCS_FAULT), Tone::Trip);
        assert_eq!(
            Tone::of(AlarmNode::Site, site::PARTIAL_AVAILABILITY),
            Tone::Warning
        );
        assert_eq!(Tone::of(AlarmNode::Site, site::HV_BREAKER_OPEN), Tone::Trip);
    }

    #[test]
    fn the_worst_tone_of_a_word_wins() {
        let rk = AlarmNode::Rack {
            block: 0,
            container: 0,
            rack: 0,
        };
        assert_eq!(Tone::worst(rk, 0), None);
        assert_eq!(
            Tone::worst(rk, 1 << rack::ISOLATED),
            Some(Tone::OutOfService)
        );
        let warn_and_iso = (1 << rack::ISOLATED) | (1 << rack::SOC_LOW);
        assert_eq!(Tone::worst(rk, warn_and_iso), Some(Tone::Warning));
        let all = warn_and_iso | (1 << rack::IMBALANCE_TRIP);
        assert_eq!(Tone::worst(rk, all), Some(Tone::Trip));
    }
}
