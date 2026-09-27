//! The rack alarm word: where each bit raises, clears or latches.
//!
//! Thresholds are anchored to the same sources as the physics they watch:
//! the EVE MB31 temperature limits behind derating, the field and BMS
//! figures behind the balancing thresholds. Sources in CALIBRATION.md.

use bess_core::alarms::layout::rack as bit;
use bess_core::alarms::{falling, has_bit, rising, with_bit, Band};
use bess_core::config::RackConfig;
use bess_core::state::RackState;

/// Thresholds of the rack word.
#[derive(Debug, Clone, PartialEq)]
pub struct RackAlarmThresholds {
    /// Cell temperature warning, rising, degrees Celsius.
    pub over_temp: Band,
    /// Cell temperature trip, degrees Celsius. Latched.
    pub over_temp_trip_c: f64,
    /// Cell temperature warning, falling, degrees Celsius.
    pub under_temp: Band,
    /// Cell temperature trip, degrees Celsius. Latched.
    pub under_temp_trip_c: f64,
    /// How far past the SoC window the SoC high and low bits raise. They
    /// clear back at the window edge.
    pub soc_margin: f64,
    /// Cell voltage spread warning, rising, V.
    pub imbalance: Band,
    /// Cell voltage spread trip, V. Latched.
    pub imbalance_trip_v: f64,
    /// Lower of the two temperature factors, falling.
    pub derate: Band,
}

impl Default for RackAlarmThresholds {
    /// - Over temperature: warn at 50 C, 5 K ahead of the 55 C shoulder
    ///   where the EVE MB31 tables start taking power; trip at 60 C, where
    ///   they reach zero and the datasheet says to stop charging.
    /// - Under temperature: warn at 0 C, below which the same document
    ///   prohibits charging; trip at -30 C, its minimum operating
    ///   temperature, where discharge reaches zero too.
    /// - Imbalance: warn at 120 mV, the threshold a published analysis of
    ///   eight months of LFP container data queries for; trip at 500 mV,
    ///   the voltage-difference fault of a published LFP BMS parameter
    ///   sheet. Balancing holds a clean plant near 30 mV at the top of the
    ///   window and under 80 mV at the bottom, so neither fires unless
    ///   balancing does not.
    /// - SoC and derate bands are estimates sized not to chatter.
    fn default() -> Self {
        Self {
            over_temp: Band {
                raise: 50.0,
                clear: 47.0,
            },
            over_temp_trip_c: 60.0,
            under_temp: Band {
                raise: 0.0,
                clear: 2.0,
            },
            under_temp_trip_c: -30.0,
            soc_margin: 0.01,
            imbalance: Band {
                raise: 0.120,
                clear: 0.100,
            },
            imbalance_trip_v: 0.500,
            derate: Band {
                raise: 0.98,
                clear: 0.995,
            },
        }
    }
}

