//! 1-RC equivalent-circuit rack model with an LFP OCV curve.

use bess_core::config::RackConfig;
use bess_core::state::RackState;
use bess_core::traits::{CellModel, RackStepResult};

/// Cell-level OCV curve for a 314 Ah class LFP cell, (SoC, V). Shape taken
/// from public datasheets and published LFP OCV studies: a flat plateau
/// around 3.3 V with knees at both ends. Strictly increasing in SoC.
const OCV_LFP_V: &[(f64, f64)] = &[
    (0.00, 2.90),
    (0.05, 3.05),
    (0.10, 3.18),
    (0.20, 3.25),
    (0.30, 3.28),
    (0.40, 3.29),
    (0.50, 3.30),
    (0.60, 3.31),
    (0.70, 3.32),
    (0.80, 3.33),
    (0.90, 3.34),
    (0.95, 3.36),
    (1.00, 3.45),
];

/// 1-RC equivalent circuit aggregated to rack level: a series resistance R0
/// plus one RC polarization branch (R1, tau1). Per tick it solves the
/// quadratic coupling of terminal power and current, coulomb-counts SoC,
/// and relaxes the polarization state.
///
/// Resistances are rack totals (cells plus busbars, fuses, contactors and
/// cabling) and are scaled by each rack's manufacturing-spread multiplier.
#[derive(Debug, Clone, PartialEq)]
pub struct Ecm1Rc {
    /// Series resistance of the whole rack, ohm.
    pub r0_ohm: f64,
    /// Polarization resistance, ohm.
    pub r1_ohm: f64,
    /// Polarization time constant, s.
    pub tau1_s: f64,
}

impl Ecm1Rc {
    /// Parameters for the GW-01 rack (416 series cells, 314 Ah class LFP).
    /// Tuned so battery losses land where field data puts them for a 0.5C
    /// system; refined against calibration sources in M1.
    pub fn lfp_314ah_rack() -> Self {
        Self {
            r0_ohm: 0.28,
            r1_ohm: 0.06,
            tau1_s: 30.0,
        }
    }
}

/// Rack open-circuit voltage at a given SoC, V.
fn ocv_rack_v(soc: f64, cfg: &RackConfig) -> f64 {
    ocv_cell_v(soc) * cfg.cells_series as f64
}

/// Piecewise-linear interpolation of the cell OCV curve.
fn ocv_cell_v(soc: f64) -> f64 {
    let s = soc.clamp(0.0, 1.0);
    // 13 points: a linear scan beats binary search at this size.
    let mut prev = OCV_LFP_V[0];
    for &(x, v) in &OCV_LFP_V[1..] {
        if s <= x {
            let t = (s - prev.0) / (x - prev.0);
            return prev.1 + t * (v - prev.1);
        }
        prev = (x, v);
    }
    prev.1
}

/// Integral of the cell OCV curve from 0 to `soc`, in V (multiply by Ah for
/// energy). Exact for the piecewise-linear curve.
fn ocv_cell_integral_v(soc: f64) -> f64 {
    let s = soc.clamp(0.0, 1.0);
    let mut acc = 0.0;
    let mut prev = OCV_LFP_V[0];
    for &(x, v) in &OCV_LFP_V[1..] {
        if s <= x {
            let vm = ocv_cell_v(s);
            acc += (s - prev.0) * (prev.1 + vm) * 0.5;
            return acc;
        }
        acc += (x - prev.0) * (prev.1 + v) * 0.5;
        prev = (x, v);
    }
    acc
}

