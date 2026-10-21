//! OCV hysteresis of the LFP cell: Plett's one-state model.
//!
//! A rested LFP cell does not sit on one open-circuit curve. After charging
//! it rests above the mean curve, after discharging below it, and the gap
//! persists for hours, which is what makes rested voltage a poor oracle
//! for SoC on the plateau. The model is the standard equivalent-circuit
//! extension: one state `h` per rack, driven by throughput toward a bounded
//! envelope whose sign follows the current.
//!
//! ```text
//! dh/dt = -|i| * gamma / Q * (h - h_max(soc) * s)      s = -sign(i)
//! ```
//!
//! with `i` positive on discharge as everywhere in the tree, so charging
//! pushes `h` toward `+h_max` and discharging toward `-h_max`. At rest the
//! right-hand side is zero and `h` holds. The envelope is half the measured
//! charge-minus-discharge rest-voltage gap, so the two branches sit at
//! `OCV(soc) +- h_max(soc)`.

/// Half the charge-minus-discharge rest-voltage gap of the cell, V, against
/// SoC: the envelope `h` is bounded by. Piecewise linear.
///
/// Shape and peak from Barai et al. 2015 (J. Power Sources 295), a 20 Ah
/// LFP pouch cell at 25 C under 4 % steps at 1C with 4 h rests: the gap
/// peaks at 38 mV near 5 % SoC, and the maximum lies within 5 to 25 % SoC.
/// The plateau level is an estimate inside the range the literature
/// reports and is gated as a sanity bound, not a sourced one; the same
/// authors' 2016 ITEC paper has the 25 C gap within 3 mV of the 45 C gap
/// and the mean curve moving only 7 mV between 30 and 60 % SoC, which is
/// why one envelope at one temperature is carried. Sources and the gate in
/// CALIBRATION.md.
pub const LFP_HALF_GAP_V: &[(f64, f64)] = &[
    (0.00, 0.012),
    (0.05, 0.019),
    (0.15, 0.017),
    (0.25, 0.014),
    (0.40, 0.011),
    (0.50, 0.010),
    (0.60, 0.0095),
    (0.70, 0.009),
    (0.80, 0.008),
    (0.90, 0.006),
    (1.00, 0.003),
];

/// Parameters of the one-state hysteresis model.
#[derive(Debug, Clone, PartialEq)]
pub struct Hysteresis {
    /// Transition rate, dimensionless: how much SoC throughput moves `h`
    /// most of the way across the envelope. `1 - exp(-gamma * dz)` of the
    /// remaining distance is covered by a throughput `dz`; zero switches the
    /// model off and leaves `h` wherever it is.
    pub gamma: f64,
    /// Half-gap envelope per cell, (SoC, V).
    pub half_gap: &'static [(f64, f64)],
}

impl Hysteresis {
    /// GW-01 default: the LFP envelope above and a rate that covers 95 % of
    /// a transition within one 4 % SoC step walked tick by tick, the step
    /// size of the pinned characterization, after which its measured rest
    /// voltage lay on the new branch. The rate is an interpretation of
    /// that protocol, since the source publishes no intermediate rest
    /// points, and is labeled so in CALIBRATION.md.
    pub fn lfp_barai_2015() -> Self {
        Self {
            gamma: 75.0,
            half_gap: LFP_HALF_GAP_V,
        }
    }

    /// Hysteresis switched off: `h` never moves. With a rack that starts
    /// at zero this is the pre-M2.5 cell model, byte for byte.
    pub fn off() -> Self {
        Self {
            gamma: 0.0,
            half_gap: LFP_HALF_GAP_V,
        }
    }

    /// Envelope at a SoC, V per cell.
    pub fn half_gap_cell_v(&self, soc: f64) -> f64 {
        let s = soc.clamp(0.0, 1.0);
        let mut prev = self.half_gap[0];
        for &(x, v) in &self.half_gap[1..] {
            if s <= x {
                let t = (s - prev.0) / (x - prev.0);
                return prev.1 + t * (v - prev.1);
            }
            prev = (x, v);
        }
        prev.1
    }

