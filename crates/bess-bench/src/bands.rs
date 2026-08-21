//! The gate bands, and where each one comes from.
//!
//! A band with no source is a preference wearing a lab coat, so every entry
//! here says what kind of bound it is and CALIBRATION.md publishes the
//! citation beside the measurement. Bands are drawn before the measurement is
//! taken and never widened to admit it: a reading outside its band is a
//! finding, and the response is to explain it or to re-source the band in the
//! open, never to tune the model until the build goes green.

use crate::run::Kpis;

/// A closed interval a measurement has to land in.
#[derive(Debug, Clone, Copy)]
pub struct Band {
    /// Lower bound, inclusive.
    pub lo: f64,
    /// Upper bound, inclusive.
    pub hi: f64,
}

impl Band {
    /// Whether a measurement lands inside.
    pub fn contains(self, value: f64) -> bool {
        value >= self.lo && value <= self.hi
    }
}

/// What a bound is entitled to claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// Drawn from published data: landing inside is evidence about realism.
    Sourced,
    /// Drawn wide around physical plausibility because no public dataset
    /// publishes the quantity at this scale. Landing inside is evidence that
    /// nothing broke, and nothing more than that.
    Sanity,
}

impl Bound {
    /// How the generated document labels it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Sourced => "sourced",
            Self::Sanity => "sanity bound",
        }
    }
}

/// One gated reading.
pub struct Gate {
    /// What is measured.
    pub name: &'static str,
    /// The interval it has to land in.
    pub band: Band,
    /// What that interval is entitled to claim.
    pub bound: Bound,
    /// Where the interval comes from, as the document prints it.
    pub source: &'static str,
    /// Pull the measurement out of a run.
    pub read: fn(&Kpis) -> f64,
}

/// Annual round-trip efficiency at the point of interconnection.
///
/// The floor is the U.S. utility-scale battery fleet's measured average
/// monthly round-trip efficiency, 82% in 2019, computed by the EIA from
/// plant-level consumption and generation reported on Form EIA-923. The
/// ceiling is the NREL Annual Technology Baseline's design assumption for
/// utility-scale storage, 85% in the 2024 edition, down from 86% in 2022. A
/// plant that beats the ceiling is claiming to be better than the reference
/// design; one that falls below the floor is claiming to be worse than a
/// fleet of mixed vintages, chemistries and duty cycles. Both are claims this
/// model has not earned, which is what makes the interval a gate.
pub const ANNUAL_RTE: Band = Band { lo: 0.80, hi: 0.85 };

/// Auxiliary energy as a share of energy imported at the POI.
///
/// Deliberately wide, and a sanity bound rather than a calibration band. No
/// public dataset publishes auxiliary share separately for a plant of this
/// size, and the quantity is dominated by utilization: the same hardware
/// reads a few percent at 300 cycles a year and a large multiple of that on a
/// plant that mostly sits still, which is the central finding of Schimpe et
/// al. 2018. The realism claim therefore rests on the round-trip gate above
/// and on the item-by-item waterfall, not on this number; regression
/// detection rests on the committed record, which holds every figure to a
/// tenth of a percent. This bound exists to catch a break gross enough to
/// survive a careless record regeneration.
pub const AUX_SHARE_OF_IMPORT: Band = Band { lo: 0.01, hi: 0.08 };

/// Unexplained energy as a share of throughput. Not a calibration band: the
/// books either close or the accounting is wrong. The tolerance matches the
/// daily invariant test and exists for float accumulation over tens of
/// millions of ticks, not for unmodeled physics.
pub const BALANCE_RESIDUAL_MAX: f64 = 2.0e-3;

/// The gates CI enforces.
pub const GATES: &[Gate] = &[
    Gate {
        name: "Annual round-trip efficiency at the POI",
        band: ANNUAL_RTE,
        bound: Bound::Sourced,
        source: "EIA-923 fleet average 82% (2019); NREL ATB 2024 design assumption 85%",
        read: |k| k.energy.round_trip_efficiency,
    },
    Gate {
        name: "Auxiliary share of energy imported",
        band: AUX_SHARE_OF_IMPORT,
        bound: Bound::Sanity,
        source: "no public dataset publishes this at plant scale; see the note below",
        read: |k| k.energy.aux_share_of_import,
    },
];

/// Check every gate. Returns one line per failure, empty when all hold.
pub fn check(kpis: &Kpis) -> Vec<String> {
    let mut failures = Vec::new();
    for gate in GATES {
        let value = (gate.read)(kpis);
        if !gate.band.contains(value) {
            failures.push(format!(
                "{}: measured {value:.4}, outside [{:.4}, {:.4}] ({}, {})",
                gate.name,
                gate.band.lo,
                gate.band.hi,
                gate.bound.label(),
                gate.source
            ));
        }
    }
    let residual = kpis.energy.balance_residual_share;
    if residual > BALANCE_RESIDUAL_MAX {
        failures.push(format!(
            "energy balance residual {residual:.7} of throughput, above {BALANCE_RESIDUAL_MAX:.1e}: \
             the loss accounts do not explain what crossed the POI"
        ));
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run that would pass every gate, as the starting point for showing
    /// what makes each one fail.
    fn passing() -> Kpis {
        let text = include_str!("../../../calibration/m1-annual.json");
        crate::report::from_json(text).expect("the committed record parses")
    }

    #[test]
    fn the_committed_record_passes_every_gate() {
        assert!(check(&passing()).is_empty());
    }

    #[test]
    fn a_run_outside_a_band_is_reported() {
        // Each gate in turn, pushed just past its own edge. A gate that
        // reads the wrong field, or that is listed but never evaluated,
        // leaves one of these silent.
        for gate in GATES {
            for value in [gate.band.lo - 0.001, gate.band.hi + 0.001] {
                let mut kpis = passing();
                match gate.name {
                    "Annual round-trip efficiency at the POI" => {
                        kpis.energy.round_trip_efficiency = value;
                    }
                    "Auxiliary share of energy imported" => {
                        kpis.energy.aux_share_of_import = value;
                    }
                    other => panic!("gate {other} has no case here"),
                }
                let failures = check(&kpis);
                assert!(
                    failures.iter().any(|line| line.starts_with(gate.name)),
                    "{} at {value} was not reported: {failures:?}",
                    gate.name
                );
            }
        }
    }

    #[test]
    fn books_that_do_not_close_are_reported() {
        let mut kpis = passing();
        kpis.energy.balance_residual_share = BALANCE_RESIDUAL_MAX * 1.1;
        let failures = check(&kpis);
        assert!(
            failures
                .iter()
                .any(|line| line.contains("balance residual")),
            "an unexplained residual passed: {failures:?}"
        );
    }
}
