//! The fiscal meter's quarter-hours, each labeled with the local day and
//! quarter it settles in.
//!
//! The kernel's meter closes periods on UTC quarter-hours and knows no
//! local clock; naming a period "2024-10-27, quarter 9" is a reader's job,
//! and this is the reader. It labels through the same calendar the planner
//! uses and the day table inside the price artifact, so a period on a DST
//! day is counted on the day the plan was made for. The counts are gated as
//! exact values: they are arithmetic, not calibration.

use std::fmt::Write as _;

use bess_core::state::RevenueMeter;
use bess_core::SiteState;
use bess_data::calendar::{civil_from_days, days_from_civil};
use bess_data::{dayahead_de_lu_2024, PriceYear};
use bess_models::calendar::reference_hour;
use serde::{Deserialize, Serialize};

/// Period counts over the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PeriodKpis {
    /// Quarter-hours the fiscal meter closed.
    pub closed: u64,
    /// Of those, periods on the 23-hour local day (spring forward).
    pub on_short_dst_day: u64,
    /// Of those, periods on the 25-hour local day (fall back).
    pub on_long_dst_day: u64,
}

/// One closed period, as the `--periods` CSV writes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClosedPeriod {
    /// UTC start of the period, s.
    pub start_unix_s: i64,
    /// Local day of the reference year it settles in (0 = January 1st).
    pub local_day: usize,
    /// Quarter of that local day, 0 to 91, 95 or 99.
    pub quarter: usize,
    /// Import booked in the period, kWh.
    pub import_kwh: f64,
    /// Export booked in the period, kWh.
    pub export_kwh: f64,
}

/// The local day and quarter a period starting at `start_unix_s` settles
/// in, through the planner's calendar and the price year's day table.
pub fn label(year: &PriceYear, start_unix_s: i64) -> (usize, usize) {
    let idx = year.index_at_utc_hour(reference_hour(start_unix_s, year.year()));
    let day = year.day_of(idx);
    let hour_of_day = idx - year.day_hours(day).start;
    let quarter_of_hour = start_unix_s.rem_euclid(3600) / RevenueMeter::PERIOD_S;
    (day, hour_of_day * 4 + quarter_of_hour as usize)
}

/// Watches the meter's period count tick over and records each close.
pub struct Collector {
    year: &'static PriceYear,
    seen: u32,
    open_since_s: i64,
    kpis: PeriodKpis,
    rows: Vec<ClosedPeriod>,
}

impl Collector {
    /// A collector for a run whose first period opens at `start_unix_s`.
    pub fn new(start_unix_s: i64) -> Self {
        Self {
            year: dayahead_de_lu_2024(),
            seen: 0,
            open_since_s: start_unix_s,
            kpis: PeriodKpis::default(),
            rows: Vec::new(),
        }
    }

    /// Read one tick of the state tree.
    pub fn observe(&mut self, state: &SiteState) {
        let meter = &state.substation.revenue;
        if meter.periods_closed == self.seen {
            return;
        }
        self.seen = meter.periods_closed;
        let start_unix_s = self.open_since_s;
        self.open_since_s = meter.period_start_s;
        let (local_day, quarter) = label(self.year, start_unix_s);
        self.kpis.closed += 1;
        match self.year.day_hours(local_day).len() {
            23 => self.kpis.on_short_dst_day += 1,
            25 => self.kpis.on_long_dst_day += 1,
            _ => {}
        }
        self.rows.push(ClosedPeriod {
            start_unix_s,
            local_day,
            quarter,
            import_kwh: meter.last_import_wh / 1000.0,
            export_kwh: meter.last_export_wh / 1000.0,
        });
    }

    /// The counts, and every closed period in order.
    pub fn finish(self) -> (PeriodKpis, Vec<ClosedPeriod>) {
        (self.kpis, self.rows)
    }
}

/// The closed periods as CSV, one row per quarter-hour.
pub fn to_csv(rows: &[ClosedPeriod]) -> String {
    let year = dayahead_de_lu_2024();
    let first_day = days_from_civil(i64::from(year.year()), 1, 1);
    let mut out = String::with_capacity(rows.len() * 48 + 64);
    out.push_str("period_start_unix_s,local_date,quarter,import_kwh,export_kwh\n");
    for row in rows {
        let (y, m, d) = civil_from_days(first_day + row.local_day as i64);
        let _ = writeln!(
            out,
            "{},{y:04}-{m:02}-{d:02},{},{:.3},{:.3}",
            row.start_unix_s, row.quarter, row.import_kwh, row.export_kwh
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{label, Collector};
    use bess_core::{PlantConfig, SiteState};
    use bess_data::dayahead_de_lu_2024;

    /// 2026-01-01 00:00 UTC.
    const START_2026_S: i64 = 1_767_225_600;

    #[test]
    fn periods_on_the_dst_days_land_on_the_right_local_quarter() {
        let year = dayahead_de_lu_2024();
        // The sim keeps its UTC date: 2026-03-30 23:00 UTC reads reference
        // 2024-03-30 23:00 UTC, the first hour of local 2024-03-31.
        let spring_midnight = START_2026_S + 88 * 86_400 + 23 * 3600;
        assert_eq!(label(year, spring_midnight), (90, 0));
        // The clock jumped from 02:00 to 03:00: 02:15 UTC later is local
        // 03:15, the day's third hour.
        assert_eq!(label(year, spring_midnight + 2 * 3600 + 900), (90, 9));
        // Local 2024-10-27 begins at 2024-10-26 22:00 UTC (CEST) and its
        // last quarter starts at 2024-10-27 22:45 UTC, 25 hours later.
        let autumn_midnight = START_2026_S + 298 * 86_400 + 22 * 3600;
        assert_eq!(label(year, autumn_midnight), (300, 0));
        assert_eq!(label(year, autumn_midnight + 25 * 3600 - 900), (300, 99));
        assert_eq!(label(year, autumn_midnight + 25 * 3600), (301, 0));
    }

    #[test]
    fn the_collector_counts_each_close_once_with_its_own_start() {
        let cfg = PlantConfig::gw01();
        let mut state = SiteState::new(&cfg, 1, START_2026_S);
        let mut collector = Collector::new(START_2026_S);
        for close in 1..=3 {
            state.substation.revenue.periods_closed = close;
            state.substation.revenue.period_start_s = START_2026_S + i64::from(close) * 900;
            state.substation.revenue.last_export_wh = f64::from(close) * 1000.0;
            collector.observe(&state);
            collector.observe(&state); // no new close, nothing recorded
        }
        let (kpis, rows) = collector.finish();
        assert_eq!(kpis.closed, 3);
        assert_eq!(rows[0].start_unix_s, START_2026_S);
        assert_eq!(rows[2].start_unix_s, START_2026_S + 1800);
        assert_eq!(rows[2].export_kwh, 3.0);
        assert_eq!((rows[0].local_day, rows[0].quarter), (0, 4));
    }
}
