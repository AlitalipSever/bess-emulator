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
/// plant that mostly sits still, which is what Schimpe et al. 2018 report.
/// The realism claim therefore rests on the round-trip gate above
/// and on the item-by-item waterfall, not on this number; regression
/// detection rests on the committed record, which holds every figure to a
/// tenth of a percent. This bound exists to catch a break gross enough to
/// survive a careless record regeneration.
pub const AUX_SHARE_OF_IMPORT: Band = Band { lo: 0.01, hi: 0.08 };

/// Mean absolute error of the reported SoC against the truth, percentage
/// points. A sanity bound: no public dataset publishes estimator error at
/// rack level over a plant year. The floor says the estimator is not
/// secretly an oracle; the ceiling is well under the field range below.
pub const ESTIMATOR_MEAN_ABS_ERROR_PCT: Band = Band { lo: 0.05, hi: 2.0 };

/// Largest reported-SoC error any rack showed, percentage points. The
/// ceiling is the field range ACCURE reports across more than 100 operating
/// sites (errors up to 15 points, outliers beyond); a plant whose worst
/// rack passes it is claiming a worse fleet than measured. Sanity, not
/// calibration: the source gives a range, not a distribution.
pub const ESTIMATOR_MAX_ABS_ERROR_PCT: Band = Band { lo: 0.2, hi: 15.0 };

/// Unexplained energy as a share of throughput. Not a calibration band: the
/// books either close or the accounting is wrong. The tolerance matches the
/// daily invariant test and exists for float accumulation over tens of
/// millions of ticks, not for unmodeled physics.
pub const BALANCE_RESIDUAL_MAX: f64 = 2.0e-3;

/// Quarter-hours the fiscal meter closes over the annual run: 365 days of
/// 96. Arithmetic, not calibration, so an exact value rather than a band:
/// the reference year has 366 local days and 35,136 quarter-hours, but the
/// run replays a common year and February 29th is not visited.
pub const PERIODS_IN_THE_RUN: u64 = 35_040;
/// Quarter-hours on the 23-hour local day, 2024-03-31.
pub const PERIODS_ON_SHORT_DST_DAY: u64 = 92;
/// Quarter-hours on the 25-hour local day, 2024-10-27.
pub const PERIODS_ON_LONG_DST_DAY: u64 = 100;

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
    Gate {
        name: "Reported SoC, mean absolute error (points)",
        band: ESTIMATOR_MEAN_ABS_ERROR_PCT,
        bound: Bound::Sanity,
        source: "no public rack-level estimator error at plant scale; see CALIBRATION.md",
        read: |k| k.estimator.mean_abs_error_pct,
    },
    Gate {
        name: "Reported SoC, largest error on any rack (points)",
        band: ESTIMATOR_MAX_ABS_ERROR_PCT,
        bound: Bound::Sanity,
        source: "ACCURE 2025 ESS report field range, errors up to 15 points",
        read: |k| k.estimator.max_abs_error_pct,
    },
];

/// The M4 gate, kEUR per MW over the year for the market participant:
/// 50 % to 100 % of the ISEA/enspired Battery Revenue Index's cross-
/// market figure for a two-hour system over the twelve months to October
/// 2024 (about 195 kEUR/MW); the lower bound for the intraday markets the
/// emulator does not model and the perfect foresight it only partly
/// shares. Written before the number (CALIBRATION.md, M4 revenue gate).
pub const MARKET_REVENUE_KEUR_PER_MW: Band = Band {
    lo: 97.5,
    hi: 195.0,
};

/// Check every gate. Returns one line per failure, empty when all hold.
/// The field band on the projected trajectory (M5 phase 4): the mean
/// rack state of health after five years of the clean year's stress.
/// Derived in the M5 design README before the number: Figgener et al.
/// 2024 (LFP home storage, 2.2 points a year at about 200 cycles, 0.7 per
/// 100 cycles, a 2-point spread), the cycle share scaled to GW-01's 280
/// cycles, five years of 1.5 to 3.5 points a year rounded outward.
pub const AGING_SOH_YEAR5: Band = Band { lo: 0.82, hi: 0.93 };

/// The laboratory band (M5 phase 4): cycles to 80 % for a rack at two
/// cycles a day in a 25 °C container, Preger et al. 2020's range across
/// LFP cells at 15 to 35 °C, 0.5C to 3C and three depth windows.
pub const AGING_EFC_TO_80_PCT: Band = Band {
    lo: 2500.0,
    hi: 9000.0,
};

/// The trajectory's gates (M5 phase 4): the year-five mean against the
/// field band, the 25 °C laboratory crossing against the laboratory band.
pub fn check_aging(trajectory: &crate::aging::AgingTrajectory) -> Vec<String> {
    let mut failures = Vec::new();
    match trajectory.years.iter().find(|y| y.year == 5) {
        Some(point) if !AGING_SOH_YEAR5.contains(point.soh_mean) => failures.push(format!(
            "Mean state of health after five projected years: {:.4}, outside [{:.2}, {:.2}] \
             (sourced, Figgener et al. 2024 scaled to the plant's cycles)",
            point.soh_mean, AGING_SOH_YEAR5.lo, AGING_SOH_YEAR5.hi
        )),
        Some(_) => {}
        None => failures.push("the trajectory has no year five; run with --years 5 or more".into()),
    }
    match trajectory
        .lab
        .iter()
        .find(|l| (l.temp_c - 25.0).abs() < 1.0e-9)
    {
        Some(lab) => match lab.efc_to_80_pct {
            Some(efc) if !AGING_EFC_TO_80_PCT.contains(efc) => failures.push(format!(
                "Cycles to 80 % at two cycles a day and 25 °C: {efc:.0}, outside [{:.0}, {:.0}] \
                 (sourced, Preger et al. 2020, LFP cells)",
                AGING_EFC_TO_80_PCT.lo, AGING_EFC_TO_80_PCT.hi
            )),
            Some(_) => {}
            None => failures.push("the 25 °C laboratory rack never reached 80 %".into()),
        },
        None => failures.push("the trajectory has no 25 °C laboratory rack".into()),
    }
    failures
}

