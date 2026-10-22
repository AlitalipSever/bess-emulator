//! NREL's capacity model for a large-format prismatic LFP cell.
//!
//! The model is BLAST-Lite's `Lfp_Gr_250AhPrismatic` (Paul Gasper, NREL;
//! BSD 3-Clause, copyright 2023 Alliance for Energy Innovation, LLC),
//! identified on the aging data of a commercial prismatic LFP cell above
//! 250 Ah with a six-hour energy-to-power ratio, reported in Gasper et al.,
//! J. Energy Storage 2023 (doi 10.1016/j.est.2023.109042). Two power-law
//! trajectories: calendar loss in days, cycle loss in equivalent full
//! cycles, each with a rate coefficient in the stress. The model's own
//! limits, carried over: the cycling data was taken at conservative rates
//! (0.65C charge at most), and the cycle coefficient's temperature term
//! was fitted to balance the calendar term inside 10 to 45 °C, so the
//! model is not for extrapolation outside that range. Only the constants
//! are carried; the code is the project's own.

use crate::math::{exp_det, tanh_det};

/// The ten parameters of the capacity model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NrelLfp250 {
    /// Calendar rate: the pre-exponential, 1/day^pcal.
    pub p1: f64,
    /// Calendar rate: the Arrhenius term, kelvin.
    pub p2: f64,
    /// Calendar rate: the anode-potential term, kelvin per volt.
    pub p3: f64,
    /// Calendar exponent on days.
    pub pcal: f64,
    /// Cycle rate: the constant term.
    pub p4: f64,
    /// Cycle rate: per unit of depth of discharge.
    pub p5: f64,
    /// Cycle rate: per unit of C-rate.
    pub p6: f64,
    /// Cycle rate: the rising temperature term, kelvin.
    pub p7: f64,
    /// Cycle rate: the falling temperature term, kelvin.
    pub p8: f64,
    /// Cycle exponent on equivalent full cycles.
    pub pcyc: f64,
}

impl NrelLfp250 {
    /// The published parameters.
    pub const fn blast_lite_2023() -> Self {
        Self {
            p1: 8.37e4,
            p2: -5.21e3,
            p3: -3.56e3,
            pcal: 0.526,
            p4: 4.38e-8,
            p5: 1.55e-8,
            p6: 1.68e-7,
            p7: 2.19e3,
            p8: 1.55e5,
            pcyc: 0.828,
        }
    }

    /// Calendar rate coefficient at a mean temperature and state of charge.
    pub fn k_cal(&self, temp_k: f64, soc: f64) -> f64 {
        let ua = anode_potential_v(soc);
        self.p1 * exp_det(self.p2 / temp_k) * exp_det(self.p3 * ua / temp_k)
    }

    /// Cycle rate coefficient at a mean temperature, depth and rate.
    pub fn k_cyc(&self, temp_k: f64, dod: f64, c_rate: f64) -> f64 {
        (self.p4 + self.p5 * dod + self.p6 * c_rate)
            * (exp_det(self.p7 / temp_k) + exp_det(-self.p8 / temp_k))
    }
}

/// The graphite anode's potential against lithium at a state of charge,
/// V: Safari and Delacourt's function (J. Electrochem. Soc. 158 (2011)
/// A562) on the lithiation map of Schimpe et al. (J. Electrochem. Soc. 165
/// (2018) A181), as BLAST-Lite computes it. A guess at where the anode's
/// plateaus sit, the model's own words, and the term that makes calendar
/// loss rise with the state of charge.
pub fn anode_potential_v(soc: f64) -> f64 {
    let xa = 8.5e-3 + soc.clamp(0.0, 1.0) * (0.78 - 8.5e-3);
    0.6379 + 0.5416 * exp_det(-305.5309 * xa) + 0.044 * tanh_det(-(xa - 0.1958) / 0.1088)
        - 0.1978 * tanh_det((xa - 1.0571) / 0.0854)
        - 0.6875 * tanh_det((xa + 0.0117) / 0.0529)
        - 0.0175 * tanh_det((xa - 0.5692) / 0.0875)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::pow_pos;

    const T25_K: f64 = 298.15;

    /// The design's year-one figures at the stated stress (D1): 1.10 %
    /// calendar at 25 °C and a mean state of charge of 0.5, 2.31 % cycle
    /// at 0.8 depth, 0.5C and 280 cycles.
    #[test]
    fn year_one_at_the_design_stress() {
        let m = NrelLfp250::blast_lite_2023();
        let cal = m.k_cal(T25_K, 0.5) * pow_pos(365.0, m.pcal);
        let cyc = m.k_cyc(T25_K, 0.8, 0.5) * pow_pos(280.0, m.pcyc);
        assert!((cal - 0.0110).abs() < 0.0002, "calendar {cal}");
        assert!((cyc - 0.0231).abs() < 0.0002, "cycle {cyc}");
    }

    #[test]
    fn calendar_loss_rises_with_temperature_and_state_of_charge() {
        let m = NrelLfp250::blast_lite_2023();
        assert!(m.k_cal(308.15, 0.5) > m.k_cal(T25_K, 0.5));
        assert!(m.k_cal(T25_K, 0.9) > m.k_cal(T25_K, 0.5));
        assert!(m.k_cal(T25_K, 0.5) > m.k_cal(T25_K, 0.2));
    }

    #[test]
    fn cycle_loss_rises_with_depth_and_rate() {
        let m = NrelLfp250::blast_lite_2023();
        assert!(m.k_cyc(T25_K, 1.0, 0.5) > m.k_cyc(T25_K, 0.5, 0.5));
        assert!(m.k_cyc(T25_K, 0.8, 1.0) > m.k_cyc(T25_K, 0.8, 0.5));
    }

    /// The anode potential falls as graphite lithiates, as the function
    /// is built to: 0.12 V around mid charge, higher when empty.
    #[test]
    fn the_anode_potential_falls_with_charge() {
        assert!(anode_potential_v(0.0) > anode_potential_v(0.5));
        assert!(anode_potential_v(0.5) > anode_potential_v(1.0));
        assert!((anode_potential_v(0.5) - 0.1233).abs() < 0.001);
    }
}
