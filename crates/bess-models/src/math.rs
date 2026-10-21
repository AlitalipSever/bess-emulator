//! Arithmetic the kernel can carry across platforms.
//!
//! The determinism contract forbids transcendental calls in the physics
//! (`exp`, `ln`, `pow`), because macOS's libm and glibc round them
//! differently and a library record taken on one drifts on the other
//! (CALIBRATION.md, the hysteresis lesson). A model that needs a
//! fractional power, as the IEC 60076-7 thermal equations do, gets it
//! from here: a power built from additions, multiplications, divisions
//! and square roots only, which IEEE 754 pins bit for bit on every
//! platform. Accuracy is to a few parts in 1e-13, far under anything a
//! thermal model resolves; the tests hold it against `f64::powf`.

/// `base.powf(exponent)` for `base >= 0`, deterministic across platforms.
///
/// `ln` by five square roots (which bring any base in [1e-9, 1e6] to
/// within 35 % of 1) and the `atanh` series, `exp` by halving the
/// argument until it is small and a short Taylor series squared back up.
/// A zero base returns zero for a positive exponent and one for zero.
pub fn pow_pos(base: f64, exponent: f64) -> f64 {
    debug_assert!(base >= 0.0, "pow_pos takes a non-negative base");
    if exponent == 0.0 {
        return 1.0;
    }
    if base <= 0.0 {
        return 0.0;
    }
    exp_rational(exponent * ln_rational(base))
}

/// Natural logarithm of a positive number by square roots and the
/// `atanh` series: `ln(b) = 32 * ln(b^(1/32))`, and for `a` near 1,
/// `ln(a) = 2 * (z + z^3/3 + z^5/5 + ...)` with `z = (a - 1) / (a + 1)`.
fn ln_rational(base: f64) -> f64 {
    let mut a = base;
    for _ in 0..5 {
        a = a.sqrt();
    }
    let z = (a - 1.0) / (a + 1.0);
    let z2 = z * z;
    let mut term = z;
    let mut sum = 0.0;
    for k in 0..24u32 {
        sum += term / f64::from(2 * k + 1);
        term *= z2;
    }
    32.0 * 2.0 * sum
}

/// `e^x` by halving the argument into [-1/64, 1/64], a Taylor series
/// there, and squaring the result six times.
fn exp_rational(x: f64) -> f64 {
    let mut y = x / 64.0;
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..14u32 {
        term *= y / f64::from(k);
        sum += term;
    }
    y = sum;
    for _ in 0..6 {
        y *= y;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the platform's own `powf`, over the bases and exponents
    /// the thermal model uses and well beyond.
    #[test]
    fn pow_pos_matches_powf_to_parts_in_1e12() {
        let bases: [f64; 12] = [
            1.0e-6, 0.01, 0.1, 0.3, 0.5, 0.8, 1.0, 1.1, 1.3, 2.0, 7.0, 100.0,
        ];
        let exponents: [f64; 7] = [0.3, 0.8, 1.0, 1.3, 1.6, 2.0, -0.5];
        for &b in &bases {
            for &e in &exponents {
                let want = b.powf(e);
                let got = pow_pos(b, e);
                let rel = ((got - want) / want).abs();
                assert!(
                    rel < 1.0e-12,
                    "{b}^{e}: got {got}, want {want}, rel {rel:e}"
                );
            }
        }
    }

    #[test]
    fn the_edges_are_exact() {
        assert_eq!(pow_pos(0.0, 1.3), 0.0);
        assert_eq!(pow_pos(0.0, 0.0), 1.0);
        assert_eq!(pow_pos(5.0, 0.0), 1.0);
        assert!((pow_pos(1.0, 1.3) - 1.0).abs() < 1.0e-15);
    }
}
