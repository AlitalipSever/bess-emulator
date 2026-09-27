//! Temperature derating from the cell maker's own tables.
//!
//! This file makes claims about a datasheet and nothing else: the points
//! below are EVE MB31 (314 Ah LFP) specification PBRI-MB31-D06-01 rev A,
//! Table 5 (continuous charging) and Table 7 (continuous discharging), in
//! P-rate at 0 to 100% SoC. Tests hold them verbatim. How the BMS combines
//! the resulting factor with the SoC window is `BasicBms`'s business.

/// A piecewise-linear curve of permitted power against cell temperature,
/// (degrees Celsius, P-rate). Zero outside the listed range; linear between
/// listed points, since the datasheet gives points and not a step rule.
#[derive(Debug, Clone, PartialEq)]
pub struct TempCurve {
    /// Strictly increasing in temperature.
    pub points: Vec<(f64, f64)>,
}

impl TempCurve {
    /// Permitted rate at `temp_c` as a fraction of the curve's own peak, in
    /// [0, 1]. The table's absolute level (0.5P for this cell) is the cell's
    /// continuous rating; the rack's rating is a separate site parameter, so
    /// only the shape is carried over.
    pub fn factor(&self, temp_c: f64) -> f64 {
        let peak = self.points.iter().map(|p| p.1).fold(0.0_f64, f64::max);
        if peak <= 0.0 {
            return 0.0;
        }
        (self.rate_at(temp_c) / peak).clamp(0.0, 1.0)
    }

    /// Permitted rate at `temp_c`, in the table's unit.
    pub fn rate_at(&self, temp_c: f64) -> f64 {
        let (first, last) = match (self.points.first(), self.points.last()) {
            (Some(f), Some(l)) => (*f, *l),
            _ => return 0.0,
        };
        if !(first.0..=last.0).contains(&temp_c) {
            return 0.0;
        }
        for w in self.points.windows(2) {
            let ((t0, r0), (t1, r1)) = (w[0], w[1]);
            if temp_c <= t1 {
                return r0 + (r1 - r0) * (temp_c - t0) / (t1 - t0);
            }
        }
        last.1
    }
}

/// EVE MB31 Table 5, maximum continuous charging power (P) by cell
/// temperature. Charging below 0 C is prohibited by the same document, so
/// the curve starts there; it ends at the 60 C absolute charging limit.
pub const EVE_MB31_CHARGE_P: &[(f64, f64)] = &[
    (0.0, 0.05),
    (5.0, 0.12),
    (10.0, 0.3),
    (15.0, 0.5),
    (20.0, 0.5),
    (25.0, 0.5),
    (45.0, 0.5),
    (50.0, 0.5),
    (55.0, 0.5),
    (60.0, 0.0),
];

/// EVE MB31 Table 7, maximum continuous discharging power (P) by cell
/// temperature. The table skips 5 to 45 C; it reads 0.5P on both sides and
/// linear interpolation carries that across.
pub const EVE_MB31_DISCHARGE_P: &[(f64, f64)] = &[
    (-30.0, 0.0),
    (-20.0, 0.5),
    (-10.0, 0.5),
    (-5.0, 0.5),
    (0.0, 0.5),
    (5.0, 0.5),
    (45.0, 0.5),
    (50.0, 0.5),
    (55.0, 0.5),
    (60.0, 0.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn charge() -> TempCurve {
        TempCurve {
            points: EVE_MB31_CHARGE_P.to_vec(),
        }
    }

    fn discharge() -> TempCurve {
        TempCurve {
            points: EVE_MB31_DISCHARGE_P.to_vec(),
        }
    }

    #[test]
    fn listed_points_are_reproduced_exactly() {
        for curve in [charge(), discharge()] {
            for &(t, r) in &curve.points {
                assert!((curve.rate_at(t) - r).abs() < 1.0e-12, "{t} C");
            }
        }
    }

    #[test]
    fn outside_the_table_nothing_is_permitted() {
        assert!(charge().factor(-0.1).abs() < f64::EPSILON);
        assert!(charge().factor(60.1).abs() < f64::EPSILON);
        assert!(discharge().factor(-30.1).abs() < f64::EPSILON);
        assert!(discharge().factor(60.1).abs() < f64::EPSILON);
    }

    #[test]
    fn between_points_the_curve_is_linear() {
        // Halfway from 5 C (0.12P) to 10 C (0.3P).
        assert!((charge().rate_at(7.5) - 0.21).abs() < 1.0e-12);
        // Halfway down the hot shoulder.
        assert!((discharge().rate_at(57.5) - 0.25).abs() < 1.0e-12);
    }

    /// Every 0.01 K from -40 C to 70 C: factors stay in [0, 1], and walking
    /// away from the comfort band (15 to 45 C, full rate in both tables)
    /// in either direction never raises one.
    #[test]
    fn factors_are_bounded_and_monotonic_away_from_comfort() {
        for curve in [charge(), discharge()] {
            let mut last = curve.factor(15.0);
            for step in 1..=5500 {
                let f = curve.factor(15.0 - f64::from(step) * 0.01);
                assert!((0.0..=1.0).contains(&f));
                assert!(f <= last + 1.0e-12, "rose walking cold at step {step}");
                last = f;
            }
            let mut last = curve.factor(45.0);
            for step in 1..=2500 {
                let f = curve.factor(45.0 + f64::from(step) * 0.01);
                assert!((0.0..=1.0).contains(&f));
                assert!(f <= last + 1.0e-12, "rose walking hot at step {step}");
                last = f;
            }
        }
    }

    #[test]
    fn at_the_cold_charge_limit_discharge_is_still_permitted() {
        assert!(charge().factor(-0.01).abs() < f64::EPSILON);
        assert!((discharge().factor(-0.01) - 1.0).abs() < f64::EPSILON);
    }
}
