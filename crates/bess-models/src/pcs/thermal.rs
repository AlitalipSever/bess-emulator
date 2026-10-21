//! The warm converter: one thermal node fed by conversion loss, a derating
//! factor from its temperature, and a bounded short-term overload budget
//! (M3 phase 3).
//!
//! The node is the converter's internal temperature, what a central
//! inverter publishes as its cabinet or heat-sink reading. It settles at
//! the air temperature plus a rise proportional to the loss, with a
//! published time constant; implicit Euler so the kernel stays rational.
//! The derating curve is the datasheet's permitted power against ambient,
//! shifted onto the node by the rise at rated power, so "full power to
//! 50 °C ambient" reads "full power to 50 °C plus the rated rise" on the
//! node. The overload budget is a reservoir in seconds at the permitted
//! overload: it spends while the AC power exceeds the rating and refills
//! below it, and while it holds the converter may run up to the overload
//! power.

/// Parameters of the node, the curve and the budget.
#[derive(Debug, Clone, PartialEq)]
pub struct ConverterThermal {
    /// Steady-state rise of the node over the air at rated loss, K.
    pub rise_at_rated_k: f64,
    /// Time constant of the node, s.
    pub tau_s: f64,
    /// Node temperature at which derating starts, °C: the datasheet's
    /// ambient knee plus the rated rise.
    pub derate_knee_c: f64,
    /// Node temperature at which the permitted power reaches zero, °C;
    /// linear in between.
    pub derate_zero_c: f64,
    /// Permitted overload above the rating, per unit (0.10 is 110 %).
    pub overload_pu: f64,
    /// Seconds the full budget lasts at the permitted overload.
    pub overload_s: f64,
    /// Seconds an empty budget takes to refill with the converter at or
    /// below its rating.
    pub refill_s: f64,
}

impl ConverterThermal {
    /// The Sungrow SG2500U, the unit behind the CEC record the surface
    /// uses. Public: operating range -30 to 60 °C with derating above
    /// 50 °C ambient, 2750 kVA at 45 °C against 2500 kVA at 50 °C (110 %),
    /// temperature-controlled forced-air cooling. Labeled estimates: the
    /// rise at rated loss (20 K), the time constant (15 min), the slope of
    /// the derating (to zero 20 K past the knee), the overload duration
    /// (10 min, the figure utility specifications commonly ask) and the
    /// refill (30 min). See CALIBRATION.md.
    pub fn sg2500u() -> Self {
        Self {
            rise_at_rated_k: 20.0,
            tau_s: 900.0,
            derate_knee_c: 70.0,
            derate_zero_c: 90.0,
            overload_pu: 0.10,
            overload_s: 600.0,
            refill_s: 1800.0,
        }
    }

    /// The node still warms, but the curve is flat at 1.0 and the budget
    /// never spends: the converter of M3 phase 2. For the switch-off proof.
    pub fn without_limits() -> Self {
        Self {
            derate_knee_c: f64::INFINITY,
            derate_zero_c: f64::INFINITY,
            overload_pu: 0.0,
            overload_s: f64::INFINITY,
            refill_s: f64::INFINITY,
            ..Self::sg2500u()
        }
    }

    /// Advance the node by `dt_s` toward the air plus the rise this loss
    /// earns, with `loss_rated_w` the loss at rated power.
    pub fn step_node(
        &self,
        node_c: f64,
        air_c: f64,
        loss_w: f64,
        loss_rated_w: f64,
        dt_s: f64,
    ) -> f64 {
        let target_c = air_c + self.rise_at_rated_k * (loss_w / loss_rated_w);
        node_c + (target_c - node_c) * dt_s / (self.tau_s + dt_s)
    }

    /// Permitted share of the rating at a node temperature, 0..=1.
    pub fn derating(&self, node_c: f64) -> f64 {
        if node_c <= self.derate_knee_c {
            1.0
        } else if node_c >= self.derate_zero_c {
            0.0
        } else {
            (self.derate_zero_c - node_c) / (self.derate_zero_c - self.derate_knee_c)
        }
    }

    /// Advance the budget (0..=1) by `dt_s` given the AC power of the last
    /// tick as a share of the rating: it spends in proportion to the
    /// excess over the rating, a full overload emptying it in
    /// `overload_s`, and refills at the rating or below.
    pub fn step_budget(&self, budget: f64, p_ac_pu: f64, dt_s: f64) -> f64 {
        let excess = p_ac_pu.abs() - 1.0;
        let next = if excess > 0.0 && self.overload_pu > 0.0 {
            budget - excess / self.overload_pu * dt_s / self.overload_s
        } else {
            budget + dt_s / self.refill_s
        };
        next.clamp(0.0, 1.0)
    }

