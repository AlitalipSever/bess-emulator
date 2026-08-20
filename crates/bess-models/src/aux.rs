//! The site's auxiliary loads, itemized.
//!
//! Until M1 this whole layer was a single 150 kW constant sitting inside the
//! substation model. That number could not be checked, could not be
//! attributed and did not move when the plant changed size. The M1 gate asks
//! for the nameplate-to-field efficiency gap to be explainable item by item,
//! so each consumer now has a name, a value with a stated origin, and a
//! meter of its own.
//!
//! Parameter provenance is recorded in CALIBRATION.md the same way the
//! thermal parameters are: an estimate says it is an estimate.

use bess_core::state::AuxPower;
use bess_core::traits::{AuxDemand, AuxiliaryModel};

/// M1 auxiliary inventory: four itemized station loads on top of the HVAC
/// draw the thermal layer reports.
///
/// Two of the four scale with the plant (rack electronics, converter
/// standby); two are site-level constants sized for a 100 MW class plant
/// with its own 110 kV substation.
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryAux {
    /// Battery-management electronics per rack, W.
    pub bms_per_rack_w: f64,
    /// Draw of one PCS that is energized but not converting, W.
    pub pcs_standby_w: f64,
    /// Plant control, protection, SCADA and communications, W.
    pub controls_w: f64,
    /// Lighting, fire detection, security and small power, W.
    pub misc_w: f64,
}

impl Default for InventoryAux {
    /// The GW-01 inventory. Sources and scaling arguments in CALIBRATION.md;
    /// in short:
    ///
    /// - **71.8 W per rack.** Schimpe et al. 2018 measured 287 W of
    ///   battery-side control and monitoring on a container system of 8
    ///   racks by 13 modules by 16 cell blocks. Scaled by monitored cell
    ///   count, since that is what sets how many sensing channels exist:
    ///   0.172 W per cell block, times the 416 cells a GW-01 rack monitors.
    /// - **339.8 W per idle PCS.** The CEC inverter database publishes night
    ///   tare, the draw of a unit that is energized and not delivering, as
    ///   169.9 W for the Sungrow SC2500UD-US already used for this plant's
    ///   efficiency curve. A 5 MW block is two of those units.
    /// - **15 kW of controls and 10 kW of lighting and small power.** Both
    ///   engineering estimates, itemized in CALIBRATION.md, and the two
    ///   figures this inventory would most like a source for.
    fn default() -> Self {
        Self {
            bms_per_rack_w: 71.8,
            pcs_standby_w: 339.8,
            controls_w: 15.0e3,
            misc_w: 10.0e3,
        }
    }
}

impl AuxiliaryModel for InventoryAux {
    fn step_site(&self, demand: AuxDemand) -> AuxPower {
        AuxPower {
            hvac_w: demand.hvac_w,
            bms_w: self.bms_per_rack_w * demand.racks as f64,
            pcs_standby_w: self.pcs_standby_w * demand.pcs_in_standby as f64,
            controls_w: self.controls_w,
            misc_w: self.misc_w,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demand(racks: usize, idle_pcs: usize, hvac_w: f64) -> AuxDemand {
        AuxDemand {
            hvac_w,
            racks,
            pcs_in_standby: idle_pcs,
        }
    }

    #[test]
    fn the_total_is_the_sum_of_the_items() {
        let aux = InventoryAux::default().step_site(demand(480, 20, 400.0e3));
        let sum = aux.hvac_w + aux.bms_w + aux.pcs_standby_w + aux.controls_w + aux.misc_w;
        assert!((aux.total_w() - sum).abs() < 1.0e-9);
    }

    /// The point of itemizing: a bigger plant now draws more. The old
    /// constant did not notice how many racks it was monitoring.
    #[test]
    fn the_inventory_follows_the_plant_size() {
        let inv = InventoryAux::default();
        let small = inv.step_site(demand(48, 2, 0.0));
        let big = inv.step_site(demand(480, 20, 0.0));
        assert!((big.bms_w - 10.0 * small.bms_w).abs() < 1.0e-6);
        assert!((big.pcs_standby_w - 10.0 * small.pcs_standby_w).abs() < 1.0e-6);
        // Site-level items do not: one substation, one control room.
        assert!((big.controls_w - small.controls_w).abs() < f64::EPSILON);
        assert!((big.misc_w - small.misc_w).abs() < f64::EPSILON);
    }

    /// A converting unit's self-supply is already inside the PCS efficiency
    /// curve's load-independent term. Counting the tare as well would bill
    /// the same watts twice, so the tare only follows idle units.
    #[test]
    fn only_idle_converters_pay_the_tare() {
        let inv = InventoryAux::default();
        assert!(inv.step_site(demand(480, 0, 0.0)).pcs_standby_w.abs() < f64::EPSILON);
        let half = inv.step_site(demand(480, 10, 0.0));
        let all = inv.step_site(demand(480, 20, 0.0));
        assert!((all.pcs_standby_w - 2.0 * half.pcs_standby_w).abs() < 1.0e-6);
    }

    /// HVAC is measured by the thermal layer, not by this one: the inventory
    /// carries it through untouched so the waterfall has one row per
    /// consumer and no consumer in two rows.
    #[test]
    fn hvac_is_passed_through_unchanged() {
        let aux = InventoryAux::default().step_site(demand(480, 20, 1.234e6));
        assert!((aux.hvac_w - 1.234e6).abs() < f64::EPSILON);
    }

    /// The GW-01 station inventory, without HVAC and with every converter
    /// idle: this is what the plant burns doing nothing at all.
    #[test]
    fn the_idle_station_load_is_what_the_record_says() {
        let aux = InventoryAux::default().step_site(demand(480, 20, 0.0));
        let kw = aux.total_w() / 1.0e3;
        assert!(
            (65.0..67.0).contains(&kw),
            "idle station load {kw:.1} kW, recorded as 66.3 kW in CALIBRATION.md"
        );
    }
}