    /// Advance the rack hysteresis state by one tick.
    ///
    /// `h_v` is the rack-level state (cell value times `cells_series`),
    /// `i_a` the rack current (positive = discharge), `capacity_ah` the
    /// rack's capacity at its state of health. Implicit Euler over the
    /// tick, `h' = (h + gamma dz target) / (1 + gamma dz)`, rather than
    /// the exact exponential: the kernel's determinism contract is
    /// byte-identity across platforms, and `exp` is the one thing in the
    /// C library that macOS and glibc round differently. Rational
    /// arithmetic is IEEE-exact everywhere; at a tick's `gamma dz` (under
    /// 0.03 at 1C) the two differ by well under a percent of a transition.
    /// Then clamped into the envelope at the new SoC so the containment
    /// invariant holds whatever the SoC did.
    pub fn step(
        &self,
        h_v: f64,
        soc_after: f64,
        i_a: f64,
        dt_s: f64,
        capacity_ah: f64,
        cells_series: usize,
    ) -> f64 {
        let bound_v = self.half_gap_cell_v(soc_after) * cells_series as f64;
        if i_a == 0.0 || self.gamma == 0.0 {
            return h_v.clamp(-bound_v, bound_v);
        }
        let dz = i_a.abs() * dt_s / (3600.0 * capacity_ah);
        let pull = self.gamma * dz;
        let target_v = if i_a > 0.0 { -bound_v } else { bound_v };
        ((h_v + pull * target_v) / (1.0 + pull)).clamp(-bound_v, bound_v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELLS: usize = 416;
    const Q_AH: f64 = 314.0;

    #[test]
    fn envelope_is_positive_and_peaks_in_the_low_knee() {
        let h = Hysteresis::lfp_barai_2015();
        let peak = LFP_HALF_GAP_V
            .iter()
            .copied()
            .fold((0.0, 0.0), |m, p| if p.1 > m.1 { p } else { m });
        assert!((0.05..=0.25).contains(&peak.0), "peak at SoC {}", peak.0);
        for s in 0..=100 {
            let v = h.half_gap_cell_v(f64::from(s) / 100.0);
            assert!(v > 0.0 && v <= peak.1);
        }
    }

    #[test]
    fn state_stays_inside_the_envelope_and_chases_the_sign() {
        let h = Hysteresis::lfp_barai_2015();
        let mut state = 0.0;
        let mut soc = 0.5;
        // Charge a while, then discharge; the state must never leave the
        // envelope and must end up on the side the current points to.
        for step in 0..7200 {
            let i_a = if step < 3600 { -100.0 } else { 100.0 };
            soc = (soc - i_a / (3600.0 * Q_AH)).clamp(0.0, 1.0);
            state = h.step(state, soc, i_a, 1.0, Q_AH, CELLS);
            let bound = h.half_gap_cell_v(soc) * CELLS as f64;
            assert!(state.abs() <= bound + 1.0e-12);
            if step == 3599 {
                assert!(state > 0.9 * bound, "charging did not raise h: {state}");
            }
        }
        let bound = h.half_gap_cell_v(soc) * CELLS as f64;
        assert!(state < -0.9 * bound, "discharging did not lower h: {state}");
    }

    #[test]
    fn rest_holds_the_state() {
        let h = Hysteresis::lfp_barai_2015();
        let before = 3.0;
        let after = h.step(before, 0.5, 0.0, 3600.0, Q_AH, CELLS);
        assert_eq!(before, after);
    }

    #[test]
    fn switched_off_never_moves() {
        let h = Hysteresis::off();
        assert_eq!(h.step(0.0, 0.5, -300.0, 1.0, Q_AH, CELLS), 0.0);
        assert_eq!(h.step(0.0, 0.1, 300.0, 1.0, Q_AH, CELLS), 0.0);
    }

    #[test]
    fn ninety_five_percent_of_a_transition_fits_one_source_step() {
        let h = Hysteresis::lfp_barai_2015();
        // From the discharge branch, charge exactly 4 % of capacity over
        // an hour, one second at a time, as the kernel would.
        let soc = 0.5;
        let start = -h.half_gap_cell_v(soc) * CELLS as f64;
        let i_a = -0.04 * Q_AH;
        let mut state = start;
        for _ in 0..3600 {
            state = h.step(state, soc, i_a, 1.0, Q_AH, CELLS);
        }
        let bound = h.half_gap_cell_v(soc) * CELLS as f64;
        let covered = (state - start) / (bound - start);
        assert!((0.94..=0.96).contains(&covered), "covered {covered}");
    }

    /// The step is rational arithmetic only: the same inputs give the
    /// same bits on every platform, which `exp` could not promise.
    #[test]
    fn the_step_is_bit_exact_rational_arithmetic() {
        let h = Hysteresis::lfp_barai_2015();
        let out = h.step(1.25, 0.5, -100.0, 1.0, Q_AH, CELLS);
        let pull = 75.0 * 100.0 / (3600.0 * Q_AH);
        let bound = h.half_gap_cell_v(0.5) * CELLS as f64;
        let expect = (1.25 + pull * bound) / (1.0 + pull);
        assert_eq!(out.to_bits(), expect.to_bits());
    }
}
