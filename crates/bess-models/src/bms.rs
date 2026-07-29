//! Basic BMS: SoC operating window with linear power taper.

use bess_core::config::RackConfig;
use bess_core::state::RackState;
use bess_core::traits::{BmsLogic, PowerLimits};

/// M0 battery management: enforce the SoC operating window by tapering the
/// power limit linearly to zero inside a band at each end. Temperature
/// derating, balancing, and the alarm tree arrive in M2.
#[derive(Debug, Clone, PartialEq)]
pub struct BasicBms {
    /// Width of the linear taper band inside each SoC limit.
    pub taper_soc_band: f64,
}

impl Default for BasicBms {
    fn default() -> Self {
        Self {
            taper_soc_band: 0.03,
        }
    }
}

impl BmsLogic for BasicBms {
    fn rack_limits(&self, rack: &RackState, cfg: &RackConfig) -> PowerLimits {
        if !rack.in_service {
            return PowerLimits::default();
        }
        let rated_w = cfg.max_current_a * cfg.nominal_v();
        let discharge_f = ((rack.soc - cfg.soc_min) / self.taper_soc_band).clamp(0.0, 1.0);
        let charge_f = ((cfg.soc_max - rack.soc) / self.taper_soc_band).clamp(0.0, 1.0);
        PowerLimits {
            max_charge_w: rated_w * charge_f,
            max_discharge_w: rated_w * discharge_f,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::config::PlantConfig;

    fn rack(soc: f64) -> RackState {
        RackState {
            in_service: true,
            soc,
            soh: 1.0,
            voltage_v: 1331.0,
            current_a: 0.0,
            cell_temp_c: 25.0,
            polarization_v: 0.0,
            resistance_scale: 1.0,
            temp_offset_c: 0.0,
            alarm_bits: 0,
        }
    }

    #[test]
    fn full_window_gives_full_limits() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let lim = bms.rack_limits(&rack(0.5), &cfg);
        let rated = cfg.max_current_a * cfg.nominal_v();
        assert!((lim.max_charge_w - rated).abs() < 1.0e-9);
        assert!((lim.max_discharge_w - rated).abs() < 1.0e-9);
    }

    #[test]
    fn limits_taper_to_zero_at_window_edges() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let at_min = bms.rack_limits(&rack(cfg.soc_min), &cfg);
        assert!(at_min.max_discharge_w.abs() < f64::EPSILON);
        assert!(at_min.max_charge_w > 0.0);
        let at_max = bms.rack_limits(&rack(cfg.soc_max), &cfg);
        assert!(at_max.max_charge_w.abs() < f64::EPSILON);
        assert!(at_max.max_discharge_w > 0.0);
    }

    #[test]
    fn out_of_service_rack_has_no_limits() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let mut r = rack(0.5);
        r.in_service = false;
        let lim = bms.rack_limits(&r, &cfg);
        assert!(lim.max_charge_w.abs() < f64::EPSILON);
        assert!(lim.max_discharge_w.abs() < f64::EPSILON);
    }
}
