//! PCS conversion models: flat efficiency (M0), the partial-load
//! efficiency curve (M0.5), and the surface with its DC-voltage axis (M3).

pub mod sandia;

use bess_core::state::{PcsOpState, PcsState};
use bess_core::traits::{PcsModel, PowerLimits};

pub use sandia::{SandiaSurface, CEC_SC2500UD_US};

/// M0 power conversion: a single conversion efficiency in both directions
/// plus the AC rating clamp. The two-dimensional efficiency map, the
/// operating state machine, and setpoint response dynamics arrive in M3.
///
/// Sign convention: positive power = discharge. On discharge the AC side
/// sees `P_dc * eta`; on charge the DC side receives `P_ac * eta`.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatPcs {
    /// AC rating, W.
    pub rated_w: f64,
    /// One-way conversion efficiency.
    pub efficiency: f64,
    /// Below this AC magnitude the unit drops to standby, W.
    pub standby_threshold_w: f64,
}

impl FlatPcs {
    /// Flat 97.5% unit at the given AC rating (partial-load behavior is an
    /// M3 concern; 97.5% approximates the broad flat top of utility-scale
    /// inverter curves including filter and self-supply losses).
    pub fn new(rated_w: f64) -> Self {
        Self {
            rated_w,
            efficiency: 0.975,
            standby_threshold_w: 1.0e3,
        }
    }
}

impl PcsModel for FlatPcs {
    fn ac_capability_w(&self, dc_limits: &PowerLimits, _dc_voltage_v: f64) -> PowerLimits {
        PowerLimits {
            max_discharge_w: (dc_limits.max_discharge_w * self.efficiency).min(self.rated_w),
            max_charge_w: (dc_limits.max_charge_w / self.efficiency).min(self.rated_w),
        }
    }

    fn dc_request_w(&self, p_ac_target_w: f64, dc_limits: &PowerLimits, _dc_voltage_v: f64) -> f64 {
        let ac = p_ac_target_w.clamp(-self.rated_w, self.rated_w);
        if ac.abs() < self.standby_threshold_w {
            return 0.0;
        }
        if ac > 0.0 {
            (ac / self.efficiency).min(dc_limits.max_discharge_w)
        } else {
            (ac * self.efficiency).max(-dc_limits.max_charge_w)
        }
    }

    fn finalize(&self, pcs: &mut PcsState, p_dc_actual_w: f64) -> f64 {
        let p_ac_w = if p_dc_actual_w > 0.0 {
            p_dc_actual_w * self.efficiency
        } else {
            p_dc_actual_w / self.efficiency
        };
        pcs.p_dc_w = p_dc_actual_w;
        pcs.p_ac_w = p_ac_w;
        // Positive in both directions: on discharge AC < DC, on charge
        // |DC| < |AC|; either way the difference is dissipated.
        pcs.loss_w = p_dc_actual_w - p_ac_w;
        if pcs.op_state != PcsOpState::Fault {
            pcs.op_state = if p_dc_actual_w.abs() < f64::EPSILON {
                PcsOpState::Standby
            } else {
                PcsOpState::Run
            };
        }
        p_ac_w
    }
}

/// M0.5 power conversion: one-way loss as a quadratic polynomial of the
/// AC-side load fraction, the standard three-coefficient inverter loss
/// model. With `p = |P_ac| / P_rated`:
///
/// ```text
/// L(p)   = (k0 + k1 p + k2 p^2) * P_rated      one-way loss, W
/// eta(p) = p / (p + k0 + k1 p + k2 p^2)        one-way efficiency
/// ```
///
/// `k0` is the load-independent part (switching, control, filters), `k2`
/// the ohmic part growing with the square of current; `k1` absorbs what is
/// left. The three terms are a fit family, not a strict physical
/// decomposition, so a slightly negative `k1` is acceptable.
///
/// Sign convention matches [`FlatPcs`]: positive power = discharge. Both
/// directions satisfy one signed equation, `P_dc = P_ac + L(|P_ac|)`; the
/// loss is supplied by the DC side on discharge and by the AC side on
/// charge, and `loss_w = P_dc - P_ac` stays positive either way, keeping
/// the energy-balance invariant intact. The charge direction reuses the
/// discharge curve (public sources rarely publish charge-direction
/// curves). The V_dc dimension of the map is M3 scope.
#[derive(Debug, Clone, PartialEq)]
pub struct CurvePcs {
    /// AC rating, W.
    pub rated_w: f64,
    /// Load-independent loss, fraction of the AC rating.
    pub k0: f64,
    /// Linear loss coefficient, dimensionless.
    pub k1: f64,
    /// Quadratic loss coefficient, dimensionless.
    pub k2: f64,
    /// Below this AC magnitude the unit drops to standby, W.
    pub standby_threshold_w: f64,
}

