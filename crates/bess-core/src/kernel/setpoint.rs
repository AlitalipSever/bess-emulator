//! How a converter follows the target it is handed (M3 phase 2 PR2).
//!
//! The plant controller cuts each block a share of the site target; the
//! converter does not jump to it. The share passes a dead time (the tick
//! that carries it: a setpoint written between ticks acts on the next one,
//! and the filter starts from what the previous tick left), then a ramp
//! limit (the grid code's power gradient, a fixed slope in watts per
//! second), then a first-order lag (the current controller's settling).
//! Both filter values are state, so a resumed run continues the ramp it
//! was on. A converter that is not connected, or not in the dispatchable
//! set, follows nothing and both values rest at zero: the ramp starts from
//! zero when it reconnects, which is what `Ramp` means.
//!
//! Rational arithmetic only: the lag is an implicit Euler step, so the
//! kernel stays byte-identical across platforms (see the hysteresis
//! lesson in CALIBRATION.md).

use crate::state::PcsState;

/// The three parameters of the follower.
#[derive(Debug, Clone, PartialEq)]
pub struct SetpointDynamics {
    /// Ramp limit as a share of the converter's rating per second. `None`
    /// is no limit: the ramped target is the setpoint.
    pub gradient_pu_per_s: Option<f64>,
    /// Time constant of the first-order lag, s. Zero is no lag.
    pub lag_tau_s: f64,
}

impl Default for SetpointDynamics {
    /// GW-01 defaults. The gradient is VDE-AR-N 4110's 10 % of agreed
    /// power per minute (public), the figure the return's spacing already
    /// rests on, so a full step takes ten minutes. The lag is a labeled
    /// estimate: a converter's current controller settles in well under a
    /// tick, and 0.5 s makes the lag visible at the tick without
    /// dominating the ramp; see CALIBRATION.md.
    fn default() -> Self {
        Self {
            gradient_pu_per_s: Some(0.10 / 60.0),
            lag_tau_s: 0.5,
        }
    }
}

impl SetpointDynamics {
    /// The follower switched off: the converter is at its setpoint the
    /// tick it receives it, as it was before this PR. For research and
    /// for the switch-off proof.
    pub fn off() -> Self {
        Self {
            gradient_pu_per_s: None,
            lag_tau_s: 0.0,
        }
    }

    /// Advance one converter's filter by `dt_s` toward `pcs.p_ac_setpoint_w`
    /// when it is `following` (connected and in the dispatchable set),
    /// else rest it at zero. Leaves the followed target in
    /// `pcs.setpoint_followed_w`.
    pub(super) fn follow(&self, pcs: &mut PcsState, following: bool, rated_w: f64, dt_s: f64) {
        if !following {
            pcs.setpoint_ramped_w = 0.0;
            pcs.setpoint_followed_w = 0.0;
            return;
        }
        let target = pcs.p_ac_setpoint_w.clamp(-rated_w, rated_w);
        pcs.setpoint_ramped_w = match self.gradient_pu_per_s {
            Some(g) => {
                let step = g * rated_w * dt_s;
                let r = pcs.setpoint_ramped_w;
                (target).clamp(r - step, r + step)
            }
            None => target,
        };
        // Implicit Euler on y' = (x - y) / tau: y += (x - y) * dt / (tau + dt).
        let y = pcs.setpoint_followed_w;
        let k = dt_s / (self.lag_tau_s + dt_s);
        pcs.setpoint_followed_w = y + (pcs.setpoint_ramped_w - y) * k;
    }

