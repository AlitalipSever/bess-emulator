//! How a kernel event looks on the surfaces: the node it happened on, its
//! name, its MQTT topic, and its JSON payload.
//!
//! MQTT and the WebSocket stream publish the same object, and the REST reset
//! names nodes the same way, so a consumer reading one surface can match
//! what another said.

use bess_core::alarms::{layout, AlarmNode, Severity};
use bess_core::kernel::Event;
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
    }
}

#[cfg(test)]
mod tests {
    use bess_core::alarms::layout::{block, rack};

    use super::*;

    const PREFIX: &str = "bess/gw01/";

    fn derate(raised: bool) -> Event {
        let node = AlarmNode::Rack {
            block: 2,
            container: 1,
            rack: 5,
        };
        let (bit, severity) = (rack::DERATE_ACTIVE, Severity::Warning);
        if raised {
            Event::AlarmRaised {
                node,
                bit,
                severity,
            }
        } else {
            Event::AlarmCleared {
                node,
                bit,
                severity,
            }
        }
    }

    /// A rack edge goes to the rack's own topic, named by the layout, and
    /// its payload says everything the topic does, so a wildcard subscriber
    /// needs only the payload.
    #[test]
    fn a_rack_edge_is_published_under_its_node_and_name() {
        assert_eq!(
            topic(PREFIX, &derate(true)),
            "bess/gw01/events/block02/container1/rack05/derate_active"
        );
        assert_eq!(
            payload(&derate(true), 41, 1_784_037_600),
            json!({
                "seq": 41,
                "ts": 1_784_037_600,
                "node": "block02/container1/rack05",
                "event": "raised",
                "alarm": "rack.derate_active",
                "bit": 5,
                "severity": "warning",
            })
        );
        assert_eq!(payload(&derate(false), 42, 0)["event"], "cleared");
    }

    #[test]
    fn block_site_and_pcs_events_have_their_own_topics() {
        let pcs = Event::AlarmRaised {
            node: AlarmNode::Block { block: 7 },
            bit: block::PCS_FAULT,
            severity: Severity::Trip,
        };
        assert_eq!(topic(PREFIX, &pcs), "bess/gw01/events/block07/pcs_fault");
        assert_eq!(payload(&pcs, 1, 0)["severity"], "trip");

        let site = Event::AlarmCleared {
            node: AlarmNode::Site,
            bit: 0,
            severity: Severity::Warning,
        };
        assert_eq!(topic(PREFIX, &site), "bess/gw01/events/site/power_limited");

        let transition = Event::PcsStateChanged {
            block: 7,
            from: PcsOpState::Fault,
            to: PcsOpState::Standby,
        };
        assert_eq!(
            topic(PREFIX, &transition),
            "bess/gw01/events/block07/pcs/state"
        );
        assert_eq!(
            payload(&transition, 9, 60),
            json!({"seq": 9, "ts": 60, "node": "block07", "event": "pcs_state",
                   "from": "fault", "to": "standby"})
        );
    }

    #[test]
    fn a_free_bit_still_has_a_name() {
        assert_eq!(alarm_name(AlarmNode::Site, 7), "site.bit7");
    }
}
