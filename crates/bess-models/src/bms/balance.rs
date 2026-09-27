//! Cell spread and passive balancing, at rack granularity.
//!
//! One scalar per rack, the SoC spread between its highest and lowest cell.
//! Throughput widens it (cells differ in coulombic efficiency, so the same
//! current moves their charge by slightly different amounts), idle time
//! widens it (they self-discharge at slightly different rates), and bleed
//! resistors narrow it by burning charge off the high cells near the top of
//! charge. The voltage spread a SCADA screen shows is that SoC spread read
//! through the OCV curve.

use bess_core::config::RackConfig;
use bess_core::state::RackState;
use bess_core::traits::BmsFlows;

use crate::cell::{ocv_cell_integral_v, ocv_cell_v};

/// Parameters of the spread dynamics and the balancing policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Balancing {
    /// SoC spread added per unit of SoC throughput (dimensionless): the
    /// cell-to-cell coulombic-efficiency difference.
    pub growth_per_throughput: f64,
    /// SoC spread added per second regardless of use: the cell-to-cell
    /// self-discharge difference, 1/s.
    pub growth_per_s: f64,
    /// Bleed current through one cell's balancing resistor, A.
    pub bleed_current_a: f64,
    /// Share of the string's cells bleeding while balancing is on: the cells
    /// sitting above the lowest by more than the BMS's tolerance.
    pub bleeding_share: f64,
    /// Balancing only runs at or above this rack SoC (top balancing: the
    /// OCV knee is where cell voltages tell the cells apart).
    pub soc_min: f64,
    /// Voltage spread that switches bleeding on, V.
    pub dv_start_v: f64,
    /// Voltage spread that switches it off again, V.
    pub dv_stop_v: f64,
    /// Discharge current above which balancing is interrupted, A (positive
    /// = discharge, as in `RackState::current_a`).
    pub max_discharge_a: f64,
}

impl Default for Balancing {
    /// GW-01 defaults. Provenance per parameter, sources in CALIBRATION.md
    /// (M2, cell spread and balancing):
    ///
    /// - `growth_per_s`: the EVE MB31 datasheet gives 3.0 %/month
    ///   self-discharge; the Nuvation balancing white paper takes its spread
    ///   as sigma = 10 % of the mean. The highest and lowest of 416 cells sit
    ///   about 6 sigma apart, so the string drifts 1.8 %/month.
    /// - `growth_per_throughput`: estimate. Coulombic efficiency differences
    ///   are measurable at the 10 ppm level (Yang 2015); Nuvation's 830 ppm
    ///   is chosen high to make imbalance visible. 100 ppm max-to-min sits
    ///   between them, and is labeled an estimate.
    /// - `bleed_current_a`: TI BQ79616 AFE, 240 mA per cell; Nuvation's ESS
    ///   bleed resistor works out to about 250 mA.
    /// - `dv_start_v` / `dv_stop_v`: 30 mV on, 20 mV off, from a published
    ///   LFP BMS parameter sheet.
    /// - `soc_min`: Orion BMS starts LFP balancing with a cell within 5 to
    ///   10 % of full.
    /// - `bleeding_share`, `max_discharge_a`: estimates.
    fn default() -> Self {
        Self {
            growth_per_throughput: 1.0e-4,
            growth_per_s: 0.018 / (30.0 * 86_400.0),
            bleed_current_a: 0.24,
            bleeding_share: 0.5,
            soc_min: 0.90,
            dv_start_v: 0.030,
            dv_stop_v: 0.020,
            max_discharge_a: 1.0,
        }
    }
}

impl Balancing {
    /// Advance one rack's spread and balancing by `dt_s`.
    pub fn step(&self, rack: &mut RackState, cfg: &RackConfig, dt_s: f64) -> BmsFlows {
        let capacity_ah = cfg.cell_capacity_ah * rack.soh;
        let mut flows = BmsFlows::default();

        if rack.in_service {
            let throughput_soc = rack.current_a.abs() * dt_s / (3600.0 * capacity_ah);
            rack.cell_dsoc += self.growth_per_throughput * throughput_soc;
        }
        rack.cell_dsoc += self.growth_per_s * dt_s;

        // The policy reads the voltage spread the last step left, as a BMS
        // reads its last measurement.
        let may_bleed =
            rack.in_service && rack.soc >= self.soc_min && rack.current_a <= self.max_discharge_a;
        rack.balancing_active = if rack.balancing_active {
            may_bleed && rack.cell_dv_v > self.dv_stop_v
        } else {
            may_bleed && rack.cell_dv_v >= self.dv_start_v
        };

        if rack.balancing_active {
            // Each bleeding cell loses this much SoC; the spread can close no
            // faster than that, and not past zero.
            let narrowed =
                (self.bleed_current_a * dt_s / (3600.0 * capacity_ah)).min(rack.cell_dsoc);
            rack.cell_dsoc -= narrowed;
            let soc_before = rack.soc;
            rack.soc = (soc_before - self.bleeding_share * narrowed).max(0.0);
            flows.bled_wh = (ocv_cell_integral_v(soc_before) - ocv_cell_integral_v(rack.soc))
                * capacity_ah
                * cfg.cells_series as f64;
            flows.heat_w = flows.bled_wh * 3600.0 / dt_s;
        }

        let half = rack.cell_dsoc / 2.0;
        rack.cell_dv_v = ocv_cell_v(rack.soc + half) - ocv_cell_v(rack.soc - half);
        flows
    }
}

