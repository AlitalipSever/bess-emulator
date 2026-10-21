//! The main transformer's two temperatures and its tap changer (M3
//! phase 4).
//!
//! The thermal model is IEC 60076-7's difference-equation form: a top-oil
//! rise over the ambient driven by the load factor through the oil
//! exponent, and a hot-spot rise over the top oil as the difference of two
//! first-order responses driven by the load factor through the winding
//! exponent. Both step by implicit Euler, and the fractional powers come
//! from `crate::math::pow_pos`, so the kernel stays byte-identical across
//! platforms. The tap changer is a position with a range and a step, moved
//! one step at a time by a regulator with a dead band and a delay.

use crate::math::pow_pos;

/// IEC 60076-7 thermal characteristics of the main transformer.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformerThermal {
    /// Top-oil rise over the ambient at rated load, K.
    pub top_oil_rise_rated_k: f64,
    /// Hot-spot rise over the top oil at rated load, K.
    pub hot_spot_gradient_rated_k: f64,
    /// Ratio of load losses at rated current to no-load losses.
    pub loss_ratio: f64,
    /// Oil exponent `x`.
    pub oil_exponent: f64,
    /// Winding exponent `y`.
    pub winding_exponent: f64,
    /// Constant `k11` on the oil time constant.
    pub k11: f64,
    /// Constant `k21`, the hot-spot overshoot factor.
    pub k21: f64,
    /// Constant `k22` on the winding time constant.
    pub k22: f64,
    /// Oil time constant, s.
    pub tau_oil_s: f64,
    /// Winding time constant, s.
    pub tau_winding_s: f64,
}

impl TransformerThermal {
    /// GW-01's 100 MVA ONAF unit. The rated rises are the IEC 60076-2
    /// limits for mineral-oil units (top oil 60 K, hot spot 78 K over
    /// the ambient, so an 18 K gradient), public. The exponents and
    /// constants are the values IEC 60076-7's table of recommended
    /// thermal characteristics gives for the ONAF column as reproduced
    /// across the open literature (x 0.8, y 1.3, k11 0.5, k21 2.0, k22
    /// 2.0, oil 150 min, winding 7 min); the table itself is not
    /// reproduced here and the figures are labeled as such in
    /// CALIBRATION.md. The loss ratio is the M0 constants' (600 kW over
    /// 100 kW).
    pub fn onaf_100_mva() -> Self {
        Self {
            top_oil_rise_rated_k: 60.0,
            hot_spot_gradient_rated_k: 18.0,
            loss_ratio: 6.0,
            oil_exponent: 0.8,
            winding_exponent: 1.3,
            k11: 0.5,
            k21: 2.0,
            k22: 2.0,
            tau_oil_s: 150.0 * 60.0,
            tau_winding_s: 7.0 * 60.0,
        }
    }

    /// Steady-state top-oil rise over the ambient at load factor `k`, K.
    pub fn top_oil_rise_k(&self, k: f64) -> f64 {
        let r = self.loss_ratio;
        self.top_oil_rise_rated_k * pow_pos((1.0 + r * k * k) / (1.0 + r), self.oil_exponent)
    }

    /// Steady-state hot-spot rise over the top oil at load factor `k`, K.
    pub fn hot_spot_gradient_k(&self, k: f64) -> f64 {
        self.hot_spot_gradient_rated_k * pow_pos(k, self.winding_exponent)
    }

    /// Advance the three state variables by `dt_s`: the top-oil
    /// temperature toward the ambient plus its steady rise (an energized
    /// transformer; `energized` false cools it toward the ambient with no
    /// rise), and the hot spot's two responses toward their shares of the
    /// steady gradient.
    pub fn step(
        &self,
        state: &mut TransformerState,
        ambient_c: f64,
        k: f64,
        energized: bool,
        dt_s: f64,
    ) {
        let rise = if energized {
            self.top_oil_rise_k(k)
        } else {
            0.0
        };
        let gradient = if energized {
            self.hot_spot_gradient_k(k)
        } else {
            0.0
        };
        let oil_target = ambient_c + rise;
        let tau_o = self.k11 * self.tau_oil_s;
        state.top_oil_c += (oil_target - state.top_oil_c) * dt_s / (tau_o + dt_s);
        let tau_1 = self.k22 * self.tau_winding_s;
        let tau_2 = self.tau_oil_s / self.k22;
        let target_1 = self.k21 * gradient;
        let target_2 = (self.k21 - 1.0) * gradient;
        state.hot_spot_rise_1_k += (target_1 - state.hot_spot_rise_1_k) * dt_s / (tau_1 + dt_s);
        state.hot_spot_rise_2_k += (target_2 - state.hot_spot_rise_2_k) * dt_s / (tau_2 + dt_s);
    }
}

/// The transformer's thermal state, as the substation carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformerState {
    /// Top-oil temperature, °C.
    pub top_oil_c: f64,
    /// The fast share of the hot-spot rise over the top oil, K.
    pub hot_spot_rise_1_k: f64,
    /// The slow share, subtracted, K.
    pub hot_spot_rise_2_k: f64,
}

impl TransformerState {
    /// Hot-spot temperature, °C: the top oil plus the gradient, which the
    /// two responses never let fall below the top oil at rest.
    pub fn hot_spot_c(&self) -> f64 {
        (self.top_oil_c + self.hot_spot_rise_1_k - self.hot_spot_rise_2_k).max(self.top_oil_c)
    }
}

