//! Default model implementations for the bess-emulator kernel.
//!
//! Every model here is the simplest version that produces honest,
//! externally observable behavior (see "model to the interface" in
//! ARCHITECTURE.md). Each one is deepened in its own milestone behind the
//! trait it implements, without touching the others.

pub mod bms;
pub mod cell;
pub mod ems;
pub mod grid;
pub mod pcs;
pub mod thermal;
pub mod weather;

pub use bms::BasicBms;
pub use cell::Ecm1Rc;
pub use ems::DayAheadEms;
pub use grid::SimpleGrid;
pub use pcs::FlatPcs;
pub use thermal::LumpedThermal;
pub use weather::SyntheticWeather;

use bess_core::config::PlantConfig;
use bess_core::traits::Models;

/// The default M0 model bundle for the GW-01 reference site.
///
/// Loss parameters are tuned so a full charge/discharge cycle lands in the
/// 87-90% round-trip band at the POI (the M0 calibration gate; realistic
/// thermal and auxiliary behavior arrives in M1).
pub fn gw01_models(cfg: &PlantConfig) -> Models {
    Models {
        cell: Box::new(Ecm1Rc::lfp_314ah_rack()),
        bms: Box::new(BasicBms::default()),
        thermal: Box::new(LumpedThermal::default()),
        pcs: Box::new(FlatPcs::new(cfg.pcs_rated_w)),
        ems: Box::new(DayAheadEms::default_profile(cfg.grid.site_rated_w)),
        grid: Box::new(SimpleGrid::new(cfg.grid.site_rated_w)),
    }
}
