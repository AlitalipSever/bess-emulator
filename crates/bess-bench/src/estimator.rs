//! What the BMS estimators said over the run, against the truth.
//!
//! The one place the truth is allowed to meet the estimate: the research
//! surface. The published plant carries the estimate alone, so measuring
//! how wrong it is, and how often it admits so, is a calibration activity,
//! and the record keeps it.

use bess_core::kernel::Event;
use bess_core::SiteState;
use serde::{Deserialize, Serialize};

/// Estimator error over the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimatorKpis {
    /// Mean absolute difference between reported and true rack SoC, in
    /// percentage points, over every rack at every sample.
    pub mean_abs_error_pct: f64,
    /// The largest difference any rack showed at any sample, percentage
    /// points.
    pub max_abs_error_pct: f64,
    /// Correction jumps (`SocResnapped` events) over the run.
    pub corrections: u64,
    /// The same per rack and day.
    pub corrections_per_rack_day: f64,
}

/// The running tally behind the figures.
#[derive(Debug, Default)]
pub struct Tally {
    sum_abs: f64,
    samples: u64,
    max_abs: f64,
    corrections: u64,
}

impl Tally {
    /// Count one tick's events.
    pub fn observe_events(&mut self, events: &[Event]) {
        self.corrections += events
            .iter()
            .filter(|e| matches!(e, Event::SocResnapped { .. }))
            .count() as u64;
    }

    /// Sample every rack's error.
    pub fn observe_state(&mut self, state: &SiteState) {
        for rack in state.racks() {
            let err = (rack.soc_reported - rack.soc).abs();
            self.sum_abs += err;
            self.max_abs = self.max_abs.max(err);
            self.samples += 1;
        }
    }

    /// Close the tally.
    pub fn finish(self, racks: usize, days: u64) -> EstimatorKpis {
        let mean = if self.samples == 0 {
            0.0
        } else {
            self.sum_abs / self.samples as f64
        };
        EstimatorKpis {
            mean_abs_error_pct: super::run::round(mean * 100.0, 3),
            max_abs_error_pct: super::run::round(self.max_abs * 100.0, 2),
            corrections: self.corrections,
            corrections_per_rack_day: super::run::round(
                self.corrections as f64 / (racks as f64 * days as f64),
                3,
            ),
        }
    }
}