impl CellModel for Ecm1Rc {
    fn step_rack(
        &self,
        rack: &mut RackState,
        cfg: &RackConfig,
        p_request_w: f64,
        dt_s: f64,
    ) -> RackStepResult {
        let r0 = self.r0_ohm * rack.resistance_scale;
        let r1 = self.r1_ohm * rack.resistance_scale;
        let ocv = ocv_rack_v(rack.soc, cfg);
        let v1 = rack.polarization_v;

        if !rack.in_service {
            rack.current_a = 0.0;
            rack.polarization_v = v1 + dt_s / self.tau1_s * (-v1);
            rack.voltage_v = ocv - rack.polarization_v;
            return RackStepResult {
                p_dc_w: 0.0,
                heat_w: 0.0,
            };
        }

        // Terminal power P = (E - R0*I) * I with E = OCV - V1. Solve the
        // quadratic for I; the physically meaningful root is the small one.
        let e = ocv - v1;
        let p_max_w = e * e / (4.0 * r0);
        let mut p = p_request_w.min(0.999 * p_max_w);
        let mut i_a = if p.abs() < f64::EPSILON {
            0.0
        } else {
            (e - (e * e - 4.0 * r0 * p).sqrt()) / (2.0 * r0)
        };

        // Hard electrical and SoC bounds (the BMS tapers before these bind;
        // they are the model's own last line of defense).
        i_a = i_a.clamp(-cfg.max_current_a, cfg.max_current_a);
        let capacity_ah = cfg.cell_capacity_ah * rack.soh;
        let max_discharge_a = rack.soc * capacity_ah * 3600.0 / dt_s;
        let max_charge_a = (1.0 - rack.soc) * capacity_ah * 3600.0 / dt_s;
        i_a = i_a.clamp(-max_charge_a, max_discharge_a);
        p = (e - r0 * i_a) * i_a;

        rack.soc = (rack.soc - i_a * dt_s / (3600.0 * capacity_ah)).clamp(0.0, 1.0);
        rack.current_a = i_a;
        rack.voltage_v = e - r0 * i_a;
        rack.polarization_v = v1 + dt_s / self.tau1_s * (i_a * r1 - v1);

        // All chemical power that does not reach the terminals is booked as
        // heat, so the site energy balance closes exactly. During RC
        // transients this briefly includes energy moving in and out of the
        // polarization branch (bounded by its tiny stored energy).
        let heat_w = ocv * i_a - p;
        RackStepResult { p_dc_w: p, heat_w }
    }

    fn stored_energy_wh(&self, rack: &RackState, cfg: &RackConfig) -> f64 {
        let capacity_ah = cfg.cell_capacity_ah * rack.soh;
        ocv_cell_integral_v(rack.soc) * capacity_ah * cfg.cells_series as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::config::PlantConfig;

    fn rack() -> RackState {
        RackState {
            in_service: true,
            soc: 0.5,
            soh: 1.0,
            voltage_v: 0.0,
            current_a: 0.0,
            cell_temp_c: 25.0,
            polarization_v: 0.0,
            resistance_scale: 1.0,
            temp_offset_c: 0.0,
            alarm_bits: 0,
        }
    }

    #[test]
    fn ocv_curve_is_strictly_increasing() {
        for w in OCV_LFP_V.windows(2) {
            assert!(w[1].0 > w[0].0 && w[1].1 > w[0].1);
        }
    }

    #[test]
    fn discharge_reduces_soc_and_generates_heat() {
        let model = Ecm1Rc::lfp_314ah_rack();
        let cfg = PlantConfig::gw01().rack;
        let mut r = rack();
        let res = model.step_rack(&mut r, &cfg, 200.0e3, 1.0);
        assert!(r.soc < 0.5);
        assert!(r.current_a > 0.0);
        assert!(res.heat_w > 0.0);
        assert!((res.p_dc_w - 200.0e3).abs() < 1.0);
    }

    #[test]
    fn charge_raises_soc() {
        let model = Ecm1Rc::lfp_314ah_rack();
        let cfg = PlantConfig::gw01().rack;
        let mut r = rack();
        let res = model.step_rack(&mut r, &cfg, -200.0e3, 1.0);
        assert!(r.soc > 0.5);
        assert!(r.current_a < 0.0);
        assert!(res.heat_w > 0.0);
    }

    #[test]
    fn current_limit_is_enforced() {
        let model = Ecm1Rc::lfp_314ah_rack();
        let cfg = PlantConfig::gw01().rack;
        let mut r = rack();
        model.step_rack(&mut r, &cfg, 5.0e6, 1.0);
        assert!(r.current_a <= cfg.max_current_a + 1.0e-9);
    }

    #[test]
    fn energy_books_balance_over_a_short_cycle() {
        let model = Ecm1Rc::lfp_314ah_rack();
        let cfg = PlantConfig::gw01().rack;
        let mut r = rack();
        let e0 = model.stored_energy_wh(&r, &cfg);
        let mut terminal_wh = 0.0;
        let mut heat_wh = 0.0;
        for step in 0..1200 {
            let p = if step < 600 { 150.0e3 } else { -150.0e3 };
            let res = model.step_rack(&mut r, &cfg, p, 1.0);
            terminal_wh += res.p_dc_w / 3600.0;
            heat_wh += res.heat_w / 3600.0;
        }
        let e1 = model.stored_energy_wh(&r, &cfg);
        let residual = (e0 - e1) - (terminal_wh + heat_wh);
        // Residual is the RC branch's stored energy, a few Wh at most.
        assert!(residual.abs() < 5.0, "residual {residual} Wh");
    }
}
