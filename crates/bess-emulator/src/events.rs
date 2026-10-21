//! How a kernel event looks on the surfaces: the node it happened on, its
//! name, its MQTT topic, and its JSON payload.
//!
//! MQTT and the WebSocket stream publish the same object, and the REST reset
//! names nodes the same way, so a consumer reading one surface can match
//! what another said.

use bess_core::alarms::{AlarmNode, Severity};
use bess_core::kernel::{Event, FaultAction};
use serde_json::{json, Value};

/// Path of an alarm node as topics and payloads spell it: `site`, `block02`,
/// `block02/container1/rack05`. The spelling lives in `bess-core`, so the
/// browser viewer holds the same strings.
pub fn node_path(node: AlarmNode) -> String {
    node.path()
}

/// Published name of a bit, `rack.derate_active`, or `rack.bit11` for a
/// position the layout leaves free.
pub fn alarm_name(node: AlarmNode, bit: u8) -> String {
    node.alarm_name(bit)
}

/// Published name of a severity.
pub fn severity_name(severity: Severity) -> &'static str {
    severity.name()
}

/// Path of the node a fault action acted on, in `node_path`'s spelling.
pub fn fault_path(action: &FaultAction) -> String {
    action.node_path()
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
        Event::SocResnapped {
            block,
            container,
            rack,
            ..
        } => format!(
            "{prefix}events/{}/soc_resnap",
            node_path(AlarmNode::Rack {
                block,
                container,
                rack
            })
        ),
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
            "from": from.name(),
            "to": to.name(),
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
        Event::SocResnapped {
            block,
            container,
            rack,
            jump,
        } => json!({
            "seq": seq,
            "ts": ts,
            "node": node_path(AlarmNode::Rack { block, container, rack }),
            "event": "soc_resnap",
            "from_pct": f64::from(jump.from_permille) / 10.0,
            "to_pct": f64::from(jump.to_permille) / 10.0,
        }),
    }
}

#[cfg(test)]
mod tests;
