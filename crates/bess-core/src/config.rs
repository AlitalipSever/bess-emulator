//! Plant configuration: topology and nameplate ratings of the reference site.
//!
//! Configuration describes structure (how many blocks, containers, racks)
//! and ratings. Behavioral parameters (ECM resistances, thermal constants,
//! efficiency figures) belong to the model implementations in `bess-models`,
//! so a model can be deepened without touching the site definition.

use serde::{Deserialize, Serialize};

/// Full plant configuration. Exactly one official site (GW-01) ships; the
/// struct exists so tests can build reduced variants, not as a user-facing
/// configuration surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlantConfig {
    /// Site identifier shown in telemetry.
    pub site_id: String,
    /// Number of power blocks.
    pub blocks: usize,
    /// Containers per block.
    pub containers_per_block: usize,
    /// Racks per container.
    pub racks_per_container: usize,
    /// Rack topology and ratings, identical for every rack.
    pub rack: RackConfig,
    /// PCS AC rating per block, W.
    pub pcs_rated_w: f64,
    /// Grid connection ratings.
    pub grid: GridConfig,
    /// State of charge all racks start at, 0..1 (small per-rack spread is
    /// applied on top from the seeded PRNG).
    pub initial_soc: f64,
}

/// Rack topology and ratings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RackConfig {
    /// Cells in series (one string per rack).
    pub cells_series: usize,
    /// Cell capacity, Ah.
    pub cell_capacity_ah: f64,
    /// Cell nominal voltage, V (used for nameplate arithmetic only; the
    /// electrical model uses its own OCV curve).
    pub cell_nominal_v: f64,
    /// Continuous current limit per rack, A.
    pub max_current_a: f64,
    /// Lower end of the SoC operating window enforced by the BMS.
    pub soc_min: f64,
    /// Upper end of the SoC operating window enforced by the BMS.
    pub soc_max: f64,
}

/// Grid connection ratings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridConfig {
    /// Point-of-interconnection nominal voltage, kV.
    pub poi_nominal_kv: f64,
    /// Site AC rating at the POI, W.
    pub site_rated_w: f64,
}

impl RackConfig {
    /// Nominal rack voltage, V.
    pub fn nominal_v(&self) -> f64 {
        self.cell_nominal_v * self.cells_series as f64
    }

    /// Nameplate rack energy, Wh.
    pub fn nominal_energy_wh(&self) -> f64 {
        self.nominal_v() * self.cell_capacity_ah
    }
}

impl PlantConfig {
    /// The official reference site GW-01: 100 MW / 200 MWh (2 h), LFP,
    /// 20 power blocks of one 5 MW PCS plus two ~5 MWh containers each,
    /// 110 kV grid connection.
    pub fn gw01() -> Self {
        Self {
            site_id: "GW-01".to_owned(),
            blocks: 20,
            containers_per_block: 2,
            racks_per_container: 12,
            rack: RackConfig {
                cells_series: 416,
                cell_capacity_ah: 314.0,
                cell_nominal_v: 3.2,
                max_current_a: 314.0,
                soc_min: 0.05,
                soc_max: 0.95,
            },
            pcs_rated_w: 5.0e6,
            grid: GridConfig {
                poi_nominal_kv: 110.0,
                site_rated_w: 100.0e6,
            },
            initial_soc: 0.5,
        }
    }

    /// Racks per block.
    pub fn racks_per_block(&self) -> usize {
        self.containers_per_block * self.racks_per_container
    }

    /// Total racks on site.
    pub fn total_racks(&self) -> usize {
        self.blocks * self.racks_per_block()
    }

    /// Nameplate site energy, Wh.
    pub fn nominal_energy_wh(&self) -> f64 {
        self.rack.nominal_energy_wh() * self.total_racks() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::PlantConfig;

    #[test]
    fn gw01_nameplate_matches_spec() {
        let cfg = PlantConfig::gw01();
        assert_eq!(cfg.total_racks(), 480);
        // 100 MW site rating out of 20 x 5 MW blocks.
        let pcs_total = cfg.pcs_rated_w * cfg.blocks as f64;
        assert!((pcs_total - cfg.grid.site_rated_w).abs() < 1.0);
        // ~200 MWh nameplate (416S x 314 Ah x 3.2 V x 480 racks).
        let mwh = cfg.nominal_energy_wh() / 1.0e6;
        assert!((199.0..202.0).contains(&mwh), "nameplate {mwh} MWh");
    }
}
