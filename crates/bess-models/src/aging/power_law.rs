//! Advancing a state whose trajectory is a power law, `y = k * x^p`, at
//! the trajectory's current slope.
//!
//! The rate coefficient `k` changes from day to day with the stress, so
//! the state is not re-evaluated from `x` (which would restart the curve
//! for every day's `k`) but moved by the slope the trajectory has at the
//! point the state has reached: `dy = k * p * (y / k)^((p - 1) / p) * dx`.
//! This is BLAST-Lite's `_update_power_state` (NREL, BSD 3-Clause), and it
//! is what makes a warm week add the loss a warm week adds to a cell this
//! old, not to a fresh one.

use crate::math::pow_pos;

/// The increment of a power-law state `y0` over a step `dx` of its
/// independent variable at rate `k` and exponent `p`. A state at zero
/// starts on the curve itself.
pub fn advance(y0: f64, dx: f64, k: f64, p: f64) -> f64 {
    if dx <= 0.0 || k <= 0.0 {
        return 0.0;
    }
    if y0 <= 0.0 {
        return k * pow_pos(dx, p);
    }
    k * p * pow_pos(y0 / k, (p - 1.0) / p) * dx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At constant rate the stepped state follows the closed form.
    #[test]
    fn constant_rate_follows_the_closed_form() {
        let (k, p) = (6.5e-4, 0.526);
        let mut y = 0.0;
        for _ in 0..1000 {
            y += advance(y, 1.0, k, p);
        }
        let closed = k * 1000f64.powf(p);
        // The first step is exact, every later one integrates the slope
        // over a unit step: a percent-level discretization at this p.
        assert!(((y - closed) / closed).abs() < 2.0e-2, "{y} vs {closed}");
        // Finer steps converge.
        let mut fine = 0.0;
        for _ in 0..100_000 {
            fine += advance(fine, 0.01, k, p);
        }
        assert!(
            ((fine - closed) / closed).abs() < 2.0e-4,
            "{fine} vs {closed}"
        );
    }

    /// A rate change integrates at the current slope: the result lands
    /// between the two single-rate trajectories, nearer the second.
    #[test]
    fn a_rate_change_continues_the_curve() {
        let p = 0.526;
        let (k1, k2) = (5.0e-4, 1.0e-3);
        let mut y = 0.0;
        for _ in 0..500 {
            y += advance(y, 1.0, k1, p);
        }
        for _ in 0..500 {
            y += advance(y, 1.0, k2, p);
        }
        let low = k1 * 1000f64.powf(p);
        let high = k2 * 1000f64.powf(p);
        assert!(y > low && y < high, "{low} < {y} < {high}");
    }

    #[test]
    fn nothing_moves_without_a_step() {
        assert_eq!(advance(0.1, 0.0, 1.0e-3, 0.5), 0.0);
        assert_eq!(advance(0.1, 1.0, 0.0, 0.5), 0.0);
    }
}
