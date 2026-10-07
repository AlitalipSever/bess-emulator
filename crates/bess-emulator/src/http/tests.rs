//! The stream's message: the summary, with the events since the last push.

use std::sync::Arc;

use bess_core::alarms::{AlarmNode, Severity};
use bess_core::kernel::Event;
use bess_core::{PlantConfig, SiteState};

use super::*;

fn snapshot() -> Snapshot {
    let cfg = PlantConfig::gw01();
    Snapshot {
        state: SiteState::new(&cfg, 1, 0),
        input_regs: Vec::new(),
        holding_regs: Vec::new(),
        speed: 60.0,
    }
}

/// Events from several ticks arrive in one message, in log order, each
/// numbered and stamped with its own tick; a lag is reported, not hidden.
#[test]
fn the_stream_carries_every_event_since_the_last_push() {
    let raise = Event::AlarmRaised {
        node: AlarmNode::Site,
        bit: 0,
        severity: Severity::Warning,
    };
    let clear = Event::AlarmCleared {
        node: AlarmNode::Site,
        bit: 0,
        severity: Severity::Warning,
    };
    let batches = [
        Arc::new(TickEvents {
            unix_time_s: 100,
            first_seq: 7,
            events: vec![raise],
        }),
        Arc::new(TickEvents {
            unix_time_s: 160,
            first_seq: 8,
            events: vec![clear],
        }),
    ];
    let message = stream_message(&snapshot(), &batches, 0);
    let events = message["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        (&events[0]["seq"], &events[0]["ts"], &events[0]["event"]),
        (&json!(7), &json!(100), &json!("raised"))
    );
    assert_eq!(
        (&events[1]["seq"], &events[1]["ts"], &events[1]["event"]),
        (&json!(8), &json!(160), &json!("cleared"))
    );
    assert!(message.get("events_lost_ticks").is_none());
    // The summary is still all there.
    assert_eq!(message["site"], "GW-01");
    assert_eq!(message["event_count"], 0);
    // Block words carry the name they have on Modbus and MQTT, where plain
    // `alarm_bits` is the rack fold.
    assert_eq!(message["blocks"][0]["block_alarm_bits"], 0);
    assert!(message["blocks"][0].get("alarm_bits").is_none());

    let quiet = stream_message(&snapshot(), &[], 3);
    assert_eq!(quiet["events"], json!([]));
    assert_eq!(quiet["events_lost_ticks"], 3);
}
