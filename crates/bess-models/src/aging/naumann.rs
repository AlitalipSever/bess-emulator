//! Naumann et al.'s resistance-growth laws for an LFP/graphite cell.
//!
//! The NREL capacity model publishes no resistance law, so the growth
//! comes from the calendar and cycle aging studies of a Sony/Murata 26650
//! LFP cell (Naumann et al., J. Energy Storage 17 (2018) 153; J. Power
//! Sources 451 (2020) 227666), with the constants as SimSES (TUM, BSD
//! 3-Clause) carries them. Labeled in CALIBRATION.md as a different cell's
//! law: a high-power 3 Ah cylinder, not a 314 Ah prismatic. Calendar
//! growth is linear in time with an Arrhenius term and a state-of-charge
//! term; cycle growth is linear in throughput with depth and rate terms.

use crate::math::exp_det;

/// The gas constant, J/(mol K).
const R_GAS: f64 = 8.314_459_8;

/// The reference temperature of the calendar law, kelvin.
const T_REF_K: f64 = 298.15;

/// The eight constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NaumannResistance {
    /// Calendar: the reference rate, per second.
    pub k_ref_cal: f64,
    /// Calendar: the activation energy, J/mol.
    pub ea_cal: f64,
    /// Calendar: the quadratic state-of-charge term.
    pub c_cal: f64,
    /// Calendar: the state-of-charge offset.
    pub d_cal: f64,
    /// Cycle: the rate term, percent per cycle per C.
    pub a_cyc: f64,
    /// Cycle: the rate offset, percent per cycle.
    pub b_cyc: f64,
    /// Cycle: the cubic depth term.
    pub c_cyc: f64,
    /// Cycle: the depth offset.
    pub d_cyc: f64,
}

impl NaumannResistance {
    /// The published constants.
    pub const fn simses() -> Self {
        Self {
            k_ref_cal: 3.4194e-10,
            ea_cal: 71_827.0,
            c_cal: -3.3903,
            d_cal: 1.5604,
            a_cyc: -0.0020,
            b_cyc: 0.0021,
            c_cyc: 6.8477,
            d_cyc: 0.918_82,
        }
    }

    /// Calendar growth over `seconds` at a mean temperature and state of
    /// charge, fraction.
    pub fn calendar_gain(&self, temp_k: f64, soc: f64, seconds: f64) -> f64 {
        let k_temp =
            self.k_ref_cal * exp_det(-self.ea_cal / R_GAS * (1.0 / temp_k - 1.0 / T_REF_K));
        let k_soc = self.c_cal * (soc - 0.5) * (soc - 0.5) + self.d_cal;
        k_temp * k_soc * seconds
    }

    /// Cycle growth over `delta_efc` equivalent full cycles at a mean
    /// rate and depth, fraction.
    pub fn cycle_gain(&self, c_rate: f64, dod: f64, delta_efc: f64) -> f64 {
        let k_rate = self.a_cyc * c_rate + self.b_cyc;
        let d = dod - 0.5;
        let k_dod = self.c_cyc * d * d * d + self.d_cyc;
        k_rate * k_dod * delta_efc / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The design's figures (D2): about 1.7 % a year at rest at 25 °C and
    /// 50 %, about 0.36 % per 300 cycles at 0.8 depth and 0.5C.
    #[test]
    fn the_design_figures() {
        let m = NaumannResistance::simses();
        let cal = m.calendar_gain(298.15, 0.5, 365.0 * 86_400.0);
        assert!((cal - 0.0168).abs() < 0.0002, "{cal}");
        let cyc = m.cycle_gain(0.5, 0.8, 300.0);
        assert!((cyc - 0.00364).abs() < 0.00005, "{cyc}");
    }

    #[test]
    fn warmth_and_mid_charge_grow_resistance_fastest() {
        let m = NaumannResistance::simses();
        assert!(m.calendar_gain(308.15, 0.5, 1.0) > m.calendar_gain(298.15, 0.5, 1.0));
        assert!(m.calendar_gain(298.15, 0.5, 1.0) > m.calendar_gain(298.15, 0.9, 1.0));
    }
}
