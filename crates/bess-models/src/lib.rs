//! Default model implementations for the bess-emulator kernel.
//!
//! Every model here is the simplest version that produces honest,
//! externally observable behavior (see "model to the interface" in
//! ARCHITECTURE.md). Each one is deepened in its own milestone behind the
//! trait it implements, without touching the others.

pub mod aging;
pub mod aux;
pub mod bms;
pub mod calendar;
pub mod cell;
pub mod ems;
pub mod grid;
pub mod grid_signal;
pub mod market;
pub mod math;
pub mod pcs;
pub mod thermal;
pub mod weather;

pub use aging::{LfpAging, NoAging};
pub use aux::InventoryAux;
pub use bms::BasicBms;
pub use cell::Ecm1Rc;
pub use ems::{DayAheadEms, MarketEms, MarketRules};
pub use grid::SimpleGrid;
pub use grid_signal::{SyntheticAfrr, SyntheticFrequency};
pub use pcs::{CurvePcs, FlatPcs, SurfacePcs};
pub use thermal::LumpedThermal;
pub use weather::{
    month_day_from_unix_days, synthetic_grid_frequency_hz, HistoricalWeather, SyntheticWeather,
};
// Re-exported because `HistoricalWeather::hour_at` returns them: a caller
// should not have to depend on `bess-data` to name what this crate hands it.
pub use bess_data::{
    AfrrRequestYear, BalancingYear, FrequencyYear, HourSample, PrecipForm, PriceYear, WeatherYear,
};
pub use market::{replay_balancing, replayed_balancing, AwardPlan};

use std::sync::OnceLock;

use bess_core::config::PlantConfig;
use bess_core::traits::Models;

/// The measured frequency year this process replays, if one was attached
/// (M4 phase 1): one per process, like the bundled datasets are statics,
/// so every surface and the bench replay the same file.
static FREQUENCY: OnceLock<FrequencyYear> = OnceLock::new();

/// Attach a measured frequency year for every driver this process builds
/// from here on (`--frequency`). Once per process.
///
/// # Errors
/// Returns the year back when one is already attached.
pub fn replay_frequency(year: FrequencyYear) -> Result<&'static FrequencyYear, FrequencyYear> {
    FREQUENCY.set(year)?;
    Ok(FREQUENCY.get().expect("just set"))
}

/// The frequency year attached to this process, if any.
pub fn replayed_frequency() -> Option<&'static FrequencyYear> {
    FREQUENCY.get()
}

/// The replayed aFRR request year this process uses, if one was attached
/// (`--afrr`, M4 phase 3).
static AFRR: OnceLock<AfrrRequestYear> = OnceLock::new();

/// Attach a replayed aFRR request year for every driver this process
/// builds from here on. Once per process: `None` when one is already
/// attached.
pub fn replay_afrr(year: AfrrRequestYear) -> Option<&'static AfrrRequestYear> {
    AFRR.set(year).ok()?;
    AFRR.get()
}

/// The aFRR request year attached to this process, if any.
pub fn replayed_afrr() -> Option<&'static AfrrRequestYear> {
    AFRR.get()
}

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
        aging: Box::new(LfpAging::gw01()),
    }
}

/// The GW-01 bundle with the market participant's strategy (M4 phase 4):
/// the same plant, selling FCR and aFRR block by block on the 2024 prices
/// and planning the rest with charges before discharges.
pub fn gw01_market_models(cfg: &PlantConfig) -> Models {
    let mut models = gw01_models(cfg);
    models.ems = Box::new(MarketEms::from_price_year(
        bess_data::dayahead_de_lu_2024(),
        cfg.grid.site_rated_w,
        cfg.nominal_energy_wh() * (cfg.rack.soc_max - cfg.rack.soc_min),
        MarketRules::default(),
    ));
    models
}

/// The exogenous input driver for the GW-01 reference site.
///
/// From M1 on the site replays DWD observations from Lindenberg (Mark),
/// 2024, which is what pins GW-01's nominal location to eastern Germany
/// (see ARCHITECTURE.md). Grid frequency is the synthetic Continental
/// European series unless the process attached a measured year
/// (`replay_frequency`). Shells and calibration runs call this instead of
/// building a driver themselves, so every surface replays the same year.
pub fn gw01_weather() -> HistoricalWeather {
    let mut weather = HistoricalWeather::lindenberg_2024();
    if let Some(year) = replayed_frequency() {
        weather = weather.with_frequency(year);
    }
    if let Some(year) = replayed_afrr() {
        weather = weather.with_afrr(year);
    }
    weather
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