impl RackAlarmThresholds {
    /// Next rack word. `derate_factor` is the lower of the rack's two
    /// temperature factors right now.
    pub fn evaluate(&self, rack: &RackState, cfg: &RackConfig, derate_factor: f64) -> u32 {
        let w = rack.alarm_bits;
        let t = rack.cell_temp_c;
        let soc_high = Band {
            raise: cfg.soc_max + self.soc_margin,
            clear: cfg.soc_max,
        };
        let soc_low = Band {
            raise: cfg.soc_min - self.soc_margin,
            clear: cfg.soc_min,
        };
        let latched = |b: u8, now: bool| has_bit(w, b) || now;

        let mut next = w;
        for (b, on) in [
            (
                bit::OVER_TEMP_WARNING,
                rising(has_bit(w, bit::OVER_TEMP_WARNING), t, self.over_temp),
            ),
            (
                bit::UNDER_TEMP_WARNING,
                falling(has_bit(w, bit::UNDER_TEMP_WARNING), t, self.under_temp),
            ),
            (
                bit::SOC_HIGH,
                rising(has_bit(w, bit::SOC_HIGH), rack.soc, soc_high),
            ),
            (
                bit::SOC_LOW,
                falling(has_bit(w, bit::SOC_LOW), rack.soc, soc_low),
            ),
            (
                bit::IMBALANCE_WARNING,
                rising(
                    has_bit(w, bit::IMBALANCE_WARNING),
                    rack.cell_dv_v,
                    self.imbalance,
                ),
            ),
            (
                bit::DERATE_ACTIVE,
                falling(has_bit(w, bit::DERATE_ACTIVE), derate_factor, self.derate),
            ),
            (bit::ISOLATED, !rack.in_service),
            (
                bit::OVER_TEMP_TRIP,
                latched(bit::OVER_TEMP_TRIP, t >= self.over_temp_trip_c),
            ),
            (
                bit::IMBALANCE_TRIP,
                latched(bit::IMBALANCE_TRIP, rack.cell_dv_v >= self.imbalance_trip_v),
            ),
            (
                bit::UNDER_TEMP_TRIP,
                latched(bit::UNDER_TEMP_TRIP, t <= self.under_temp_trip_c),
            ),
        ] {
            next = with_bit(next, b, on);
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use bess_core::alarms::TRIP_MASK;
    use bess_core::config::PlantConfig;
    use bess_core::traits::BmsLogic;

    use super::*;
    use crate::bms::BasicBms;

    fn rack(cell_temp_c: f64) -> RackState {
        RackState {
            in_service: true,
            soc: 0.5,
            soh: 1.0,
            voltage_v: 1331.0,
            current_a: 0.0,
            cell_temp_c,
            polarization_v: 0.0,
            resistance_scale: 1.0,
            temp_offset_c: 0.0,
            alarm_bits: 0,
            cell_dsoc: 0.01,
            cell_dv_v: 0.002,
            balancing_active: false,
        }
    }

    /// Step the word the way the kernel does: evaluate, store.
    fn settle(bms: &BasicBms, r: &mut RackState) -> u32 {
        let cfg = PlantConfig::gw01().rack;
        r.alarm_bits = bms.rack_alarms(r, &cfg);
        r.alarm_bits
    }

    #[test]
    fn a_comfortable_rack_raises_nothing() {
        let bms = BasicBms::default();
        assert_eq!(settle(&bms, &mut rack(25.0)), 0);
    }

    #[test]
    fn the_hot_warning_holds_through_its_band() {
        let bms = BasicBms::default();
        let mut r = rack(50.0);
        assert!(has_bit(settle(&bms, &mut r), bit::OVER_TEMP_WARNING));
        r.cell_temp_c = 48.0;
        assert!(has_bit(settle(&bms, &mut r), bit::OVER_TEMP_WARNING));
        r.cell_temp_c = 46.9;
        assert!(!has_bit(settle(&bms, &mut r), bit::OVER_TEMP_WARNING));
    }

    #[test]
    fn a_trip_outlives_its_cause() {
        let bms = BasicBms::default();
        let mut r = rack(60.0);
        assert!(has_bit(settle(&bms, &mut r), bit::OVER_TEMP_TRIP));
        r.cell_temp_c = 25.0;
        let word = settle(&bms, &mut r);
        assert!(
            has_bit(word, bit::OVER_TEMP_TRIP),
            "the trip cleared itself"
        );
        assert!(
            !has_bit(word, bit::OVER_TEMP_WARNING),
            "the warning did not"
        );
    }

    #[test]
    fn derating_shows_once_a_factor_leaves_full_rate() {
        let bms = BasicBms::default();
        // 55.5 C: both factors 0.9 on the hot shoulder.
        assert!(has_bit(settle(&bms, &mut rack(55.5)), bit::DERATE_ACTIVE));
        // 14 C: charging at 0.46P of 0.5P, a winter morning.
        assert!(has_bit(settle(&bms, &mut rack(14.0)), bit::DERATE_ACTIVE));
        assert!(!has_bit(settle(&bms, &mut rack(30.0)), bit::DERATE_ACTIVE));
    }

    #[test]
    fn the_cold_bits_raise_at_the_datasheet_limits() {
        let bms = BasicBms::default();
        let word = settle(&bms, &mut rack(-0.5));
        assert!(has_bit(word, bit::UNDER_TEMP_WARNING));
        assert_eq!(word & TRIP_MASK, 0);
        assert!(has_bit(
            settle(&bms, &mut rack(-30.0)),
            bit::UNDER_TEMP_TRIP
        ));
    }

    #[test]
    fn imbalance_warns_then_trips() {
        let bms = BasicBms::default();
        let mut r = rack(25.0);
        r.cell_dv_v = 0.13;
        let word = settle(&bms, &mut r);
        assert!(has_bit(word, bit::IMBALANCE_WARNING));
        assert!(!has_bit(word, bit::IMBALANCE_TRIP));
        r.cell_dv_v = 0.5;
        assert!(has_bit(settle(&bms, &mut r), bit::IMBALANCE_TRIP));
    }

    #[test]
    fn soc_bits_raise_only_past_the_window() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let mut r = rack(25.0);
        r.soc = cfg.soc_max;
        assert!(
            !has_bit(settle(&bms, &mut r), bit::SOC_HIGH),
            "the window edge is not a violation"
        );
        r.soc = cfg.soc_max + 0.011;
        assert!(has_bit(settle(&bms, &mut r), bit::SOC_HIGH));
        r.soc = cfg.soc_min - 0.011;
        let word = settle(&bms, &mut r);
        assert!(has_bit(word, bit::SOC_LOW));
        assert!(!has_bit(word, bit::SOC_HIGH));
    }

    #[test]
    fn isolation_mirrors_the_contactor() {
        let bms = BasicBms::default();
        let mut r = rack(25.0);
        r.in_service = false;
        assert!(has_bit(settle(&bms, &mut r), bit::ISOLATED));
        r.in_service = true;
        assert!(!has_bit(settle(&bms, &mut r), bit::ISOLATED));
    }
}
