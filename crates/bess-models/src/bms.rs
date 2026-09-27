//! Basic BMS: SoC operating window with linear power taper, derated by
//! cell temperature, with a cell spread that passive balancing narrows.

pub mod alarms;
pub mod balance;
pub mod derate;

use bess_core::config::RackConfig;
use bess_core::state::RackState;
use bess_core::traits::{BmsFlows, BmsLogic, PowerLimits};

pub use alarms::RackAlarmThresholds;
pub use balance::Balancing;
pub use derate::{TempCurve, EVE_MB31_CHARGE_P, EVE_MB31_DISCHARGE_P};

/// Battery management: enforce the SoC operating window by tapering the
/// power limit linearly to zero inside a band at each end, and derate each
/// direction by cell temperature. The two factors multiply: a cold rack
/// near the top of the window charges worse than either alone would allow.
/// Its step advances the cell spread and runs top balancing, and it owns
/// the rack alarm word.
#[derive(Debug, Clone, PartialEq)]
pub struct BasicBms {
    /// Width of the linear taper band inside each SoC limit.
    pub taper_soc_band: f64,
    /// Permitted charging power against cell temperature.
    pub charge_temp: TempCurve,
    /// Permitted discharging power against cell temperature.
    pub discharge_temp: TempCurve,
    /// Cell spread dynamics and the balancing policy.
    pub balancing: Balancing,
    /// Where the rack alarm bits raise, clear and latch.
    pub alarms: RackAlarmThresholds,
}

impl Default for BasicBms {
    fn default() -> Self {
        Self {
            taper_soc_band: 0.03,
            charge_temp: TempCurve::new(EVE_MB31_CHARGE_P),
            discharge_temp: TempCurve::new(EVE_MB31_DISCHARGE_P),
            balancing: Balancing::default(),
            alarms: RackAlarmThresholds::default(),
        }
    }
}

impl BmsLogic for BasicBms {
    fn rack_limits(&self, rack: &RackState, cfg: &RackConfig) -> PowerLimits {
        if !rack.in_service {
            return PowerLimits::default();
        }
        let rated_w = cfg.max_current_a * cfg.nominal_v();
        let discharge_f = ((rack.soc - cfg.soc_min) / self.taper_soc_band).clamp(0.0, 1.0)
            * self.discharge_temp.factor(rack.cell_temp_c);
        let charge_f = ((cfg.soc_max - rack.soc) / self.taper_soc_band).clamp(0.0, 1.0)
            * self.charge_temp.factor(rack.cell_temp_c);
        PowerLimits {
            max_charge_w: rated_w * charge_f,
            max_discharge_w: rated_w * discharge_f,
        }
    }

    fn step_bms(&self, rack: &mut RackState, cfg: &RackConfig, dt_s: f64) -> BmsFlows {
        self.balancing.step(rack, cfg, dt_s)
    }

    fn rack_alarms(&self, rack: &RackState, cfg: &RackConfig) -> u32 {
        let t = rack.cell_temp_c;
        let factor = self
            .charge_temp
            .factor(t)
            .min(self.discharge_temp.factor(t));
        self.alarms.evaluate(rack, cfg, factor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::config::PlantConfig;

    fn rack(soc: f64) -> RackState {
        rack_at(soc, 25.0)
    }

    fn rack_at(soc: f64, cell_temp_c: f64) -> RackState {
        RackState {
            in_service: true,
            soc,
            soh: 1.0,
            voltage_v: 1331.0,
            current_a: 0.0,
            cell_temp_c,
            polarization_v: 0.0,
            resistance_scale: 1.0,
            temp_offset_c: 0.0,
            alarm_bits: 0,
            cell_dsoc: 0.0,
            cell_dv_v: 0.0,
            balancing_active: false,
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

    #[test]
    fn defaults_are_the_datasheet_tables() {
        let bms = BasicBms::default();
        assert_eq!(bms.charge_temp.points(), EVE_MB31_CHARGE_P);
        assert_eq!(bms.discharge_temp.points(), EVE_MB31_DISCHARGE_P);
    }

    #[test]
    fn cold_rack_charges_slowly_but_still_discharges() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let rated = cfg.max_current_a * cfg.nominal_v();
        let cold = bms.rack_limits(&rack_at(0.5, 5.0), &cfg);
        // 0.12P against the table's 0.5P peak.
        assert!((cold.max_charge_w - rated * 0.24).abs() < 1.0e-6);
        assert!((cold.max_discharge_w - rated).abs() < 1.0e-6);
        let frozen = bms.rack_limits(&rack_at(0.5, -5.0), &cfg);
        assert!(frozen.max_charge_w.abs() < f64::EPSILON);
        assert!(frozen.max_discharge_w > 0.0);
    }

    #[test]
    fn beyond_the_hot_limit_nothing_flows() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let lim = bms.rack_limits(&rack_at(0.5, 61.0), &cfg);
        assert!(lim.max_charge_w.abs() < f64::EPSILON);
        assert!(lim.max_discharge_w.abs() < f64::EPSILON);
    }

    #[test]
    fn temperature_multiplies_the_soc_taper() {
        let bms = BasicBms::default();
        let cfg = PlantConfig::gw01().rack;
        let rated = cfg.max_current_a * cfg.nominal_v();
        // Halfway into the top taper band and halfway down the cold ramp
        // from 10 C to 15 C (0.4P of 0.5P).
        let soc = cfg.soc_max - bms.taper_soc_band / 2.0;
        let lim = bms.rack_limits(&rack_at(soc, 12.5), &cfg);
        assert!((lim.max_charge_w - rated * 0.5 * 0.8).abs() < 1.0e-6);
    }
}
