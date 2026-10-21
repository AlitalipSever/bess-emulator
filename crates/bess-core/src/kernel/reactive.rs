//! Reactive power at the plant's choice (M4 phase 1 PR2): the site target
//! in the mode the operator set, filtered, and shared over the converters.
//!
//! Three modes, VDE-AR-N 4110's: a fixed displacement factor, a fixed
//! reactive power, and a voltage characteristic Q(U) with a dead band
//! around nominal and a slope that reaches the connection rule's range
//! (cos φ 0.9 at rated active power) at ±5 % voltage. The target passes a
//! first-order filter with a 10 s time constant, the settling the rule
//! asks of a Q(U) step (labeled; CALIBRATION.md). The converters' circle
//! bounds what each one delivers (`PcsModel::reactive`); the transformer
//! consumes its share on the way to the POI (`GridInterface::step`).
//!
//! Sign: positive reactive power is over-excited (vars delivered to the
//! grid), the generator convention every published register model uses.
//! Rational arithmetic and one square root.

use crate::state::{EmsState, ReactiveMode};

/// The plant's reactive-power rules.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactiveControl {
    /// Time constant of the first-order settling on the site target, s.
    pub tau_s: f64,
    /// Q(U): half-width of the dead band around nominal, per unit.
    pub deadband_pu: f64,
    /// Q(U): voltage deviation at which the full range is reached, per
    /// unit.
    pub full_pu: f64,
    /// The range the characteristic reaches, as a share of rated active
    /// power: tan φ at the rule's cos φ 0.9, 0.4843.
    pub range_share: f64,
}

impl Default for ReactiveControl {
    /// 10 s settling, a ±2 % dead band, the full range at ±5 %, the range
    /// of cos φ 0.9. Labeled estimates except the range (VDE-AR-N 4110).
    fn default() -> Self {
        Self {
            tau_s: 10.0,
            deadband_pu: 0.02,
            full_pu: 0.05,
            range_share: (1.0 - 0.9_f64 * 0.9).sqrt() / 0.9,
        }
    }
}

impl ReactiveControl {
    /// The unfiltered site target, var, for `mode` at the site's active
    /// power `p_site_w` and POI voltage `voltage_pu`, with `rated_w` the
    /// site rating.
    pub fn target_var(
        &self,
        mode: ReactiveMode,
        p_site_w: f64,
        voltage_pu: f64,
        rated_w: f64,
    ) -> f64 {
        match mode {
            ReactiveMode::FixedCosPhi {
                cos_phi,
                under_excited,
            } => {
                let c = cos_phi.clamp(0.01, 1.0);
                let tan_phi = (1.0 - c * c).sqrt() / c;
                let q = p_site_w.abs() * tan_phi;
                if under_excited {
                    -q
                } else {
                    q
                }
            }
            ReactiveMode::FixedQ { var } => var,
            ReactiveMode::QofU => {
                let dev = voltage_pu - 1.0;
                let beyond = (dev.abs() - self.deadband_pu).max(0.0);
                let share = (beyond / (self.full_pu - self.deadband_pu)).min(1.0);
                // A high voltage asks for under-excited (absorbing) vars.
                -dev.signum() * share * self.range_share * rated_w
            }
        }
    }

    /// Advance the site target one tick toward `raw_var` through the
    /// filter; returns the filtered target, left in `ems.q_site_target_var`.
    pub fn settle(&self, ems: &mut EmsState, raw_var: f64, dt_s: f64) -> f64 {
        let y = ems.q_site_target_var;
        let k = dt_s / (self.tau_s + dt_s);
        ems.q_site_target_var = y + (raw_var - y) * k;
        ems.q_site_target_var
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATED_W: f64 = 100.0e6;

    #[test]
    fn the_three_modes_give_their_targets() {
        let rc = ReactiveControl::default();
        let unity = ReactiveMode::FixedCosPhi {
            cos_phi: 1.0,
            under_excited: false,
        };
        assert_eq!(rc.target_var(unity, 80.0e6, 1.0, RATED_W), 0.0);
        let lagging = ReactiveMode::FixedCosPhi {
            cos_phi: 0.9,
            under_excited: true,
        };
        let q = rc.target_var(lagging, -50.0e6, 1.0, RATED_W);
        assert!((q + 50.0e6 * 0.4843).abs() < 1.0e4, "{q}");
        assert_eq!(
            rc.target_var(ReactiveMode::FixedQ { var: 7.0e6 }, 0.0, 1.0, RATED_W),
            7.0e6
        );
        assert_eq!(rc.target_var(ReactiveMode::QofU, 0.0, 1.015, RATED_W), 0.0);
        let full = rc.target_var(ReactiveMode::QofU, 0.0, 1.06, RATED_W);
        assert!((full + rc.range_share * RATED_W).abs() < 1.0, "{full}");
        let half = rc.target_var(ReactiveMode::QofU, 0.0, 0.965, RATED_W);
        assert!(
            (half - 0.5 * rc.range_share * RATED_W).abs() < 1.0,
            "{half}"
        );
    }

    #[test]
    fn the_filter_settles_in_a_few_time_constants() {
        let rc = ReactiveControl::default();
        let mut ems = EmsState::default_for_tests();
        for _ in 0..30 {
            rc.settle(&mut ems, 10.0e6, 1.0);
        }
        assert!(ems.q_site_target_var > 9.4e6 && ems.q_site_target_var < 10.0e6);
    }
}
