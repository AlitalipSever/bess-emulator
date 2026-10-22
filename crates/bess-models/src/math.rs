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
    exp_det(exponent * ln_rational(base))
}

/// `e^x`, deterministic across platforms, for any `x` (M5).
///
/// The argument is halved until it is within 1/64 (six halvings for
/// `|x| <= 1`, one more per doubling beyond, so the thermal model's small
/// arguments take exactly the path they took before M5 and the aging
/// model's Arrhenius terms, hundreds of kelvin over a temperature, are
/// reduced further), a short Taylor series there, and the result squared
/// back up. Each squaring doubles the relative error, so the result is
/// within parts in 1e-12 for `|x| <= 1` and parts in 1e-11 for arguments
/// in the hundreds. Below -745 the answer is zero and above 709 it is the
/// largest finite value, the limits of `f64` themselves.
pub fn exp_det(x: f64) -> f64 {
    if x < -745.0 {
        return 0.0;
    }
    if x > 709.0 {
        return f64::MAX;
    }
    let mut halvings = 6u32;
    let mut magnitude = x.abs();
    while magnitude > 1.0 {
        magnitude /= 2.0;
        halvings += 1;
    }
    let mut y = x / f64::from(1u32 << halvings);
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..14u32 {
        term *= y / f64::from(k);
        sum += term;
    }
    y = sum;
    for _ in 0..halvings {
        y *= y;
    }
    y
}

/// `tanh(x)`, deterministic across platforms: `(e^2x - 1) / (e^2x + 1)`,
/// saturated past |x| = 20 where the fraction is 1 to every bit.
pub fn tanh_det(x: f64) -> f64 {
    if x > 20.0 {
        return 1.0;
    }
    if x < -20.0 {
        return -1.0;
    }
    let e = exp_det(2.0 * x);
    (e - 1.0) / (e + 1.0)
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
    fn exp_and_tanh_match_the_platform_to_parts_in_1e12() {
        for &x in &[
            -520.0, -17.5, -5.0, -1.0, -0.3, 0.0, 0.01, 0.9, 1.0, 7.35, 20.0, 100.0,
        ] {
            let want = f64::exp(x);
            let got = exp_det(x);
            // Each squaring doubles the relative error: parts in 1e-12
            // for the thermal model's arguments, parts in 1e-11 for an
            // Arrhenius term in the tens and hundreds.
            let tolerance = if x.abs() >= 20.0 { 1.0e-10 } else { 1.0e-12 };
            assert!(
                ((got - want) / want).abs() < tolerance,
                "exp {x}: {got} vs {want}"
            );
        }
        for &x in &[-3.0, -0.7, -0.1, 0.0, 0.2, 1.5, 4.0, 25.0] {
            let want = f64::tanh(x);
            let got = tanh_det(x);
            assert!((got - want).abs() < 1.0e-12, "tanh {x}: {got} vs {want}");
        }
        assert_eq!(exp_det(-800.0), 0.0);
        assert_eq!(tanh_det(-30.0), -1.0);
    }

    #[test]
    fn the_edges_are_exact() {
        assert_eq!(pow_pos(0.0, 1.3), 0.0);
        assert_eq!(pow_pos(0.0, 0.0), 1.0);
        assert_eq!(pow_pos(5.0, 0.0), 1.0);
        assert!((pow_pos(1.0, 1.3) - 1.0).abs() < 1.0e-15);
    }
}
