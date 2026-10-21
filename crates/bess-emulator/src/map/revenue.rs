//! The fiscal meter's points: the last closed quarter-hour and the period
//! count, at inputs 46 to 51, right after the station-service meter (a
//! u32 at 44 and 45).
//!
//! Period energies, not the running fiscal totals: those would repeat
//! inputs 12 and 14 to the last digit until M2.5 gives the two meters
//! different accuracy classes, and a SCADA integrator reading a 15-minute
//! meter expects the period value. The running totals go where a time
//! series lives (Prometheus counters, the bench). The price is not on the
//! map: no public BESS register model carries a market price, and the
//! price is the EMS's input, published as such on the shell's other
//! surfaces. Since M4 phase 3 the meter also books EUR: the last closed
//! period's four rows and their total at inputs 87 to 96 (i32, cents),
//! and the running total at 97 (i32, whole EUR).

use bess_core::state::SiteState;

use super::{point, Class, Encoding, Point, Space};

/// Revenue-meter points.
pub(super) fn points() -> Vec<Point> {
    vec![
        point!(
            "site.revenue.period_import_kwh",
            "kWh",
            Class::Slow,
            Encoding::U32,
            1.0,
            46,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_import_wh / 1000.0
        ),
        point!(
            "site.revenue.period_export_kwh",
            "kWh",
            Class::Slow,
            Encoding::U32,
            1.0,
            48,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_export_wh / 1000.0
        ),
        point!(
            "site.revenue.period_day_ahead_eur_x100",
            "EUR",
            Class::Slow,
            Encoding::I32,
            100.0,
            87,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_eur.day_ahead
        ),
        point!(
            "site.revenue.period_capacity_eur_x100",
            "EUR",
            Class::Slow,
            Encoding::I32,
            100.0,
            89,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_eur.capacity
        ),
        point!(
            "site.revenue.period_afrr_energy_eur_x100",
            "EUR",
            Class::Slow,
            Encoding::I32,
            100.0,
            91,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_eur.afrr_energy
        ),
        point!(
            "site.revenue.period_imbalance_eur_x100",
            "EUR",
            Class::Slow,
            Encoding::I32,
            100.0,
            93,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_eur.imbalance
        ),
        point!(
            "site.revenue.period_total_eur_x100",
            "EUR",
            Class::Slow,
            Encoding::I32,
            100.0,
            95,
            Space::Input,
            |s: &SiteState| s.substation.revenue.last_eur.total()
        ),
        point!(
            "site.revenue.total_eur",
            "EUR",
            Class::Slow,
            Encoding::I32,
            1.0,
            97,
            Space::Input,
            |s: &SiteState| s.substation.revenue.total_eur.total()
        ),
        point!(
            "site.revenue.periods_closed",
            "count",
            Class::Slow,
            Encoding::U32,
            1.0,
            50,
            Space::Input,
            |s: &SiteState| f64::from(s.substation.revenue.periods_closed)
        ),
    ]
}
