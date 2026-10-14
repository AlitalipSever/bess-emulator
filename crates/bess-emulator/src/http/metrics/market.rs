//! The plant's market side on the scraping surface: what the fiscal meter
//! has booked, and the day-ahead price the planner is acting on.
//!
//! The fiscal totals are published here as running counters, where a time
//! series lives; the Modbus map carries only the last closed quarter-hour.
//! No revenue in EUR: energy and price are both here and their product is
//! one multiplication away, but the emulator emits no signal it cannot yet
//! calibrate, and M4 sources that band first.

use bess_core::SiteState;

use super::metric;

/// Fiscal meter counters and the day-ahead price gauge.
pub(super) fn market_metrics(out: &mut String, s: &SiteState) {
    let meter = &s.substation.revenue;
    metric(
        out,
        "bess_revenue_import_watthours_total",
        "counter",
        "Fiscal (revenue) meter import at the POI.",
        meter.import_wh,
    );
    metric(
        out,
        "bess_revenue_export_watthours_total",
        "counter",
        "Fiscal (revenue) meter export at the POI.",
        meter.export_wh,
    );
    metric(
        out,
        "bess_revenue_periods_closed_total",
        "counter",
        "Quarter-hour settlement periods the fiscal meter has closed.",
        f64::from(meter.periods_closed),
    );
    metric(
        out,
        "bess_day_ahead_price_eur_per_mwh",
        "gauge",
        "Day-ahead price the planner is acting on this hour (DE-LU 2024, replayed).",
        bess_models::gw01_day_ahead_price_eur_mwh(s.unix_time_s()),
    );
}
