//! The fiscal meter at the point of interconnection.
//!
//! A second meter beside the SCADA import and export registers, the one a
//! settlement reads: running totals plus quarter-hour periods, the
//! imbalance settlement period in Germany. It sees the same POI power as
//! the SCADA meters, so the two agree to the last bit until M2.5 gives them
//! different accuracy classes. Energy only: the price is known and the
//! product is one multiplication away, but the emulator publishes no EUR
//! until M4 sources the band.

use serde::{Deserialize, Serialize};

/// Quarter-hour revenue meter: monotonic totals, the open period, and the
/// last closed one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RevenueMeter {
    /// Monotonic fiscal import total, Wh.
    pub import_wh: f64,
    /// Monotonic fiscal export total, Wh.
    pub export_wh: f64,
    /// UTC start of the open period, s. A run that starts off a
    /// quarter-hour has a short first period, and this shows it.
    pub period_start_s: i64,
    /// Import booked to the open period so far, Wh.
    pub period_import_wh: f64,
    /// Export booked to the open period so far, Wh.
    pub period_export_wh: f64,
    /// Periods closed since the run began.
    pub periods_closed: u32,
    /// Import of the most recently closed period, Wh.
    pub last_import_wh: f64,
    /// Export of the most recently closed period, Wh.
    pub last_export_wh: f64,
}

impl RevenueMeter {
    /// Settlement period length, s.
    pub const PERIOD_S: i64 = 900;

    /// A meter whose first period opens at `start_unix_s`.
    pub fn new(start_unix_s: i64) -> Self {
        Self {
            period_start_s: start_unix_s,
            ..Self::default()
        }
    }

    /// Book one tick: `poi_w` (positive = export) for `dt_s` seconds, the
    /// tick ending at `now_s`. The energy goes to the open period by its
    /// sign, then the period closes if `now_s` is a quarter-hour. Pure
    /// arithmetic, no clock: Europe/Berlin is a whole-hour offset from UTC,
    /// so local and UTC quarter-hours are the same instants, and labeling a
    /// period with its local day is a reader's job.
    pub fn observe(&mut self, poi_w: f64, dt_s: f64, now_s: i64) {
        // The same arithmetic, in the same order, as the SCADA meters.
        let wh = poi_w * dt_s / 3600.0;
        if wh >= 0.0 {
            self.export_wh += wh;
            self.period_export_wh += wh;
        } else {
            self.import_wh -= wh;
            self.period_import_wh -= wh;
        }
        if now_s.rem_euclid(Self::PERIOD_S) == 0 {
            self.last_import_wh = self.period_import_wh;
            self.last_export_wh = self.period_export_wh;
            self.period_import_wh = 0.0;
            self.period_export_wh = 0.0;
            self.periods_closed += 1;
            self.period_start_s = now_s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RevenueMeter;
    use crate::rng::Rng;

    /// 2026-01-01 00:00 UTC.
    const START_S: i64 = 1_767_225_600;

    #[test]
    fn a_period_closes_at_every_quarter_hour_and_not_between() {
        let mut meter = RevenueMeter::new(START_S);
        for tick in 1..=3600 {
            meter.observe(1.0e6, 1.0, START_S + tick);
            let expected = (tick / 900) as u32;
            assert_eq!(meter.periods_closed, expected, "tick {tick}");
        }
        assert_eq!(meter.period_start_s, START_S + 3600);
    }

    #[test]
    fn energy_routes_by_sign() {
        let mut meter = RevenueMeter::new(START_S);
        meter.observe(3600.0, 1.0, START_S + 1);
        assert_eq!((meter.import_wh, meter.export_wh), (0.0, 1.0));
        meter.observe(-7200.0, 1.0, START_S + 2);
        assert_eq!((meter.import_wh, meter.export_wh), (2.0, 1.0));
        meter.observe(0.0, 1.0, START_S + 3);
        assert_eq!((meter.import_wh, meter.export_wh), (2.0, 1.0));
        assert_eq!((meter.period_import_wh, meter.period_export_wh), (2.0, 1.0));
    }

    #[test]
    fn a_run_started_off_the_quarter_has_a_short_first_period() {
        let start = START_S + 600;
        let mut meter = RevenueMeter::new(start);
        for tick in 1..=300 {
            meter.observe(3600.0, 1.0, start + tick);
        }
        assert_eq!(meter.periods_closed, 1);
        assert_eq!(meter.last_export_wh, 300.0);
        assert_eq!(meter.period_start_s, START_S + 900);
    }

    /// 64 seeded profiles of whole-Wh ticks, so every sum is exact: the
    /// totals never decrease, and the closed periods plus the open one add
    /// up to the totals to the last bit. A tick booked twice or dropped at
    /// a period edge cannot hide behind rounding.
    #[test]
    fn totals_are_monotonic_and_periods_add_up_exactly() {
        let mut rng = Rng::from_seed(0x5eed);
        for _ in 0..64 {
            let mut meter = RevenueMeter::new(START_S);
            let (mut closed_import, mut closed_export) = (0.0, 0.0);
            for tick in 1..=4000 {
                let wh = (rng.next_u64() % 2001) as f64 - 1000.0;
                let (import, export) = (meter.import_wh, meter.export_wh);
                let closed = meter.periods_closed;
                meter.observe(wh * 3600.0, 1.0, START_S + tick);
                assert!(meter.import_wh >= import && meter.export_wh >= export);
                if meter.periods_closed > closed {
                    closed_import += meter.last_import_wh;
                    closed_export += meter.last_export_wh;
                }
            }
            assert_eq!(closed_import + meter.period_import_wh, meter.import_wh);
            assert_eq!(closed_export + meter.period_export_wh, meter.export_wh);
        }
    }
}