pub fn check(kpis: &Kpis) -> Vec<String> {
    let mut failures = Vec::new();
    if kpis.run.market && !MARKET_REVENUE_KEUR_PER_MW.contains(kpis.revenue.total_keur_per_mw) {
        failures.push(format!(
            "Annual revenue per MW: measured {:.2} kEUR/MW, outside [{:.1}, {:.1}] (sourced, \
             ISEA/enspired Battery Revenue Index, two-hour system, twelve months to Oct 2024)",
            kpis.revenue.total_keur_per_mw,
            MARKET_REVENUE_KEUR_PER_MW.lo,
            MARKET_REVENUE_KEUR_PER_MW.hi
        ));
    }
    for gate in GATES {
        // The round-trip band is the arbitrage fleet's: a plant that holds
        // reserve spends its year at partial load, which is cheaper per
        // MWh on every loss row, and reads above it. The market year
        // reports its round trip and is gated on its revenue instead.
        if kpis.run.market && gate.name == GATES[0].name {
            continue;
        }
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
    let p = &kpis.periods;
    let counted = (p.closed, p.on_short_dst_day, p.on_long_dst_day);
    let expected = (
        PERIODS_IN_THE_RUN,
        PERIODS_ON_SHORT_DST_DAY,
        PERIODS_ON_LONG_DST_DAY,
    );
    if counted != expected {
        failures.push(format!(
            "fiscal periods closed / on the short DST day / on the long one: \
             {counted:?}, expected {expected:?}"
        ));
    }
    if kpis.alarms.trips_raised > 0 {
        failures.push(format!(
            "{} trips raised on the reference year ({:?}): a clean plant that trips \
             is failing the realism it claims",
            kpis.alarms.trips_raised, kpis.alarms.raised_by_alarm
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
        assert_eq!(check(&passing()), Vec::<String>::new());
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
                    "Reported SoC, mean absolute error (points)" => {
                        kpis.estimator.mean_abs_error_pct = value;
                    }
                    "Reported SoC, largest error on any rack (points)" => {
                        kpis.estimator.max_abs_error_pct = value;
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

    #[test]
    fn a_single_trip_fails_the_year() {
        let mut kpis = passing();
        kpis.alarms.trips_raised = 1;
        let failures = check(&kpis);
        assert!(
            failures.iter().any(|line| line.contains("trips raised")),
            "a tripping year passed: {failures:?}"
        );
    }

    #[test]
    fn a_miscounted_dst_day_is_reported() {
        let mut kpis = passing();
        kpis.periods.on_long_dst_day = 96;
        let failures = check(&kpis);
        assert!(
            failures.iter().any(|line| line.contains("fiscal periods")),
            "a 24-hour autumn day passed: {failures:?}"
        );
    }
}

#[cfg(test)]
mod aging_tests {
    use super::*;
    use crate::aging::{AgingTrajectory, LabTrajectory, YearPoint};
    use crate::run::RunRecord;

    fn trajectory(soh_year5: f64, efc_to_80: Option<f64>) -> AgingTrajectory {
        AgingTrajectory {
            run: RunRecord {
                site_id: "GW-01".into(),
                engine_version: "0.9.0".into(),
                seed: 7,
                start_unix_s: 1_767_225_600,
                days: 365,
                ticks: 31_536_000,
                market: false,
            },
            method: "projected".into(),
            years: vec![YearPoint {
                year: 5,
                soh_mean: soh_year5,
                soh_min: soh_year5 - 0.01,
                soh_max: soh_year5 + 0.01,
                efc_mean: 1400.0,
                resistance_growth_mean: 0.1,
            }],
            lab: vec![LabTrajectory {
                label: "two cycles a day at 25 °C".into(),
                temp_c: 25.0,
                efc_per_day: 2.0,
                dod: 0.9,
                c_rate: 0.5,
                efc_to_80_pct: efc_to_80,
                years_to_80_pct: efc_to_80.map(|e| e / 730.0),
                soh_at_8000_efc: 0.6,
            }],
        }
    }

    /// A gate that does not fail is not a gate: each band refuses a value
    /// outside it and accepts one inside.
    #[test]
    fn each_band_refuses_outside_and_accepts_inside() {
        assert_eq!(
            check_aging(&trajectory(0.89, Some(3200.0))),
            Vec::<String>::new()
        );
        let too_fast = check_aging(&trajectory(0.80, Some(3200.0)));
        assert_eq!(too_fast.len(), 1, "{too_fast:?}");
        assert!(too_fast[0].starts_with("Mean state of health"));
        let too_slow = check_aging(&trajectory(0.95, Some(3200.0)));
        assert_eq!(too_slow.len(), 1);
        let lab = check_aging(&trajectory(0.89, Some(12_000.0)));
        assert_eq!(lab.len(), 1, "{lab:?}");
        assert!(lab[0].starts_with("Cycles to 80 %"));
        let never = check_aging(&trajectory(0.89, None));
        assert_eq!(never.len(), 1);
        let mut short = trajectory(0.89, Some(3200.0));
        short.years.clear();
        assert_eq!(check_aging(&short).len(), 1);
    }
}
