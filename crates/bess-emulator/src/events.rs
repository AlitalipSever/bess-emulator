//! How a kernel event looks on the surfaces: the node it happened on, its
//! name, its MQTT topic, and its JSON payload.
//!
//! MQTT and the WebSocket stream publish the same object, and the REST reset
//! names nodes the same way, so a consumer reading one surface can match
//! what another said.

use bess_core::alarms::{layout, AlarmNode, ResetScope, Severity};
use bess_core::kernel::{Event, FaultAction};
use bess_core::state::PcsOpState;
use serde_json::{json, Value};

/// Path of an alarm node as topics and payloads spell it: `site`, `block02`,
/// `block02/container1/rack05`.
pub fn node_path(node: AlarmNode) -> String {
    match node {
        AlarmNode::Site => "site".to_owned(),
        AlarmNode::Block { block } => format!("block{block:02}"),
        AlarmNode::Rack {
            block,
            container,
            rack,
        } => format!("block{block:02}/container{container}/rack{rack:02}"),
    }
}

/// Published name of a bit, `rack.derate_active`. A position the layout
/// leaves free reads `rack.bit11`; the kernel never raises one, but a name
/// that fails loudly is better than an event that cannot be published.
pub fn alarm_name(node: AlarmNode, bit: u8) -> String {
    layout::name(node, bit).unwrap_or_else(|| {
        let word = match node {
            AlarmNode::Site => "site",
            AlarmNode::Block { .. } => "block",
            AlarmNode::Rack { .. } => "rack",
        };
        format!("{word}.bit{bit}")
    })
}

/// Published name of a severity.
pub fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Warning => "warning",
        Severity::Trip => "trip",
    }
}

/// Path of the node a fault action acted on, in `node_path`'s spelling. An
/// HVAC unit has no alarm word of its own, so it is spelled as the tree
/// addresses it, `block02/container0/hvac`, beside the rack paths.
pub fn fault_path(action: &FaultAction) -> String {
    match *action {
        FaultAction::HvacFailure { block, container }
        | FaultAction::HvacRepair { block, container } => {
            format!("block{block:02}/container{container}/hvac")
        }
        FaultAction::PcsTrip { block }
        | FaultAction::BlockMaintenanceEnter { block }
        | FaultAction::BlockMaintenanceExit { block }
        | FaultAction::AlarmReset(ResetScope::Block(block)) => {
            node_path(AlarmNode::Block { block })
        }
        FaultAction::ProtectionTrip | FaultAction::AlarmReset(ResetScope::Site) => {
            node_path(AlarmNode::Site)
        }
        FaultAction::RackSelfDischarge {
            block,
            container,
            rack,
            ..
        }
        | FaultAction::RackIsolate {
            block,
            container,
            rack,
        }
        | FaultAction::RackRestore {
            block,
            container,
            rack,
        }
        | FaultAction::AlarmReset(ResetScope::Rack {
            block,
            container,
            rack,
        }) => node_path(AlarmNode::Rack {
            block,
            container,
            rack,
        }),
    }
}

fn pcs_state_name(state: PcsOpState) -> &'static str {
    match state {
        PcsOpState::Standby => "standby",
        PcsOpState::Run => "run",
        PcsOpState::Fault => "fault",
    }
}

/// MQTT topic of an event under `prefix` (which ends in `/`): the events
/// subtree mirrors the telemetry tree, so a rack's derate edge goes to
/// `events/block02/container1/rack05/derate_active` and a PCS transition to
/// `events/block02/pcs/state`, beside where `block02/pcs/state` publishes.
pub fn topic(prefix: &str, event: &Event) -> String {
    match *event {
        Event::AlarmRaised { node, bit, .. } | Event::AlarmCleared { node, bit, .. } => {
            let name = alarm_name(node, bit);
            let short = name.split_once('.').map_or(name.as_str(), |(_, n)| n);
            format!("{prefix}events/{}/{short}", node_path(node))
        }
        Event::PcsStateChanged { block, .. } => {
            format!("{prefix}events/block{block:02}/pcs/state")
        }
        Event::FaultApplied(action) => format!("{prefix}events/{}/fault", fault_path(&action)),
    }
}

/// JSON payload of an event: its number in the kernel's log (`seq`, from 1,
/// the count the Modbus event counter wraps), the simulation time of the
/// snapshot that shows its effect (`ts`, unix seconds), the node, and what
/// happened.
pub fn payload(event: &Event, seq: u64, ts: i64) -> Value {
    match *event {
        Event::AlarmRaised {
            node,
            bit,
            severity,
        }
        | Event::AlarmCleared {
            node,
            bit,
            severity,
        } => json!({
            "seq": seq,
            "ts": ts,
            "node": node_path(node),
            "event": if matches!(event, Event::AlarmRaised { .. }) { "raised" } else { "cleared" },
            "alarm": alarm_name(node, bit),
            "bit": bit,
            "severity": severity_name(severity),
        }),
        Event::PcsStateChanged { block, from, to } => json!({
            "seq": seq,
            "ts": ts,
            "node": node_path(AlarmNode::Block { block }),
            "event": "pcs_state",
            "from": pcs_state_name(from),
            "to": pcs_state_name(to),
        }),
        Event::FaultApplied(action) => {
            let mut v = json!({
                "seq": seq,
                "ts": ts,
                "node": fault_path(&action),
                "event": "fault",
                "fault": action.name(),
            });
            match action {
                FaultAction::RackSelfDischarge {
                    excess_ppm_per_day, ..
                } => v["rate_ppm_per_day"] = json!(excess_ppm_per_day),
                FaultAction::AlarmReset(_) => v["scope"] = json!(fault_path(&action)),
                _ => {}
            }
            v
        }
    }
}

#[cfg(test)]
mod tests;
