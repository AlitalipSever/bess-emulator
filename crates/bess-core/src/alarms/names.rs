//! How the surfaces spell a node, an alarm, a severity and a PCS state.
//!
//! MQTT topics, stream payloads, the REST reset and the browser viewer all
//! name things the same way, and COMPATIBILITY.md publishes that spelling.
//! It lives here, beside the bit layout that owns the other half of it, so
//! that every consumer holds the same strings from one function.

use crate::kernel::FaultAction;
use crate::state::PcsOpState;

use super::{layout, AlarmNode, ResetScope, Severity};

impl AlarmNode {
    /// Path of the node: `site`, `block02`, `block02/container1/rack05`.
    pub fn path(self) -> String {
        match self {
            Self::Site => "site".to_owned(),
            Self::Block { block } => format!("block{block:02}"),
            Self::Rack {
                block,
                container,
                rack,
            } => format!("block{block:02}/container{container}/rack{rack:02}"),
        }
    }

    /// The word the node carries: `site`, `block` or `rack`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Site => "site",
            Self::Block { .. } => "block",
            Self::Rack { .. } => "rack",
        }
    }

    /// Published name of a bit on this node, `rack.derate_active`. A
    /// position the layout leaves free reads `rack.bit11`; the kernel never
    /// raises one, but a name that fails loudly is better than an event
    /// that cannot be published.
    pub fn alarm_name(self, bit: u8) -> String {
        layout::name(self, bit).unwrap_or_else(|| format!("{}.bit{bit}", self.word()))
    }
}

impl Severity {
    /// Published name: `warning` or `trip`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Trip => "trip",
        }
    }
}

impl PcsOpState {
    /// Published name: `off`, `precharge`, `contactor_close`,
    /// `synchronize`, `ramp`, `standby`, `run` or `fault`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Precharge => "precharge",
            Self::ContactorClose => "contactor_close",
            Self::Synchronize => "synchronize",
            Self::Ramp => "ramp",
            Self::Standby => "standby",
            Self::Run => "run",
            Self::Fault => "fault",
        }
    }
}

impl FaultAction {
    /// Path of the node the action acted on, in [`AlarmNode::path`]'s
    /// spelling. An HVAC unit has no alarm word of its own, so it is spelled
    /// as the tree addresses it, `block02/container0/hvac`, beside the rack
    /// paths.
    pub fn node_path(&self) -> String {
        match *self {
            Self::HvacFailure { block, container } | Self::HvacRepair { block, container } => {
                format!("block{block:02}/container{container}/hvac")
            }
            Self::PcsTrip { block }
            | Self::BlockMaintenanceEnter { block }
            | Self::BlockMaintenanceExit { block }
            | Self::AlarmReset(ResetScope::Block(block)) => AlarmNode::Block { block }.path(),
            Self::ProtectionTrip
            | Self::AlarmReset(ResetScope::Site)
            | Self::HvBreakerOpen
            | Self::HvBreakerClose
            | Self::Disconnectors { .. }
            | Self::EarthingSwitch { .. }
            | Self::GridFrequencyOffset { .. }
            | Self::ExternalLimit { .. }
            | Self::ExternalLimitClear
            | Self::Award { .. }
            | Self::AwardsClear => AlarmNode::Site.path(),
            Self::RackSelfDischarge {
                block,
                container,
                rack,
                ..
            }
            | Self::RackIsolate {
                block,
                container,
                rack,
            }
            | Self::RackRestore {
                block,
                container,
                rack,
            }
            | Self::AlarmReset(ResetScope::Rack {
                block,
                container,
                rack,
            }) => AlarmNode::Rack {
                block,
                container,
                rack,
            }
            .path(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alarms::layout::{block, rack, site};

    const RACK: AlarmNode = AlarmNode::Rack {
        block: 2,
        container: 1,
        rack: 5,
    };

    /// The strings as the surfaces have published them since M2 phase 2,
    /// held field for field, so a move between crates cannot change a byte.
    #[test]
    fn nodes_and_alarms_are_spelled_as_published() {
        assert_eq!(AlarmNode::Site.path(), "site");
        assert_eq!(AlarmNode::Block { block: 7 }.path(), "block07");
        assert_eq!(RACK.path(), "block02/container1/rack05");
        assert_eq!(RACK.alarm_name(rack::DERATE_ACTIVE), "rack.derate_active");
        assert_eq!(
            AlarmNode::Block { block: 7 }.alarm_name(block::PCS_FAULT),
            "block.pcs_fault"
        );
        assert_eq!(
            AlarmNode::Site.alarm_name(site::POWER_LIMITED),
            "site.power_limited"
        );
        assert_eq!(AlarmNode::Site.alarm_name(12), "site.bit12");
        assert_eq!(RACK.alarm_name(11), "rack.bit11");
        assert_eq!(Severity::Warning.name(), "warning");
        assert_eq!(Severity::Trip.name(), "trip");
        assert_eq!(PcsOpState::Standby.name(), "standby");
        assert_eq!(PcsOpState::Run.name(), "run");
        assert_eq!(PcsOpState::Fault.name(), "fault");
    }

    /// Every action, the node it is published under.
    #[test]
    fn every_fault_action_names_its_node() {
        let (b, c, r) = (2, 1, 5);
        let cases = [
            (
                FaultAction::HvacFailure {
                    block: b,
                    container: 0,
                },
                "block02/container0/hvac",
            ),
            (
                FaultAction::HvacRepair {
                    block: b,
                    container: 1,
                },
                "block02/container1/hvac",
            ),
            (FaultAction::PcsTrip { block: 7 }, "block07"),
            (FaultAction::ProtectionTrip, "site"),
            (
                FaultAction::RackSelfDischarge {
                    block: b,
                    container: c,
                    rack: r,
                    excess_ppm_per_day: 333,
                },
                "block02/container1/rack05",
            ),
            (
                FaultAction::RackIsolate {
                    block: b,
                    container: c,
                    rack: r,
                },
                "block02/container1/rack05",
            ),
            (
                FaultAction::RackRestore {
                    block: b,
                    container: c,
                    rack: r,
                },
                "block02/container1/rack05",
            ),
            (FaultAction::BlockMaintenanceEnter { block: 7 }, "block07"),
            (FaultAction::BlockMaintenanceExit { block: 7 }, "block07"),
            (FaultAction::AlarmReset(ResetScope::Site), "site"),
            (FaultAction::AlarmReset(ResetScope::Block(7)), "block07"),
            (
                FaultAction::AlarmReset(ResetScope::Rack {
                    block: b,
                    container: c,
                    rack: r,
                }),
                "block02/container1/rack05",
            ),
        ];
        for (action, path) in cases {
            assert_eq!(action.node_path(), path, "{action:?}");
        }
    }
}