    /// How far a connected block is still behind its (rating-clamped)
    /// setpoint because it is ramping, counted in the setpoint's own
    /// direction, W. What the site's shortfall discounts, so a legitimate
    /// ramp is not a power limitation. Zero once the follower has settled,
    /// and zero for a block asked for nothing.
    pub(super) fn ramp_slack_w(pcs: &PcsState, rated_w: f64) -> f64 {
        let target = pcs.p_ac_setpoint_w.clamp(-rated_w, rated_w);
        let behind = target - pcs.setpoint_followed_w;
        if target >= 0.0 {
            behind.max(0.0)
        } else {
            (-behind).max(0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PcsOpState;

    const RATED_W: f64 = 2.5e6;

    fn pcs(setpoint_w: f64) -> PcsState {
        PcsState {
            op_state: PcsOpState::Run,
            p_ac_setpoint_w: setpoint_w,
            p_ac_w: 0.0,
            p_dc_w: 0.0,
            loss_w: 0.0,
            dc_voltage_v: 1_331.2,
            state_elapsed_s: 0.0,
            setpoint_ramped_w: 0.0,
            setpoint_followed_w: 0.0,
        }
    }

    /// A full step is followed monotonically, never exceeds the gradient
    /// between ticks, never overshoots, and settles within the ramp time
    /// plus a few lag constants.
    #[test]
    fn a_step_is_followed_without_overshoot_and_inside_the_gradient() {
        let dynamics = SetpointDynamics::default();
        let mut p = pcs(RATED_W);
        let max_step = 0.10 / 60.0 * RATED_W;
        let mut last = 0.0;
        let mut settled_at = None;
        for tick in 1..=700u64 {
            dynamics.follow(&mut p, true, RATED_W, 1.0);
            let y = p.setpoint_followed_w;
            assert!(y >= last - 1e-9, "not monotone at tick {tick}");
            assert!(
                y - last <= max_step + 1e-6,
                "gradient exceeded at tick {tick}"
            );
            assert!(y <= RATED_W + 1e-9, "overshoot at tick {tick}");
            if settled_at.is_none() && RATED_W - y < 1.0 {
                settled_at = Some(tick);
            }
            last = y;
        }
        // Ten minutes of ramp, then the lag closes the last watt in a
        // handful of ticks.
        let settled = settled_at.expect("settled");
        assert!((600..=612).contains(&settled), "settled at {settled}");
    }

    /// Charge mirrors discharge, and the slack counts in the setpoint's
    /// direction either way.
    #[test]
    fn a_charge_step_mirrors_and_the_slack_is_signed_right() {
        let dynamics = SetpointDynamics::default();
        let mut p = pcs(-RATED_W);
        for _ in 0..300 {
            dynamics.follow(&mut p, true, RATED_W, 1.0);
        }
        assert!(p.setpoint_followed_w < -0.49 * RATED_W);
        assert!(p.setpoint_followed_w > -0.51 * RATED_W);
        let slack = SetpointDynamics::ramp_slack_w(&p, RATED_W);
        assert!(slack > 0.49 * RATED_W && slack < 0.51 * RATED_W);
        // A block already past its target in the setpoint's direction has
        // no slack: it is the alarm's business, not the ramp's.
        p.setpoint_followed_w = -RATED_W;
        p.p_ac_setpoint_w = -0.5 * RATED_W;
        assert_eq!(SetpointDynamics::ramp_slack_w(&p, RATED_W), 0.0);
    }

    /// Switched off, the converter is at its setpoint on the first tick.
    #[test]
    fn switched_off_the_follower_is_the_setpoint() {
        let mut p = pcs(0.7 * RATED_W);
        SetpointDynamics::off().follow(&mut p, true, RATED_W, 1.0);
        assert_eq!(p.setpoint_followed_w, 0.7 * RATED_W);
        assert_eq!(p.setpoint_ramped_w, 0.7 * RATED_W);
    }

    /// A converter that stops following rests at zero at once, and a
    /// setpoint beyond the rating ramps to the rating.
    #[test]
    fn not_following_rests_at_zero_and_the_rating_caps_the_target() {
        let dynamics = SetpointDynamics::default();
        let mut p = pcs(10.0 * RATED_W);
        for _ in 0..1000 {
            dynamics.follow(&mut p, true, RATED_W, 1.0);
        }
        assert!((p.setpoint_followed_w - RATED_W).abs() < 1.0);
        dynamics.follow(&mut p, false, RATED_W, 1.0);
        assert_eq!(p.setpoint_followed_w, 0.0);
        assert_eq!(p.setpoint_ramped_w, 0.0);
    }
}
