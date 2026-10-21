//! The two-dimensional efficiency surface: the Sandia inverter performance
//! model with its DC-voltage terms, as the CEC inverter database tabulates
//! it (M3 phase 1).
//!
//! Per unit of the AC rating, with `d = V_dc - V_dco`:
//!
//! ```text
//! A = Pdco (1 + C1 d)      the DC power at which rated AC power is reached
//! B = Pso  (1 + C2 d)      the DC power that covers self-consumption
//! C = C0   (1 + C3 d)      the curvature
//! P_ac = (1 / (A - B) - C (A - B)) (P_dc - B) + C (P_dc - B)^2
//! ```
//!
//! M0.5 fitted a loss polynomial to this surface at `V_dco` and dropped the
//! voltage terms; this module keeps the record's own form, so the second
//! axis is the database's rather than a fit of ours. The inverse (DC for a
//! wanted AC) is the quadratic's closed form; nothing here is iterative or
//! transcendental, so the kernel's byte-identity across platforms holds.

/// The surface's coefficients, per unit of the AC rating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SandiaSurface {
    /// `Pdco / Paco`.
    pub pdco_pu: f64,
    /// `Pso / Paco`.
    pub pso_pu: f64,
    /// `C0 x Paco`, dimensionless curvature.
    pub c0_pu: f64,
    /// `C1`, 1/V.
    pub c1_per_v: f64,
    /// `C2`, 1/V.
    pub c2_per_v: f64,
    /// `C3`, 1/V.
    pub c3_per_v: f64,
    /// Reference DC voltage, V.
    pub vdco_v: f64,
}

/// The CEC inverter database record of the Sungrow SC2500UD-US, as
/// distributed with NREL SAM (`CEC Inverters.csv`, retrieved 2026-10-21):
/// Paco 2,507,230 W, Pdco 2,563,700 W, Pso 6,066.41 W, Vdco 1,350 V,
/// C0 -8.03897e-9 1/W, C1 3.22933e-6, C2 4.85948e-4, C3 4.84988e-5 1/V,
/// MPPT window 1,300 to 1,500 V. Scaled per unit of Paco, so the surface
/// applies to GW-01's 5 MW blocks as the M0.5 curve did.
pub const CEC_SC2500UD_US: SandiaSurface = SandiaSurface {
    pdco_pu: 2.563_7e6 / 2_507_230.0,
    pso_pu: 6_066.41 / 2_507_230.0,
    c0_pu: -8.038_97e-9 * 2_507_230.0,
    c1_per_v: 3.229_33e-6,
    c2_per_v: 4.859_48e-4,
    c3_per_v: 4.849_88e-5,
    vdco_v: 1_350.0,
};

impl SandiaSurface {
    /// The three voltage-dependent terms at a DC voltage: the DC power at
    /// which rated AC is reached, the self-consumption, the curvature.
    fn terms(&self, vdc_v: f64) -> (f64, f64, f64) {
        let d = vdc_v - self.vdco_v;
        (
            self.pdco_pu * (1.0 + self.c1_per_v * d),
            self.pso_pu * (1.0 + self.c2_per_v * d),
            self.c0_pu * (1.0 + self.c3_per_v * d),
        )
    }

    /// AC output per unit for a DC input per unit at a DC voltage; zero
    /// when the input cannot cover self-consumption.
    pub fn ac_pu(&self, dc_pu: f64, vdc_v: f64) -> f64 {
        let (full, idle, curve) = self.terms(vdc_v);
        let above = dc_pu - idle;
        if above <= 0.0 {
            return 0.0;
        }
        let slope = 1.0 / (full - idle) - curve * (full - idle);
        (slope * above + curve * above * above).max(0.0)
    }

    /// DC input per unit needed for an AC output per unit at a DC voltage:
    /// the quadratic's physical root, in the form that does not cancel.
    pub fn dc_pu(&self, ac_pu: f64, vdc_v: f64) -> f64 {
        let (full, idle, curve) = self.terms(vdc_v);
        if ac_pu <= 0.0 {
            return idle;
        }
        let slope = 1.0 / (full - idle) - curve * (full - idle);
        let disc = (slope * slope + 4.0 * curve * ac_pu).max(0.0);
        idle + 2.0 * ac_pu / (slope + disc.sqrt())
    }

    /// One-way efficiency at an AC output per unit and a DC voltage.
    pub fn efficiency(&self, ac_pu: f64, vdc_v: f64) -> f64 {
        if ac_pu <= 0.0 {
            return 0.0;
        }
        ac_pu / self.dc_pu(ac_pu, vdc_v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The M0.5 reference points, computed from this record at `Vdco`;
    /// the surface has to reproduce them, since they are its own.
    #[test]
    fn at_the_reference_voltage_the_surface_is_the_m0_5_reference() {
        let s = CEC_SC2500UD_US;
        let reference = [
            (0.05, 0.95370),
            (0.10, 0.97527),
            (0.20, 0.98493),
            (0.30, 0.98686),
            (0.50, 0.98595),
            (0.75, 0.98239),
            (1.00, 0.97797),
        ];
        for (frac, eta_ref) in reference {
            let eta = s.efficiency(frac, s.vdco_v);
            assert!(
                (eta - eta_ref).abs() < 1.0e-4,
                "at {:.0} %: {eta:.5} vs {eta_ref:.5}",
                frac * 100.0
            );
        }
    }

    #[test]
    fn forward_and_inverse_agree_at_every_voltage() {
        let s = CEC_SC2500UD_US;
        for vdc in [1_206.0, 1_300.0, 1_350.0, 1_435.0, 1_500.0] {
            for ac in [0.01, 0.05, 0.3, 0.6, 1.0] {
                let dc = s.dc_pu(ac, vdc);
                let back = s.ac_pu(dc, vdc);
                assert!((back - ac).abs() < 1.0e-9, "{vdc} V, {ac} pu: {back}");
            }
        }
    }

    /// The voltage terms move the curve by a bounded amount across the
    /// rack's window: a sanity band, since the record's own coefficients
    /// are the source and there is nothing to fit.
    #[test]
    fn across_the_rack_window_the_surface_moves_little() {
        let s = CEC_SC2500UD_US;
        for ac in [0.05, 0.3, 1.0] {
            let at_ref = s.efficiency(ac, s.vdco_v);
            for vdc in [1_206.0, 1_435.0] {
                let eta = s.efficiency(ac, vdc);
                assert!(
                    (eta - at_ref).abs() < 0.01,
                    "{vdc} V at {ac} pu: {eta} vs {at_ref}"
                );
            }
        }
        // And it does move: the terms are not zero.
        assert!((s.efficiency(1.0, 1_206.0) - s.efficiency(1.0, 1_435.0)).abs() > 1.0e-6);
    }

    #[test]
    fn below_self_consumption_nothing_comes_out() {
        let s = CEC_SC2500UD_US;
        assert!(s.ac_pu(0.001, s.vdco_v).abs() < f64::EPSILON);
        assert!(s.efficiency(0.0, s.vdco_v).abs() < f64::EPSILON);
    }
}
