//! What the alarm tree said over the run.
//!
//! The gate is a single claim: a clean replayed year trips nothing. A plant
//! that false-trips on a normal year would fail the realism it claims, and
//! a harness that only watched energy would never notice. Warnings are
//! counted and published but not gated; the reference plan runs out of
//! energy twice a day, and a plant that says so is telling the truth.

use std::collections::BTreeMap;

use bess_core::alarms::{layout, Severity};
use bess_core::kernel::Event;
use serde::{Deserialize, Serialize};

/// Raise counts over the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AlarmKpis {
    /// Trip-byte raises. The gate: zero on the reference year.
    pub trips_raised: u64,
    /// Warning-byte raises.
    pub warnings_raised: u64,
    /// Raises per published alarm name, `word.name`, alarms that never
    /// raised left out.
    pub raised_by_alarm: BTreeMap<String, u64>,
}

impl AlarmKpis {
    /// Count one tick's events.
    pub fn observe(&mut self, events: &[Event]) {
        for event in events {
            if let Event::AlarmRaised {
                node,
                bit,
                severity,
            } = *event
            {
                match severity {
                    Severity::Trip => self.trips_raised += 1,
                    Severity::Warning => self.warnings_raised += 1,
                }
                let name = layout::name(node, bit).unwrap_or_else(|| format!("unlaid.{bit}"));
                *self.raised_by_alarm.entry(name).or_insert(0) += 1;
            }
        }
    }
}
