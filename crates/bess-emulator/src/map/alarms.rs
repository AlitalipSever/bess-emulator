//! The alarm points: the three alarm words, the counts read off them, and
//! the event counter.
//!
//! They sit at site and block addresses alike and share one rule: every
//! point here moves only when an alarm edge happens, so every point here is
//! [`Class::Event`] and nothing outside this file is. The bit layouts are the
//! kernel's (`bess_core::alarms::layout`); the projection publishes a word's
//! low 16 bits, which is all the layout occupies.

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::rack::DERATE_ACTIVE;
use bess_core::state::{BlockState, SiteState};

use super::{block_base, block_ext_base, block_prefix, point, Class, Encoding, Point, Space};

/// A word as the u16 register publishes it.
fn word_u16(word: u32) -> f64 {
    f64::from(word & 0xffff)
}

/// The block's rack words folded with OR: some rack in the block has the
/// condition. The MQTT event subtree says which.
fn rack_word_fold(block: &BlockState) -> u32 {
    block
        .containers
        .iter()
        .flat_map(|c| c.racks.iter())
        .fold(0, |acc, r| acc | r.alarm_bits)
}

/// Every alarm active on site: the set bits of every rack, block and site
/// word.
fn active_alarm_count(s: &SiteState) -> f64 {
    let racks: u32 = s.racks().map(|r| r.alarm_bits.count_ones()).sum();
    let blocks: u32 = s.blocks.iter().map(|b| b.alarm_bits.count_ones()).sum();
    f64::from(racks + blocks + s.alarm_bits.count_ones())
}

/// The site's alarm points.
pub(super) fn site_points() -> Vec<Point> {
    use Class::Event;
    use Encoding::U16;
    use Space::Input;

    vec![
        point!(
            "site.alarm_count",
            "count",
            Event,
            U16,
            1.0,
            31,
            Input,
            active_alarm_count
        ),
        point!(
            "site.alarm_bits",
            "bitfield",
            Event,
            U16,
            1.0,
            42,
            Input,
            |s: &SiteState| word_u16(s.alarm_bits)
        ),
        // Wraps at 65536. A poller diffing two reads learns how many events
        // it missed between them, even when it cannot learn which.
        point!(
            "site.event_counter",
            "count",
            Event,
            U16,
            1.0,
            43,
            Input,
            |s: &SiteState| f64::from(s.event_log.counter_u16())
        ),
    ]
}

/// Block `b`'s alarm points, in both per-block ranges.
pub(super) fn block_points(b: usize) -> Vec<Point> {
    use Class::Event;
    use Encoding::U16;
    use Space::Input;

    let prefix = block_prefix(b);
    vec![
        // The name predates the block's own word and is published, so it
        // stays: this register is the rack words, folded.
        point!(
            format!("{prefix}.alarm_bits"),
            "bitfield",
            Event,
            U16,
            1.0,
            block_base(b) + 6,
            Input,
            move |s: &SiteState| word_u16(rack_word_fold(&s.blocks[b]))
        ),
        point!(
            format!("{prefix}.block_alarm_bits"),
            "bitfield",
            Event,
            U16,
            1.0,
            block_ext_base(b),
            Input,
            move |s: &SiteState| word_u16(s.blocks[b].alarm_bits)
        ),
        // Derating as a count rather than a fold: the fold says some rack
        // is below full rate, the count says how much of the block is.
        point!(
            format!("{prefix}.racks_derated"),
            "count",
            Event,
            U16,
            1.0,
            block_ext_base(b) + 1,
            Input,
            move |s: &SiteState| {
                s.blocks[b]
                    .containers
                    .iter()
                    .flat_map(|c| c.racks.iter())
                    .filter(|r| has_bit(r.alarm_bits, DERATE_ACTIVE))
                    .count() as f64
            }
        ),
    ]
}

#[cfg(test)]
mod tests {
    use bess_core::alarms::layout::{block as blk, rack, site};
    use bess_core::config::PlantConfig;

