//! Flat-efficiency PCS model.

use bess_core::state::{PcsOpState, PcsState};
use bess_core::traits::{PcsModel, PowerLimits};

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
    fn ac_capability_w(&self, dc_limits: &PowerLimits) -> PowerLimits {
        PowerLimits {
            max_discharge_w: (dc_limits.max_discharge_w * self.efficiency).min(self.rated_w),
            max_charge_w: (dc_limits.max_charge_w / self.efficiency).min(self.rated_w),
        }
    }

    fn dc_request_w(&self, p_ac_target_w: f64, dc_limits: &PowerLimits) -> f64 {
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
        let dc = pcs.dc_request_w(5.0e6, &wide_limits());
        assert!(dc > 5.0e6);
        let mut st = pcs_state();
        let ac = pcs.finalize(&mut st, dc);
        assert!((ac - 5.0e6).abs() < 1.0);
        assert!(st.loss_w > 0.0);
    }

    #[test]
    fn charge_request_accounts_for_losses() {
        let pcs = FlatPcs::new(5.0e6);
        let dc = pcs.dc_request_w(-5.0e6, &wide_limits());
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
        assert!(pcs.dc_request_w(5.0e6, &limits) <= 2.0e6);
        assert!(pcs.dc_request_w(-5.0e6, &limits) >= -1.0e6);
    }

    #[test]
    fn small_setpoints_drop_to_standby() {
        let pcs = FlatPcs::new(5.0e6);
        assert!(pcs.dc_request_w(500.0, &wide_limits()).abs() < f64::EPSILON);
        let mut st = pcs_state();
        pcs.finalize(&mut st, 0.0);
        assert_eq!(st.op_state, PcsOpState::Standby);
    }
}
