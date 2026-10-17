//! Which rows the alarm panel shows: by severity, by block, and whether PCS
//! transitions and applied faults are listed beside the alarm edges.

use bess_core::alarms::Severity;
use bess_core::kernel::Event;

use super::{block_of, row, ActiveAlarm};

/// The panel's filter toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    /// Show trips (high byte).
    pub trips: bool,
    /// Show warnings and status bits (low byte).
    pub warnings: bool,
    /// Only this block and its racks; `None` for the whole site. Site
    /// entries belong to no block, so a block filter hides them.
    pub block: Option<usize>,
    /// Hide PCS transitions and applied faults in the journal.
    pub alarms_only: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            trips: true,
            warnings: true,
            block: None,
            alarms_only: false,
        }
    }
}

impl Filter {
    fn admits_severity(self, severity: Severity) -> bool {
        match severity {
            Severity::Trip => self.trips,
            Severity::Warning => self.warnings,
        }
    }

    fn admits_block(self, block: Option<usize>) -> bool {
        self.block.is_none_or(|want| block == Some(want))
    }

    /// Whether a summary row is shown.
    pub fn admits_alarm(self, alarm: ActiveAlarm) -> bool {
        self.admits_severity(alarm.severity()) && self.admits_block(block_of(alarm.node))
    }

    /// Whether a journal row is shown.
    pub fn admits_event(self, event: &Event) -> bool {
        let by_kind = match row::severity(event) {
            Some(severity) => self.admits_severity(severity),
            None => !self.alarms_only,
        };
        by_kind && self.admits_block(row::block(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::alarms::layout::{block, site};
    use bess_core::alarms::AlarmNode;
    use bess_core::kernel::FaultAction;

    const PCS_FAULT: ActiveAlarm = ActiveAlarm {
        node: AlarmNode::Block { block: 2 },
        bit: block::PCS_FAULT,
    };
    const LIMITED: ActiveAlarm = ActiveAlarm {
        node: AlarmNode::Site,
        bit: site::POWER_LIMITED,
    };

    #[test]
    fn the_default_shows_everything() {
        let f = Filter::default();
        assert!(f.admits_alarm(PCS_FAULT) && f.admits_alarm(LIMITED));
        assert!(f.admits_event(&Event::FaultApplied(FaultAction::ProtectionTrip)));
    }

    #[test]
    fn severity_and_block_narrow_the_summary() {
        let trips_only = Filter {
            warnings: false,
            ..Filter::default()
        };
        assert!(trips_only.admits_alarm(PCS_FAULT));
        assert!(!trips_only.admits_alarm(LIMITED));

        let block2 = Filter {
            block: Some(2),
            ..Filter::default()
        };
        assert!(block2.admits_alarm(PCS_FAULT));
        assert!(!block2.admits_alarm(LIMITED), "the site is in no block");
        let block3 = Filter {
            block: Some(3),
            ..Filter::default()
        };
        assert!(!block3.admits_alarm(PCS_FAULT));
    }

    #[test]
    fn alarms_only_hides_transitions_and_faults() {
        let f = Filter {
            alarms_only: true,
            ..Filter::default()
        };
        let trip = Event::FaultApplied(FaultAction::PcsTrip { block: 2 });
        assert!(!f.admits_event(&trip));
        let raise = Event::AlarmRaised {
            node: PCS_FAULT.node,
            bit: PCS_FAULT.bit,
            severity: Severity::Trip,
        };
        assert!(f.admits_event(&raise));
        assert!(Filter::default().admits_event(&trip));
    }
}
