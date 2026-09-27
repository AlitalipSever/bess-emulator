//! The energy books: what the site drew for itself and where the losses
//! went, per tick and accumulated.

use serde::{Deserialize, Serialize};

/// The site's own consumption during the last tick, W, itemized by what
/// drew it.
///
/// The M1 gate asks for the nameplate-to-field gap to be explainable item by
/// item, which makes an unattributed lump of auxiliary power a design defect
/// rather than a simplification. Every consumer therefore reports here, and
/// `total_w` is what the substation meters as the house load.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct AuxPower {
    /// Container HVAC: compressors, heaters and fans.
    pub hvac_w: f64,
    /// Rack battery-management electronics.
    pub bms_w: f64,
    /// PCS units energized but not converting. A converting unit's own
    /// supply is already inside its conversion loss, so it is not counted
    /// twice here.
    pub pcs_standby_w: f64,
    /// Plant control, protection and SCADA.
    pub controls_w: f64,
    /// Fire and gas detection, security and access control, site and
    /// building lighting, small power. An enumerated row, not a remainder:
    /// nothing is assigned here because it did not fit elsewhere.
    pub lighting_and_safety_w: f64,
}

impl AuxPower {
    /// Total auxiliary draw, W.
    pub fn total_w(&self) -> f64 {
        self.hvac_w + self.bms_w + self.pcs_standby_w + self.controls_w + self.lighting_and_safety_w
    }
}

/// Auxiliary consumption accumulated per item, Wh.
///
/// The same five items as [`AuxPower`], integrated. `bess-bench` builds the
/// loss waterfall by reading these, rather than by re-deriving the physics
/// from a run it did not perform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuxEnergy {
    /// Container HVAC.
    pub hvac_wh: f64,
    /// Rack battery-management electronics.
    pub bms_wh: f64,
    /// PCS standby tare.
    pub pcs_standby_wh: f64,
    /// Plant control, protection and SCADA.
    pub controls_wh: f64,
    /// Fire and gas detection, security, lighting and small power.
    pub lighting_and_safety_wh: f64,
}

impl AuxEnergy {
    /// Add one tick of `power` lasting `hours`.
    pub fn accumulate(&mut self, power: &AuxPower, hours: f64) {
        self.hvac_wh += power.hvac_w * hours;
        self.bms_wh += power.bms_w * hours;
        self.pcs_standby_wh += power.pcs_standby_w * hours;
        self.controls_wh += power.controls_w * hours;
        self.lighting_and_safety_wh += power.lighting_and_safety_w * hours;
    }

    /// Total auxiliary energy, Wh.
    pub fn total_wh(&self) -> f64 {
        self.hvac_wh
            + self.bms_wh
            + self.pcs_standby_wh
            + self.controls_wh
            + self.lighting_and_safety_wh
    }
}

/// Cumulative energy accounting, Wh. All counters are monotonic; together
/// with the POI meters and the stored energy they close the site energy
/// balance, which CI enforces as an invariant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EnergyAccounting {
    /// Heat dissipated inside the racks: cell losses and balancing bleed.
    pub battery_loss_wh: f64,
    /// Conversion losses in the PCS units.
    pub pcs_loss_wh: f64,
    /// Transformer losses.
    pub transformer_loss_wh: f64,
    /// Auxiliary consumption as the substation meters it: the total that
    /// crossed into the house load.
    pub aux_wh: f64,
    /// The same energy attributed to the items that drew it. Accumulated on
    /// its own path, so `total_wh` matching `aux_wh` is a testable property
    /// rather than an arithmetic identity.
    pub aux_items: AuxEnergy,
}