    /// Whether the overload is armed after this tick's budget: a spent
    /// budget disarms it, and only a full one arms it again, so a converter
    /// that has used its allowance runs at its rating until the refill is
    /// complete rather than flickering in and out of overload at the
    /// edge of an empty budget.
    pub fn armed(&self, armed: bool, budget: f64) -> bool {
        if budget <= 0.0 {
            false
        } else if budget >= 1.0 {
            true
        } else {
            armed
        }
    }

    /// The AC power the converter permits this tick, W: the rating times
    /// the derating, plus the overload while it is armed.
    pub fn ac_limit_w(&self, rated_w: f64, node_c: f64, armed: bool) -> f64 {
        let overload = if armed { self.overload_pu } else { 0.0 };
        rated_w * self.derating(node_c) * (1.0 + overload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At constant loss the node settles at the air plus the rise, within
    /// a few time constants, and never overshoots.
    #[test]
    fn the_node_settles_at_the_air_plus_the_rise() {
        let t = ConverterThermal::sg2500u();
        let mut node = 20.0;
        let mut last = node;
        for _ in 0..(5 * 900) {
            node = t.step_node(node, 30.0, 75.0e3, 75.0e3, 1.0);
            assert!(node >= last - 1e-12);
            assert!(node <= 50.0 + 1e-9);
            last = node;
        }
        assert!((node - 50.0).abs() < 0.5, "{node}");
    }

    /// The curve is 1.0 to the knee, 0.5 halfway, 0.0 at the end.
    #[test]
    fn the_curve_reads_the_datasheet_knee() {
        let t = ConverterThermal::sg2500u();
        assert_eq!(t.derating(69.0), 1.0);
        assert_eq!(t.derating(70.0), 1.0);
        assert!((t.derating(80.0) - 0.5).abs() < 1e-12);
        assert_eq!(t.derating(90.0), 0.0);
        assert_eq!(t.derating(120.0), 0.0);
    }

    /// 110 % empties the budget in ten minutes, 105 % in twenty, and an
    /// empty budget refills in thirty at the rating; it never leaves
    /// 0..=1.
    #[test]
    fn the_budget_spends_and_refills_at_the_published_rates() {
        let t = ConverterThermal::sg2500u();
        let mut b = 1.0;
        let mut ticks = 0;
        while b > 0.0 {
            b = t.step_budget(b, 1.10, 1.0);
            ticks += 1;
        }
        assert_eq!(ticks, 600);
        let mut b = 1.0;
        let mut ticks = 0;
        while b > 0.0 {
            b = t.step_budget(b, -1.05, 1.0);
            ticks += 1;
        }
        assert_eq!(ticks, 1200);
        let mut b = 0.0;
        let mut ticks = 0;
        while b < 1.0 {
            b = t.step_budget(b, 1.0, 1.0);
            ticks += 1;
        }
        assert!((1799..=1801).contains(&ticks), "{ticks}");
        assert_eq!(t.step_budget(1.0, 0.5, 1.0), 1.0);
        assert_eq!(t.step_budget(0.0, 2.0, 1.0), 0.0);
    }

    /// The limit is the rating with the overload while it is armed, the
    /// rating alone once it is spent, and derated when hot; a spent budget
    /// disarms and only a full one re-arms.
    #[test]
    fn the_limit_combines_the_curve_and_the_budget() {
        let t = ConverterThermal::sg2500u();
        assert!((t.ac_limit_w(5.0e6, 40.0, true) - 5.5e6).abs() < 1.0);
        assert!((t.ac_limit_w(5.0e6, 40.0, false) - 5.0e6).abs() < 1.0);
        assert!((t.ac_limit_w(5.0e6, 80.0, false) - 2.5e6).abs() < 1.0);
        assert!(!t.armed(true, 0.0));
        assert!(!t.armed(false, 0.5));
        assert!(t.armed(true, 0.5));
        assert!(t.armed(false, 1.0));
        let off = ConverterThermal::without_limits();
        assert!((off.ac_limit_w(5.0e6, 200.0, true) - 5.0e6).abs() < 1.0);
        assert_eq!(off.step_budget(1.0, 3.0, 1.0), 1.0);
    }
}