impl CurvePcs {
    /// Coefficients fitted to a real 1500 V utility-scale storage inverter:
    /// the Sungrow SC2500UD-US entry of the CEC inverter database (Sandia
    /// model coefficients as distributed with NREL SAM), evaluated at
    /// nominal DC voltage. The fit reproduces that curve within 0.037
    /// percentage points at the 5-100% load points (gate: < 0.3, see
    /// CALIBRATION.md). Coefficients are per-unit, so they scale to any AC
    /// rating.
    pub fn cec_utility_reference(rated_w: f64) -> Self {
        Self {
            rated_w,
            k0: 2.464_096e-3,
            k1: -1.402_627e-3,
            k2: 2.144_562e-2,
            standby_threshold_w: 1.0e3,
        }
    }

    /// One-way conversion loss at an AC-side magnitude, W (>= 0).
    fn loss_w(&self, p_ac_abs_w: f64) -> f64 {
        let p = p_ac_abs_w / self.rated_w;
        (self.k0 + self.k1 * p + self.k2 * p * p) * self.rated_w
    }

    /// One-way efficiency at an AC-side magnitude (0 when idle).
    pub fn efficiency(&self, p_ac_abs_w: f64) -> f64 {
        if p_ac_abs_w <= 0.0 {
            return 0.0;
        }
        p_ac_abs_w / (p_ac_abs_w + self.loss_w(p_ac_abs_w))
    }

    /// Solve `x + L(x) = p_dc` for the discharge AC output `x >= 0`.
    /// Returns 0 when the DC input cannot cover the no-load loss.
    fn discharge_ac_w(&self, p_dc_w: f64) -> f64 {
        let a = self.k2 / self.rated_w;
        let b = 1.0 + self.k1;
        let c = self.k0 * self.rated_w - p_dc_w;
        let disc = (b * b - 4.0 * a * c).max(0.0);
        ((-b + disc.sqrt()) / (2.0 * a)).max(0.0)
    }

    /// Solve `x - L(x) = |p_dc|` for the charge AC input magnitude `x`.
    /// The smaller quadratic root is the physical branch (the larger one
    /// sits beyond the loss parabola's turning point, far above rating).
    fn charge_ac_abs_w(&self, p_dc_abs_w: f64) -> f64 {
        let a = self.k2 / self.rated_w;
        let b = 1.0 - self.k1;
        let c = self.k0 * self.rated_w + p_dc_abs_w;
        let disc = (b * b - 4.0 * a * c).max(0.0);
        (b - disc.sqrt()) / (2.0 * a)
    }
}

impl PcsModel for CurvePcs {
    fn ac_capability_w(&self, dc_limits: &PowerLimits, _dc_voltage_v: f64) -> PowerLimits {
        PowerLimits {
            max_discharge_w: self
                .discharge_ac_w(dc_limits.max_discharge_w)
                .min(self.rated_w),
            max_charge_w: self
                .charge_ac_abs_w(dc_limits.max_charge_w)
                .min(self.rated_w),
        }
    }

    fn dc_request_w(&self, p_ac_target_w: f64, dc_limits: &PowerLimits, _dc_voltage_v: f64) -> f64 {
        let ac = p_ac_target_w.clamp(-self.rated_w, self.rated_w);
        if ac.abs() < self.standby_threshold_w {
            return 0.0;
        }
        let dc = ac + self.loss_w(ac.abs());
        if ac > 0.0 {
            dc.min(dc_limits.max_discharge_w)
        } else {
            // A small charge target may not cover the no-load loss (dc > 0
            // would mean discharging the battery to run the converter);
            // stand by instead until M1 models standby draw explicitly.
            dc.min(0.0).max(-dc_limits.max_charge_w)
        }
    }