    use super::super::{block, build_points, control, input_bank};
    use super::*;

    fn plant() -> (Vec<Point>, SiteState) {
        let cfg = PlantConfig::gw01();
        (build_points(&cfg), SiteState::new(&cfg, 1, 0))
    }

    /// Base+6 folds the block's rack words, and only that block's.
    #[test]
    fn the_original_block_register_folds_its_rack_words() {
        let (points, mut state) = plant();
        state.blocks[1].containers[0].racks[3].alarm_bits = 1 << rack::DERATE_ACTIVE;
        state.blocks[1].containers[1].racks[9].alarm_bits = 1 << rack::OVER_TEMP_TRIP;
        let input = input_bank(&points, &state);
        assert_eq!(
            input[1016],
            (1 << rack::DERATE_ACTIVE) | (1 << rack::OVER_TEMP_TRIP)
        );
        assert_eq!(input[1006], 0, "block 0 has no rack alarms");
    }

    /// The block's own word and the site word each land at their address,
    /// and the block word is not mistaken for the rack fold beside it.
    #[test]
    fn the_block_and_site_words_carry_their_own_bits() {
        let (points, mut state) = plant();
        state.blocks[2].alarm_bits = (1 << blk::SETPOINT_NOT_MET) | (1 << blk::PCS_FAULT);
        state.alarm_bits = 1 << site::POWER_LIMITED;
        let input = input_bank(&points, &state);
        assert_eq!(input[2020], 0x0101, "block02.block_alarm_bits");
        assert_eq!(
            input[1026], 0,
            "block02.alarm_bits is the racks, still clear"
        );
        assert_eq!(input[2010], 0, "block01 reports its own word");
        assert_eq!(input[42], 1, "site.alarm_bits");
    }

    /// The alarm count is every set bit on site, whichever word holds it.
    #[test]
    fn the_alarm_count_counts_all_three_words() {
        let (points, mut state) = plant();
        state.blocks[0].containers[0].racks[0].alarm_bits =
            (1 << rack::OVER_TEMP_WARNING) | (1 << rack::DERATE_ACTIVE);
        state.blocks[5].alarm_bits = 1 << blk::HVAC_FAILURE;
        state.alarm_bits = 1 << site::POWER_LIMITED;
        assert_eq!(input_bank(&points, &state)[31], 4);
    }

    /// The counter is the log's count modulo 65536, so a poller's diff
    /// survives the wrap.
    #[test]
    fn the_event_counter_wraps_with_the_log() {
        let (points, mut state) = plant();
        state.event_log.count = 65_536 + 7;
        assert_eq!(input_bank(&points, &state)[43], 7);
    }

    /// The derate count is the racks with the bit, not the racks with any
    /// alarm.
    #[test]
    fn the_derate_count_counts_derating_racks_only() {
        let (points, mut state) = plant();
        let racks = &mut state.blocks[4].containers[1].racks;
        racks[0].alarm_bits = 1 << rack::DERATE_ACTIVE;
        racks[1].alarm_bits = (1 << rack::DERATE_ACTIVE) | (1 << rack::OVER_TEMP_WARNING);
        racks[2].alarm_bits = 1 << rack::OVER_TEMP_WARNING;
        assert_eq!(input_bank(&points, &state)[2041], 2);
    }

    /// The event class belongs to this file: everything here publishes on
    /// change, and nothing elsewhere does.
    #[test]
    fn the_event_class_is_exactly_the_alarm_points() {
        let alarm_points = site_points().into_iter().chain(block_points(0));
        assert!(alarm_points.into_iter().all(|p| p.class == Class::Event));
        let others = super::super::site::points()
            .into_iter()
            .chain(control::points())
            .chain(block::points(0));
        for p in others {
            assert_ne!(p.class, Class::Event, "{} publishes on change", p.name);
        }
    }
}
