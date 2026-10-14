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
//! surfaces.

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