    fn finalize(&self, pcs: &mut PcsState, p_dc_actual_w: f64) -> f64 {
        let p_ac_w = if p_dc_actual_w > 0.0 {
            self.discharge_ac_w(p_dc_actual_w)
        } else if p_dc_actual_w < 0.0 {
            -self.charge_ac_abs_w(-p_dc_actual_w)
        } else {
            0.0
        };
        pcs.p_dc_w = p_dc_actual_w;
        pcs.p_ac_w = p_ac_w;
        pcs.loss_w = p_dc_actual_w - p_ac_w;
        if pcs.op_state != PcsOpState::Fault {
            pcs.op_state = if p_dc_actual_w.abs() < f64::EPSILON {
                PcsOpState::Standby
            } else {
                PcsOpState::Run
            };
        }
        p_ac_w
    }
}

/// M3 power conversion: the Sandia surface at the DC bus voltage the racks
/// present, so conversion loss follows state of charge, current and
/// hysteresis through the voltage they set. Sign convention as
/// [`CurvePcs`]; the charge direction mirrors the discharge surface (the
/// database publishes one direction), the M0.5 simplification carried.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfacePcs {
    /// AC rating, W.
    pub rated_w: f64,
    /// The surface, per unit of the rating.
    pub surface: SandiaSurface,
    /// Below this AC magnitude the unit drops to standby, W.
    pub standby_threshold_w: f64,
}

impl SurfacePcs {
    /// The CEC record of the Sungrow SC2500UD-US scaled to the block's
    /// rating (`sandia::CEC_SC2500UD_US`).
    pub fn cec_sc2500ud_us(rated_w: f64) -> Self {
        Self {
            rated_w,
            surface: CEC_SC2500UD_US,
            standby_threshold_w: 1.0e3,
        }
    }

    /// One-way efficiency at an AC-side magnitude and a DC voltage.
    pub fn efficiency_at(&self, p_ac_abs_w: f64, dc_voltage_v: f64) -> f64 {
        self.surface
            .efficiency(p_ac_abs_w / self.rated_w, dc_voltage_v)
    }
}

impl PcsModel for SurfacePcs {
    fn ac_capability_w(&self, dc_limits: &PowerLimits, dc_voltage_v: f64) -> PowerLimits {
        let r = self.rated_w;
        PowerLimits {
            max_discharge_w: (r * self
                .surface
                .ac_pu(dc_limits.max_discharge_w / r, dc_voltage_v))
            .min(r),
            max_charge_w: (r * self.surface.dc_pu(dc_limits.max_charge_w / r, dc_voltage_v)).min(r),
        }
    }

    fn dc_request_w(&self, p_ac_target_w: f64, dc_limits: &PowerLimits, dc_voltage_v: f64) -> f64 {
        let r = self.rated_w;
        let ac = p_ac_target_w.clamp(-r, r);
        if ac.abs() < self.standby_threshold_w {
            return 0.0;
        }
        if ac > 0.0 {
            (r * self.surface.dc_pu(ac / r, dc_voltage_v)).min(dc_limits.max_discharge_w)
        } else {
            // What the DC side receives from an AC input: the mirrored
            // surface; a small target that cannot cover self-consumption
            // stands by rather than discharging the racks to run itself.
            let dc = -r * self.surface.ac_pu(-ac / r, dc_voltage_v);
            dc.min(0.0).max(-dc_limits.max_charge_w)
        }
    }

