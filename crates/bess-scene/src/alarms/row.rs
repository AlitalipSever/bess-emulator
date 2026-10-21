//! How an event reads as a journal row: what kind it is, which node, what
//! happened. The words are the surfaces' own (`raised`, `cleared`,
//! `pcs_state`, `fault`, the published node paths and alarm names), so a
//! row reads the way the MQTT payload of the same event does.

use bess_core::alarms::{AlarmNode, ResetScope, Severity};
use bess_core::kernel::{Event, FaultAction};

/// The payload's `event` field: `raised`, `cleared`, `pcs_state`, `fault`.
pub fn kind(event: &Event) -> &'static str {
    match event {
        Event::AlarmRaised { .. } => "raised",
        Event::AlarmCleared { .. } => "cleared",
        Event::PcsStateChanged { .. } => "pcs_state",
        Event::FaultApplied(_) => "fault",
        Event::SocResnapped { .. } => "soc_resnap",
        Event::BreakerSequenceChanged { .. } => "breaker_sequence",
        Event::SwitchOperated { .. } => "switch",
        Event::CommandRefused { .. } => "command_refused",
    }
}

/// The payload's `node` field.
pub fn node_path(event: &Event) -> String {
    match event {
        Event::AlarmRaised { node, .. } | Event::AlarmCleared { node, .. } => node.path(),
        Event::PcsStateChanged { block, .. } => AlarmNode::Block { block: *block }.path(),
        Event::FaultApplied(action) => action.node_path(),
        Event::SocResnapped {
            block,
            container,
            rack,
            ..
        } => AlarmNode::Rack {
            block: *block,
            container: *container,
            rack: *rack,
        }
        .path(),
        Event::BreakerSequenceChanged { .. }
        | Event::SwitchOperated { .. }
        | Event::CommandRefused { .. } => AlarmNode::Site.path(),
    }
}

/// What happened: the alarm name, the PCS transition, or the fault name.
/// Plain ASCII, because egui's bundled fonts have no arrow glyphs.
pub fn what(event: &Event) -> String {
    match event {
        Event::AlarmRaised { node, bit, .. } | Event::AlarmCleared { node, bit, .. } => {
            node.alarm_name(*bit)
        }
        Event::PcsStateChanged { from, to, .. } => {
            format!("{} -> {}", from.name(), to.name())
        }
        Event::FaultApplied(action) => action.name().to_owned(),
        Event::SocResnapped { jump, .. } => format!(
            "soc {:.1} % -> {:.1} %",
            f64::from(jump.from_permille) / 10.0,
            f64::from(jump.to_permille) / 10.0
        ),
        Event::BreakerSequenceChanged { from, to } => {
            format!("breaker {} -> {}", from.name(), to.name())
        }
        Event::SwitchOperated { switch, closed } => format!(
            "{} {}",
            switch.name(),
            if *closed { "closed" } else { "opened" }
        ),
        Event::CommandRefused { command } => format!("command {command} refused"),
    }
}

/// The severity of an alarm edge; `None` for a PCS transition or a fault.
pub fn severity(event: &Event) -> Option<Severity> {
    match event {
        Event::AlarmRaised { severity, .. } | Event::AlarmCleared { severity, .. } => {
            Some(*severity)
        }
        Event::PcsStateChanged { .. }
        | Event::FaultApplied(_)
        | Event::SocResnapped { .. }
        | Event::BreakerSequenceChanged { .. }
        | Event::SwitchOperated { .. }
        | Event::CommandRefused { .. } => None,
    }
}

/// The block an event happened in, `None` for the site.
pub fn block(event: &Event) -> Option<usize> {
    match event {
        Event::AlarmRaised { node, .. } | Event::AlarmCleared { node, .. } => {
            super::block_of(*node)
        }
        Event::PcsStateChanged { block, .. } | Event::SocResnapped { block, .. } => Some(*block),
        Event::FaultApplied(action) => fault_block(action),
        Event::BreakerSequenceChanged { .. }
        | Event::SwitchOperated { .. }
        | Event::CommandRefused { .. } => None,
    }
}

fn fault_block(action: &FaultAction) -> Option<usize> {
    match *action {
        FaultAction::HvacFailure { block, .. }
        | FaultAction::HvacRepair { block, .. }
        | FaultAction::PcsTrip { block }
        | FaultAction::RackSelfDischarge { block, .. }
        | FaultAction::RackIsolate { block, .. }
        | FaultAction::RackRestore { block, .. }
        | FaultAction::BlockMaintenanceEnter { block }
        | FaultAction::BlockMaintenanceExit { block }
        | FaultAction::AlarmReset(ResetScope::Block(block) | ResetScope::Rack { block, .. }) => {
            Some(block)
        }
        FaultAction::ProtectionTrip
        | FaultAction::AlarmReset(ResetScope::Site)
        | FaultAction::HvBreakerOpen
        | FaultAction::HvBreakerClose
        | FaultAction::Disconnectors { .. }
        | FaultAction::EarthingSwitch { .. }
        | FaultAction::GridFrequencyOffset { .. }
        | FaultAction::ExternalLimit { .. }
        | FaultAction::ExternalLimitClear => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::alarms::layout::rack;
    use bess_core::state::PcsOpState;

    #[test]
    fn a_row_reads_like_the_payload_of_the_same_event() {
        let node = AlarmNode::Rack {
            block: 2,
            container: 1,
            rack: 5,
        };
        let derate = Event::AlarmRaised {
            node,
            bit: rack::DERATE_ACTIVE,
            severity: Severity::Warning,
        };
        assert_eq!(kind(&derate), "raised");
        assert_eq!(node_path(&derate), "block02/container1/rack05");
        assert_eq!(what(&derate), "rack.derate_active");
        assert_eq!(severity(&derate), Some(Severity::Warning));
        assert_eq!(block(&derate), Some(2));

        let pcs = Event::PcsStateChanged {
            block: 7,
            from: PcsOpState::Run,
            to: PcsOpState::Fault,
        };
        assert_eq!(kind(&pcs), "pcs_state");
        assert_eq!(node_path(&pcs), "block07");
        assert_eq!(what(&pcs), "run -> fault");
        assert_eq!(severity(&pcs), None);

        let trip = Event::FaultApplied(FaultAction::ProtectionTrip);
        assert_eq!(
            (kind(&trip), node_path(&trip)),
            ("fault", "site".to_owned())
        );
        assert_eq!(what(&trip), "protection_trip");
        assert_eq!(block(&trip), None);

        let hvac = Event::FaultApplied(FaultAction::HvacFailure {
            block: 3,
            container: 0,
        });
        assert_eq!(node_path(&hvac), "block03/container0/hvac");
        assert_eq!(block(&hvac), Some(3));
    }
}