/// The on-load tap changer and its voltage regulator.
#[derive(Debug, Clone, PartialEq)]
pub struct TapChanger {
    /// Positions each side of neutral.
    pub steps: i8,
    /// Voltage change per step, per unit of nominal.
    pub step_pu: f64,
    /// Half-width of the regulator's dead band around nominal, per unit.
    pub deadband_pu: f64,
    /// Seconds the MV bus has to sit outside the band before a step.
    pub delay_s: f64,
}

impl Default for TapChanger {
    /// ±8 steps of 1.25 % (17 positions, ±10 %), the common range on an
    /// HV/MV power transformer's OLTC; a dead band of 1.5 % (a little
    /// over one step, so the regulator cannot hunt) and a 60 s delay.
    /// Labeled estimates; see CALIBRATION.md.
    fn default() -> Self {
        Self {
            steps: 8,
            step_pu: 0.0125,
            deadband_pu: 0.015,
            delay_s: 60.0,
        }
    }
}

impl TapChanger {
    /// The ratio the tap sets on the MV side, per unit.
    pub fn ratio(&self, position: i8) -> f64 {
        1.0 + f64::from(position) * self.step_pu
    }

    /// Run the regulator one tick on the MV bus voltage it sees (per
    /// unit): count time outside the dead band and step once, toward
    /// nominal, when the delay is up; reset the count inside the band or
    /// at the end of the range. Returns the position after the tick.
    pub fn regulate(&self, position: i8, timer_s: &mut f64, mv_pu: f64, dt_s: f64) -> i8 {
        let error = mv_pu - 1.0;
        if error.abs() <= self.deadband_pu {
            *timer_s = 0.0;
            return position;
        }
        let direction: i8 = if error > 0.0 { -1 } else { 1 };
        let next = position + direction;
        if next.abs() > self.steps {
            *timer_s = 0.0;
            return position;
        }
        *timer_s += dt_s;
        if *timer_s >= self.delay_s {
            *timer_s = 0.0;
            next
        } else {
            position
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(ambient_c: f64) -> TransformerState {
        TransformerState {
            top_oil_c: ambient_c,
            hot_spot_rise_1_k: 0.0,
            hot_spot_rise_2_k: 0.0,
        }
    }

    /// At rated load for a day the top oil settles 60 K and the hot spot
    /// 78 K over the ambient, the standard's rated rises; at no load the
    /// top oil keeps the core's share and the gradient is zero.
    #[test]
    fn the_rated_rises_are_the_steady_state() {
        let t = TransformerThermal::onaf_100_mva();
        let mut s = fresh(20.0);
        for _ in 0..86_400 {
            t.step(&mut s, 20.0, 1.0, true, 1.0);
        }
        assert!((s.top_oil_c - 80.0).abs() < 0.05, "{}", s.top_oil_c);
        assert!((s.hot_spot_c() - 98.0).abs() < 0.05, "{}", s.hot_spot_c());
        let mut s = fresh(20.0);
        for _ in 0..86_400 {
            t.step(&mut s, 20.0, 0.0, true, 1.0);
        }
        assert!((s.top_oil_c - 20.0 - t.top_oil_rise_k(0.0)).abs() < 0.05);
        assert!((s.hot_spot_c() - s.top_oil_c).abs() < 1.0e-9);
        let mut s = fresh(60.0);
        for _ in 0..86_400 {
            t.step(&mut s, 20.0, 0.0, false, 1.0);
        }
        assert!(
            (s.top_oil_c - 20.0).abs() < 0.05,
            "de-energized, it cools to the ambient"
        );
    }

    /// A step to rated load: the hot spot overshoots its steady gradient
    /// within the winding's minutes (the `k21` response) and never sits
    /// below the top oil; the top oil rises monotonically.
    #[test]
    fn a_load_step_overshoots_the_hot_spot_and_never_the_top_oil() {
        let t = TransformerThermal::onaf_100_mva();
        let mut s = fresh(20.0);
        let mut last_oil = s.top_oil_c;
        let mut max_gradient: f64 = 0.0;
        for _ in 0..7200 {
            t.step(&mut s, 20.0, 1.0, true, 1.0);
            assert!(s.top_oil_c >= last_oil - 1e-12);
            assert!(s.hot_spot_c() >= s.top_oil_c);
            max_gradient = max_gradient.max(s.hot_spot_c() - s.top_oil_c);
            last_oil = s.top_oil_c;
        }
        assert!(max_gradient > 18.0 * 1.3, "no overshoot: {max_gradient}");
        assert!(max_gradient < 18.0 * 2.0);
    }

    /// The tap stays in its range, steps once per delay, in the direction
    /// that brings the bus back, and holds inside the dead band.
    #[test]
    fn the_tap_steps_once_per_delay_and_stays_in_range() {
        let tap = TapChanger::default();
        let mut pos = 0i8;
        let mut timer = 0.0;
        let mut steps = Vec::new();
        for tick in 1..=600u64 {
            let before = pos;
            pos = tap.regulate(pos, &mut timer, 1.04, 1.0);
            if pos != before {
                steps.push(tick);
            }
        }
        assert_eq!(steps, vec![60, 120, 180, 240, 300, 360, 420, 480]);
        assert_eq!(pos, -8);
        for _ in 0..600 {
            pos = tap.regulate(pos, &mut timer, 1.04, 1.0);
        }
        assert_eq!(pos, -8, "left its range");
        assert_eq!(
            tap.regulate(0, &mut timer, 1.01, 1.0),
            0,
            "stepped inside the band"
        );
        let mut pos = 0i8;
        for _ in 0..60 {
            pos = tap.regulate(pos, &mut timer, 0.97, 1.0);
        }
        assert_eq!(pos, 1);
        assert!((tap.ratio(8) - 1.1).abs() < 1e-12);
    }
}