#[cfg(test)]
mod tests {
    use bess_core::config::PlantConfig;
    use bess_core::traits::CellModel;

    use super::*;
    use crate::cell::Ecm1Rc;

    fn rack(soc: f64, current_a: f64, dsoc: f64) -> RackState {
        RackState {
            in_service: true,
            soc,
            soh: 1.0,
            voltage_v: 1400.0,
            current_a,
            cell_temp_c: 25.0,
            polarization_v: 0.0,
            resistance_scale: 1.0,
            temp_offset_c: 0.0,
            alarm_bits: 0,
            cell_dsoc: dsoc,
            cell_dv_v: 0.0,
            balancing_active: false,
        }
    }

    /// A rack at the top of the window with a spread wide enough to bleed.
    fn wide_top_rack() -> RackState {
        let mut r = rack(0.95, 0.0, 0.03);
        r.cell_dv_v = 0.035;
        r
    }

    #[test]
    fn the_energy_bled_is_the_energy_the_cell_model_lost() {
        let cfg = PlantConfig::gw01().rack;
        let cell = Ecm1Rc::lfp_314ah_rack();
        let bal = Balancing::default();
        let mut r = wide_top_rack();
        let before_wh = cell.stored_energy_wh(&r, &cfg);
        let mut bled_wh = 0.0;
        for _ in 0..3600 {
            let flows = bal.step(&mut r, &cfg, 1.0);
            assert!((flows.heat_w - flows.bled_wh * 3600.0).abs() < 1.0e-9);
            bled_wh += flows.bled_wh;
        }
        let lost_wh = before_wh - cell.stored_energy_wh(&r, &cfg);
        assert!(
            bled_wh > 0.0,
            "an hour at the top with a wide spread bled nothing"
        );
        assert!(
            (lost_wh - bled_wh).abs() < 1.0e-6 * lost_wh,
            "stored energy fell {lost_wh} Wh, the BMS reported {bled_wh} Wh"
        );
    }

    #[test]
    fn balancing_narrows_no_faster_than_the_bleed_current_explains() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing::default();
        let mut r = wide_top_rack();
        let start = r.cell_dsoc;
        for _ in 0..3600 {
            bal.step(&mut r, &cfg, 1.0);
        }
        let bleed_only = bal.bleed_current_a * 3600.0 / (3600.0 * cfg.cell_capacity_ah);
        let drift = bal.growth_per_s * 3600.0;
        let narrowed = start - r.cell_dsoc;
        assert!(narrowed > 0.0);
        assert!(narrowed <= bleed_only - drift + 1.0e-12);
    }

    #[test]
    fn throughput_widens_the_spread_while_balancing_is_off() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing::default();
        let mut r = rack(0.5, 157.0, 0.01);
        let mut last = r.cell_dsoc;
        for _ in 0..600 {
            bal.step(&mut r, &cfg, 1.0);
            assert!(!r.balancing_active);
            assert!(r.cell_dsoc > last);
            last = r.cell_dsoc;
        }
    }

    #[test]
    fn the_spread_never_goes_negative() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing {
            dv_stop_v: -1.0,
            ..Balancing::default()
        };
        let mut r = wide_top_rack();
        r.cell_dsoc = 1.0e-6;
        for _ in 0..100 {
            bal.step(&mut r, &cfg, 1.0);
            assert!(r.cell_dsoc >= 0.0);
        }
    }

    #[test]
    fn bleeding_waits_for_the_top_of_the_window() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing::default();
        let mut r = wide_top_rack();
        r.soc = bal.soc_min - 0.01;
        let flows = bal.step(&mut r, &cfg, 1.0);
        assert!(!r.balancing_active);
        assert!(flows.bled_wh.abs() < f64::EPSILON);
    }

    #[test]
    fn discharge_interrupts_but_charging_does_not() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing::default();
        let mut charging = wide_top_rack();
        charging.current_a = -50.0;
        bal.step(&mut charging, &cfg, 1.0);
        assert!(charging.balancing_active);

        charging.current_a = 50.0;
        bal.step(&mut charging, &cfg, 1.0);
        assert!(!charging.balancing_active, "discharge left the bleeders on");
    }

    #[test]
    fn the_bleeders_hold_between_the_two_thresholds() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing::default();
        let between = f64::midpoint(bal.dv_start_v, bal.dv_stop_v);

        let mut off = wide_top_rack();
        off.cell_dv_v = between;
        bal.step(&mut off, &cfg, 1.0);
        assert!(!off.balancing_active, "started below the start threshold");

        let mut on = wide_top_rack();
        on.balancing_active = true;
        on.cell_dv_v = between;
        bal.step(&mut on, &cfg, 1.0);
        assert!(on.balancing_active, "stopped above the stop threshold");

        on.cell_dv_v = bal.dv_stop_v;
        bal.step(&mut on, &cfg, 1.0);
        assert!(!on.balancing_active, "kept bleeding at the stop threshold");
    }

    #[test]
    fn an_out_of_service_rack_neither_bleeds_nor_cycles() {
        let cfg = PlantConfig::gw01().rack;
        let bal = Balancing::default();
        let mut r = wide_top_rack();
        r.in_service = false;
        r.current_a = 300.0;
        let before = r.cell_dsoc;
        let flows = bal.step(&mut r, &cfg, 1.0);
        assert!(!r.balancing_active);
        assert!(flows.bled_wh.abs() < f64::EPSILON);
        assert!((r.cell_dsoc - before - bal.growth_per_s).abs() < 1.0e-15);
    }
}