    fn finalize(&self, pcs: &mut PcsState, p_dc_actual_w: f64) -> f64 {
        let r = self.rated_w;
        let v = pcs.dc_voltage_v;
        let p_ac_w = if p_dc_actual_w > 0.0 {
            r * self.surface.ac_pu(p_dc_actual_w / r, v)
        } else if p_dc_actual_w < 0.0 {
            -r * self.surface.dc_pu(-p_dc_actual_w / r, v)
        } else {
            0.0
        };
        pcs.p_dc_w = p_dc_actual_w;
        pcs.p_ac_w = p_ac_w;
        pcs.loss_w = p_dc_actual_w - p_ac_w;
        if pcs.op_state != PcsOpState::Fault {
            pcs.op_state = if p_dc_actual_w.abs() < f64::EPSILON {
                PcsOpState::Standby
            } else {
                PcsOpState::Run
            };
        }
        p_ac_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcs_state() -> PcsState {
        PcsState {
            op_state: PcsOpState::Standby,
            p_ac_setpoint_w: 0.0,
            p_ac_w: 0.0,
            p_dc_w: 0.0,
            loss_w: 0.0,
            dc_voltage_v: 1_331.2,
        }
    }

    fn wide_limits() -> PowerLimits {
        PowerLimits {
            max_charge_w: 100.0e6,
            max_discharge_w: 100.0e6,
        }
    }

    #[test]
    fn discharge_request_accounts_for_losses() {
        let pcs = FlatPcs::new(5.0e6);
        let dc = pcs.dc_request_w(5.0e6, &wide_limits(), 1_331.2);
        assert!(dc > 5.0e6);
        let mut st = pcs_state();
        let ac = pcs.finalize(&mut st, dc);
        assert!((ac - 5.0e6).abs() < 1.0);
        assert!(st.loss_w > 0.0);
    }

    #[test]
    fn charge_request_accounts_for_losses() {
        let pcs = FlatPcs::new(5.0e6);
        let dc = pcs.dc_request_w(-5.0e6, &wide_limits(), 1_331.2);
        assert!(dc > -5.0e6 && dc < 0.0);
        let mut st = pcs_state();
        let ac = pcs.finalize(&mut st, dc);
        assert!((ac + 5.0e6).abs() < 1.0);
        assert!(st.loss_w > 0.0);
    }

    #[test]
    fn dc_limits_cap_the_request() {
        let pcs = FlatPcs::new(5.0e6);
        let limits = PowerLimits {
            max_charge_w: 1.0e6,
            max_discharge_w: 2.0e6,
        };
        assert!(pcs.dc_request_w(5.0e6, &limits, 1_331.2) <= 2.0e6);
        assert!(pcs.dc_request_w(-5.0e6, &limits, 1_331.2) >= -1.0e6);
    }

    #[test]
    fn small_setpoints_drop_to_standby() {
        let pcs = FlatPcs::new(5.0e6);
        assert!(pcs.dc_request_w(500.0, &wide_limits(), 1_331.2).abs() < f64::EPSILON);
        let mut st = pcs_state();
        pcs.finalize(&mut st, 0.0);
        assert_eq!(st.op_state, PcsOpState::Standby);
    }

    /// M0.5 calibration gate: the fitted coefficients reproduce the
    /// reference curve (Sungrow SC2500UD-US, Sandia model at nominal DC
    /// voltage, CEC inverter database via NREL SAM) within 0.3 percentage
    /// points at every published load point. See CALIBRATION.md.
    #[test]
    fn curve_matches_the_cec_reference_within_the_gate() {
        let rated = 5.0e6;
        let pcs = CurvePcs::cec_utility_reference(rated);
        let reference = [
            (0.05, 0.95370),
            (0.10, 0.97527),
            (0.20, 0.98493),
            (0.30, 0.98686),
            (0.50, 0.98595),
            (0.75, 0.98239),
            (1.00, 0.97797),
        ];
        for (frac, eta_ref) in reference {
            let eta = pcs.efficiency(frac * rated);
            assert!(
                (eta - eta_ref).abs() < 0.003,
                "at {:.0}% load: eta {eta:.5} vs reference {eta_ref:.5}",
                frac * 100.0
            );
        }
    }

    #[test]
    fn curve_request_then_finalize_recovers_the_ac_target() {
        let pcs = CurvePcs::cec_utility_reference(5.0e6);
        for target in [-5.0e6, -2.5e6, -0.5e6, 0.25e6, 1.5e6, 5.0e6] {
            let dc = pcs.dc_request_w(target, &wide_limits(), 1_331.2);
            let mut st = pcs_state();
            let ac = pcs.finalize(&mut st, dc);
            assert!(
                (ac - target).abs() < 1.0e-3,
                "target {target} came back as {ac} (dc {dc})"
            );
            assert!(st.loss_w > 0.0, "loss must be positive at {target}");
        }
    }

    #[test]
    fn curve_efficiency_peaks_at_partial_load_and_collapses_when_shallow() {
        let rated = 5.0e6;
        let pcs = CurvePcs::cec_utility_reference(rated);
        let eta = |frac: f64| pcs.efficiency(frac * rated);
        assert!(eta(0.02) < 0.92, "2% load should be deep in the knee");
        assert!(eta(0.30) > eta(0.10));
        assert!(eta(0.30) > eta(1.00), "full load sits below the peak");
        assert!(eta(1.00) > 0.97);
    }

    #[test]
    fn curve_charge_below_the_no_load_loss_stands_by() {
        let pcs = CurvePcs::cec_utility_reference(5.0e6);
        // 5 kW charge target cannot cover the ~12 kW no-load loss; the unit
        // must not silently discharge the battery to run itself.
        let dc = pcs.dc_request_w(-5.0e3, &wide_limits(), 1_331.2);
        assert!(dc.abs() < f64::EPSILON, "got dc {dc}");
    }

    #[test]
    fn curve_capability_is_consistent_with_finalize() {
        let pcs = CurvePcs::cec_utility_reference(5.0e6);
        let limits = PowerLimits {
            max_charge_w: 3.0e6,
            max_discharge_w: 4.0e6,
        };
        let cap = pcs.ac_capability_w(&limits, 1_331.2);
        let mut st = pcs_state();
        let ac = pcs.finalize(&mut st, limits.max_discharge_w);
        assert!((ac - cap.max_discharge_w).abs() < 1.0e-3);
        let ac = pcs.finalize(&mut st, -limits.max_charge_w);
        assert!((-ac - cap.max_charge_w).abs() < 1.0e-3);
    }
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    fn pcs_state(vdc: f64) -> PcsState {
        PcsState {
            op_state: PcsOpState::Standby,
            p_ac_setpoint_w: 0.0,
            p_ac_w: 0.0,
            p_dc_w: 0.0,
            loss_w: 0.0,
            dc_voltage_v: vdc,
        }
    }

    fn wide() -> PowerLimits {
        PowerLimits {
            max_charge_w: 100.0e6,
            max_discharge_w: 100.0e6,
        }
    }

    /// M3 phase 1 gate: at the record's reference voltage the surface
    /// reproduces the M0.5 reference points within the M0.5 tolerance.
    #[test]
    fn surface_matches_the_cec_reference_at_vdco_within_the_gate() {
        let pcs = SurfacePcs::cec_sc2500ud_us(5.0e6);
        let reference = [
            (0.05, 0.95370),
            (0.10, 0.97527),
            (0.20, 0.98493),
            (0.30, 0.98686),
            (0.50, 0.98595),
            (0.75, 0.98239),
            (1.00, 0.97797),
        ];
        for (frac, eta_ref) in reference {
            let eta = pcs.efficiency_at(frac * 5.0e6, CEC_SC2500UD_US.vdco_v);
            assert!(
                (eta - eta_ref).abs() < 0.003,
                "at {frac}: {eta} vs {eta_ref}"
            );
        }
    }

    #[test]
    fn surface_request_then_finalize_recovers_the_ac_target_at_any_voltage() {
        let pcs = SurfacePcs::cec_sc2500ud_us(5.0e6);
        for vdc in [1_206.0, 1_331.2, 1_435.0] {
            for target in [-5.0e6, -2.5e6, -0.5e6, 0.25e6, 1.5e6, 5.0e6] {
                let dc = pcs.dc_request_w(target, &wide(), vdc);
                let mut st = pcs_state(vdc);
                let ac = pcs.finalize(&mut st, dc);
                assert!(
                    (ac - target).abs() < 1.0e-3,
                    "{vdc} V target {target}: {ac}"
                );
                assert!(st.loss_w > 0.0);
            }
        }
    }

    #[test]
    fn surface_capability_is_consistent_with_finalize() {
        let pcs = SurfacePcs::cec_sc2500ud_us(5.0e6);
        let limits = PowerLimits {
            max_charge_w: 3.0e6,
            max_discharge_w: 4.0e6,
        };
        let vdc = 1_300.0;
        let cap = pcs.ac_capability_w(&limits, vdc);
        let mut st = pcs_state(vdc);
        let ac = pcs.finalize(&mut st, limits.max_discharge_w);
        assert!((ac - cap.max_discharge_w).abs() < 1.0e-3);
        let ac = pcs.finalize(&mut st, -limits.max_charge_w);
        assert!((-ac - cap.max_charge_w).abs() < 1.0e-3);
    }

    #[test]
    fn surface_small_charge_stands_by() {
        let pcs = SurfacePcs::cec_sc2500ud_us(5.0e6);
        assert!(pcs.dc_request_w(-5.0e3, &wide(), 1_331.2).abs() < f64::EPSILON);
    }
}
