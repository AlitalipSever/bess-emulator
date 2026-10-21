//! Default model implementations for the bess-emulator kernel.
//!
//! Every model here is the simplest version that produces honest,
//! externally observable behavior (see "model to the interface" in
//! ARCHITECTURE.md). Each one is deepened in its own milestone behind the
//! trait it implements, without touching the others.

pub mod aux;
pub mod bms;
pub mod calendar;
pub mod cell;
pub mod ems;
pub mod grid;
pub mod pcs;
pub mod thermal;
pub mod weather;

pub use aux::InventoryAux;
pub use bms::BasicBms;
pub use cell::Ecm1Rc;
pub use ems::DayAheadEms;
pub use grid::SimpleGrid;
pub use pcs::{CurvePcs, FlatPcs, SurfacePcs};
pub use thermal::LumpedThermal;
pub use weather::{
    month_day_from_unix_days, synthetic_grid_frequency_hz, HistoricalWeather, SyntheticWeather,
};
// Re-exported because `HistoricalWeather::hour_at` returns them: a caller
// should not have to depend on `bess-data` to name what this crate hands it.
pub use bess_data::{HourSample, PrecipForm, PriceYear, WeatherYear};

use bess_core::config::PlantConfig;
use bess_core::traits::Models;

/// The default model bundle for the GW-01 reference site.
///
/// A full charge/discharge cycle at 0.5C lands at ~0.92 round-trip at the
/// POI (the M0.5 calibration gate, band [0.90, 0.93]; see CALIBRATION.md).
/// Realistic thermal and auxiliary behavior arrives in M1 and pulls the
/// annual figure toward the 80-85% field band.
pub fn gw01_models(cfg: &PlantConfig) -> Models {
    Models {
        cell: Box::new(Ecm1Rc::lfp_314ah_rack()),
        bms: Box::new(BasicBms::default()),
        thermal: Box::new(LumpedThermal::default()),
        pcs: Box::new(SurfacePcs::cec_sc2500ud_us(cfg.pcs_rated_w)),
        ems: Box::new(DayAheadEms::from_price_year(
            bess_data::dayahead_de_lu_2024(),
            cfg.grid.site_rated_w,
            3,
            2,
        )),
        aux: Box::new(InventoryAux::gw01()),
        grid: Box::new(SimpleGrid::new(cfg.grid.site_rated_w)),
    }
}

/// The exogenous input driver for the GW-01 reference site.
///
/// From M1 on the site replays DWD observations from Lindenberg (Mark),
/// 2024, which is what pins GW-01's nominal location to eastern Germany
/// (see ARCHITECTURE.md). Grid frequency inside the driver stays synthetic
/// until M4. Shells and calibration runs call this instead of building a
/// driver themselves, so every surface replays the same year.
pub fn gw01_weather() -> HistoricalWeather {
    HistoricalWeather::lindenberg_2024()
}

/// The day-ahead price the GW-01 planner is acting on at a UTC timestamp,
/// EUR/MWh: the replayed DE-LU 2024 hour that instant maps onto.
///
/// The one function the shell and the bench call for "the price now", so
/// neither can disagree with the planner about which hour it is. It
/// composes the shared calendar with the artifact lookup and has no other
/// logic.
pub fn gw01_day_ahead_price_eur_mwh(unix_time_s: i64) -> f64 {
    let year = bess_data::dayahead_de_lu_2024();
    let hour = calendar::reference_hour(unix_time_s, year.year());
    year.price_eur_mwh(year.index_at_utc_hour(hour))
}
