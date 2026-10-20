//! The instrument inventory: every published number has passed through a
//! meter, and the meters have classes (M2.5 phase 3).
//!
//! The kernel is exact. Measurement is a property of publication, so the
//! error lives here in the projection layer, beside the resolution model:
//! each point names the instrument that reads it, or none, and the
//! instrument's class says how wrong the reading may be and how that
//! depends on load. Scenario data faults (`tap.rs`) apply on top of the
//! instrumented value, as they would to a real device.
//!
//! The error has three parts, each visible in a different analysis: white
//! noise (spectrum), a slow bounded wander of the offset (day-to-day
//! baseline), and a limit that grows at low load (load sweep), the reason
//! accuracy classes with the S suffix exist. All three are derived from
//! (seed, instrument, tick), never stored: a resumed run publishes the same
//! bytes, and the checkpoint stays the kernel's (design D5).

mod inventory;
mod model;

pub use inventory::instrument_of;
pub use model::Instruments;

use crate::map::Point;

/// The instrumented readings the REST summary and the Prometheus metrics
/// publish by name rather than through the map. Kernel units (W, Wh, C),
/// so the surfaces that read them keep their arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Readings {
    /// POI active power as the SCADA analog reads it, W.
    pub poi_active_power_w: f64,
    /// The SCADA import register, Wh.
    pub import_wh: f64,
    /// The SCADA export register, Wh.
    pub export_wh: f64,
    /// The station-service register, Wh.
    pub station_service_wh: f64,
    /// The fiscal meter's last closed quarter-hour, import, Wh.
    pub revenue_period_import_wh: f64,
    /// The same, export, Wh.
    pub revenue_period_export_wh: f64,
    /// Ambient air as the site thermometer reads it, C.
    pub ambient_c: f64,
    /// House load as the auxiliary meter reads it, W.
    pub aux_power_w: f64,
}

impl Readings {
    /// The readings as the kernel holds them, no instrument in between:
    /// what `--exact-instruments` publishes, and what test fixtures that
    /// build a snapshot by hand use.
    #[cfg(test)]
    pub fn exact(state: &bess_core::SiteState) -> Self {
        Self {
            poi_active_power_w: state.substation.poi_active_power_w,
            import_wh: state.substation.import_wh,
            export_wh: state.substation.export_wh,
            station_service_wh: state.substation.station_service_wh,
            revenue_period_import_wh: state.substation.revenue.last_import_wh,
            revenue_period_export_wh: state.substation.revenue.last_export_wh,
            ambient_c: state.weather.ambient_c,
            aux_power_w: state.substation.aux_power_w,
        }
    }
}

/// Where each of the `Readings` sits in the map, found once.
pub struct ReadingIndex {
    slots: [usize; 8],
}

impl ReadingIndex {
    /// The eight points, by name; a map without one of them is a build error.
    pub fn new(points: &[Point]) -> Self {
        let find = |name: &str| {
            points
                .iter()
                .position(|p| p.name == name)
                .unwrap_or_else(|| panic!("the map has no point {name}"))
        };
        Self {
            slots: [
                find("site.poi.active_power_w"),
                find("site.meter.import_kwh"),
                find("site.meter.export_kwh"),
                find("site.meter.station_service_kwh"),
                find("site.revenue.period_import_kwh"),
                find("site.revenue.period_export_kwh"),
                find("site.weather.ambient_c"),
                find("site.aux_power_w"),
            ],
        }
    }

    /// Pull the readings out of a measured vector.
    pub fn read(&self, measured: &[f64]) -> Readings {
        let at = |k: usize| measured[self.slots[k]];
        Readings {
            poi_active_power_w: at(0),
            import_wh: at(1) * 1000.0,
            export_wh: at(2) * 1000.0,
            station_service_wh: at(3) * 1000.0,
            revenue_period_import_wh: at(4) * 1000.0,
            revenue_period_export_wh: at(5) * 1000.0,
            ambient_c: at(6),
            aux_power_w: at(7),
        }
    }
}

#[cfg(test)]
mod tests;
