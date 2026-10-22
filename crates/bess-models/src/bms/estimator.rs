//! The BMS's state-of-charge estimator: coulomb counting through an
//! imperfect current sensor, a weak pull on the plateau, and a hard
//! correction at the knees.
//!
//! A behavioral model, not a filter. What a register can show of an
//! estimator is that its number walks away from the truth while the cell
//! sits on the LFP plateau, where voltage says nothing and hysteresis
//! (phase 1) says even less, and jumps back when the cell reaches a knee,
//! where voltage finally tells it so. That sawtooth is what this produces,
//! with its drift tied to the sensor's published error bounds and its jumps
//! to where the OCV curve bends. The estimator reads the truth only in the
//! two places a real one reads its instruments: the current it counts is
//! the true current through the sensor's error, and the knee test stands
//! in for the voltage measurement a real BMS would make there.

use bess_core::config::RackConfig;
use bess_core::state::RackState;
use bess_core::traits::SocResnap;

/// Parameters of the estimator's behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimator {
    /// Whether the estimator counts through the sensor's error at all.
    /// Off, the reported SoC is the truth: the decoupling proof's oracle.
    pub imperfect: bool,
    /// Rate at which the estimate is pulled toward the truth on the
    /// plateau, 1/s: the feeble voltage correction a flat curve allows.
    pub plateau_pull_per_s: f64,
    /// Below this true SoC the lower knee is reachable by voltage.
    pub knee_low: f64,
    /// Above this true SoC the upper knee is reachable by voltage.
    pub knee_high: f64,
    /// Error, in SoC, above which a knee visit triggers a correction.
    pub resnap_threshold: f64,
    /// Share of the error a correction leaves behind: a real correction
    /// lands near the truth, not on it.
    pub resnap_residual: f64,
}

impl Default for Estimator {
    /// GW-01 defaults. Provenance in CALIBRATION.md (M2.5, reported SoC):
    ///
    /// - the sensor bounds the drift and live on the rack (`sensor_offset_a`,
    ///   `sensor_gain`, drawn at commissioning inside a published class);
    /// - `knee_low` / `knee_high`: where the mean OCV curve leaves its
    ///   plateau (10 % and 90 %, the table in `cell.rs`);
    /// - `plateau_pull_per_s`, `resnap_threshold`, `resnap_residual`:
    ///   estimates under sanity bounds, there being no public plant-scale
    ///   observation to fit them to; the resulting error is gated against
    ///   the field range ACCURE reports.
    fn default() -> Self {
        Self {
            imperfect: true,
            plateau_pull_per_s: 1.0 / (10.0 * 86_400.0),
            knee_low: 0.10,
            knee_high: 0.90,
            resnap_threshold: 0.005,
            resnap_residual: 0.2,
        }
    }
}

impl Estimator {
    /// An estimator that reports the truth.
    pub fn perfect() -> Self {
        Self {
            imperfect: false,
            ..Self::default()
        }
    }

    /// Advance one rack's reported SoC by `dt_s`. Returns the correction
    /// jump if one happened this step.
    pub fn step(&self, rack: &mut RackState, cfg: &RackConfig, dt_s: f64) -> Option<SocResnap> {
        if !self.imperfect {
            rack.soc_reported = rack.soc;
            return None;
        }
        // Coulomb counting through the sensor, against the capacity the
        // rack has: the BMS is given the state of health (M5 phase 3,
        // design D7), since a BMS kept counting on the nameplate resnaps
        // every cycle on an aged rack (CALIBRATION.md, the aged plant). An
        // estimator that learns the capacity itself, with the error that
        // carries, is the next deepening of this file.
        let i_meas_a = rack.current_a * rack.sensor_gain + rack.sensor_offset_a;
        let capacity_ah = cfg.cell_capacity_ah * rack.soh;
        let mut est = rack.soc_reported - i_meas_a * dt_s / (3600.0 * capacity_ah);
        // The plateau's feeble voltage correction.
        est += self.plateau_pull_per_s * dt_s * (rack.soc - est);
        est = est.clamp(0.0, 1.0);

        let at_knee = rack.soc <= self.knee_low || rack.soc >= self.knee_high;
        let error = est - rack.soc;
        let jump = if at_knee && error.abs() > self.resnap_threshold {
            let corrected = (rack.soc + self.resnap_residual * error).clamp(0.0, 1.0);
            let j = SocResnap {
                from_permille: permille(est),
                to_permille: permille(corrected),
            };
            est = corrected;
            Some(j)
        } else {
            None
        };
        rack.soc_reported = est;
        jump
    }
}

/// SoC as the per-mille integer a register would carry.
fn permille(soc: f64) -> u16 {
    (soc.clamp(0.0, 1.0) * 1000.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::config::PlantConfig;
    use bess_core::SiteState;

    fn rack(soc: f64, current_a: f64, offset_a: f64) -> RackState {
        let cfg = PlantConfig::gw01();
        let mut r = SiteState::new(&cfg, 7, 0).blocks[0].containers[0].racks[0].clone();
        r.soc = soc;
        r.soc_reported = soc;
        r.current_a = current_a;
        r.sensor_offset_a = offset_a;
        r.sensor_gain = 1.0;
        r
    }

    #[test]
    fn a_positive_offset_walks_the_estimate_down_at_its_rate() {
        let cfg = PlantConfig::gw01().rack;
        let est = Estimator::default();
        let mut r = rack(0.5, 0.0, 0.075);
        for _ in 0..86_400 {
            assert!(est.step(&mut r, &cfg, 1.0).is_none());
        }
        // 75 mA for a day over 314 Ah is 0.573 % of SoC, less the pull.
        let expected = 0.075 * 24.0 / 314.0;
        let drift = r.soc - r.soc_reported;
        assert!(drift > 0.9 * expected && drift < expected, "drift {drift}");
    }

    #[test]
    fn nothing_snaps_on_the_plateau_however_large_the_error() {
        let cfg = PlantConfig::gw01().rack;
        let est = Estimator::default();
        let mut r = rack(0.5, 0.0, 0.0);
        r.soc_reported = 0.45;
        assert!(est.step(&mut r, &cfg, 1.0).is_none());
        assert!((r.soc_reported - 0.45).abs() < 1.0e-6);
    }

    #[test]
    fn a_knee_visit_closes_most_of_the_error_and_reports_the_jump() {
        let cfg = PlantConfig::gw01().rack;
        let est = Estimator::default();
        let mut r = rack(0.08, 0.0, 0.0);
        r.soc_reported = 0.10;
        let jump = est
            .step(&mut r, &cfg, 1.0)
            .expect("a knee visit with 2 % error snaps");
        assert_eq!(jump.from_permille, 100);
        assert_eq!(jump.to_permille, 84);
        assert!((r.soc_reported - 0.084).abs() < 1.0e-6);
        // The residual is under the threshold, so the next tick is quiet.
        assert!(est.step(&mut r, &cfg, 1.0).is_none());
    }

    #[test]
    fn the_perfect_estimator_reports_the_truth() {
        let cfg = PlantConfig::gw01().rack;
        let est = Estimator::perfect();
        let mut r = rack(0.5, 100.0, 0.075);
        r.soc_reported = 0.3;
        assert!(est.step(&mut r, &cfg, 1.0).is_none());
        assert_eq!(r.soc_reported, r.soc);
    }
}
