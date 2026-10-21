//! The plant's market side on the scraping surface: what the fiscal meter
//! has booked, and the day-ahead price the planner is acting on.
//!
//! The fiscal totals are published here as running counters, where a time
//! series lives; the Modbus map carries only the last closed quarter-hour.
//! Since M4 phase 3 the EUR rows are here too, as running counters.

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
    for (name, help, value) in [
        (
            "bess_revenue_day_ahead_eur_total",
            "Day-ahead value of the energy exchanged at the POI, EUR (export earns, import pays).",
            meter.total_eur.day_ahead,
        ),
        (
            "bess_revenue_capacity_eur_total",
            "Capacity payments the balancing awards accrued, EUR.",
            meter.total_eur.capacity,
        ),
        (
            "bess_revenue_afrr_energy_eur_total",
            "aFRR energy delivered or absorbed at its price, EUR.",
            meter.total_eur.afrr_energy,
        ),
        (
            "bess_revenue_imbalance_eur_total",
            "Imbalance settlement of the quarter-hour schedule deviations, EUR.",
            meter.total_eur.imbalance,
        ),
    ] {
        metric(out, name, "counter", help, value);
    }
    metric(
        out,
        "bess_day_ahead_price_eur_per_mwh",
        "gauge",
        "Day-ahead price the planner is acting on this hour (DE-LU 2024, replayed).",
        bess_models::gw01_day_ahead_price_eur_mwh(s.unix_time_s()),
    );
}
