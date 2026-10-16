//! The stream's message: the summary, with the events since the last push.

use std::sync::Arc;

use bess_core::alarms::{AlarmNode, Severity};
use bess_core::kernel::Event;
use bess_core::state::BlockMode;
use bess_core::{PlantConfig, SiteState};

use super::*;

fn snapshot() -> Snapshot {
    let cfg = PlantConfig::gw01();
    Snapshot {
        state: SiteState::new(&cfg, 1, 0),
        input_regs: Vec::new(),
        holding_regs: Vec::new(),
        speed: 60.0,
        scenario: None,
        taps: Arc::default(),
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

/// Each block's mode carries the register's value, and the station-service
/// meter sits beside the two POI meters, in kWh like them.
#[test]
fn the_summary_carries_block_mode_and_station_service() {
    let mut snap = snapshot();
    snap.state.blocks[4].mode = BlockMode::Maintenance;
    snap.state.substation.station_service_wh = 2_500.0;
    let summary = summary_value(&snap);
    assert_eq!(summary["blocks"][4]["mode"], 1);
    assert_eq!(summary["blocks"][3]["mode"], 0);
    assert_eq!(summary["station_service_kwh"], 2.5);
    let message = stream_message(&snap, &[], 0);
    assert_eq!(message["blocks"][4]["mode"], 1);
    assert_eq!(message["station_service_kwh"], 2.5);
}

/// The fiscal meter's last closed period and the planner's price ride in
/// the summary, so the stream carries them too.
#[test]
fn the_summary_carries_the_revenue_period_and_the_price() {
    let mut snap = snapshot();
    snap.state.substation.revenue.last_export_wh = 25_000_000.0;
    snap.state.substation.revenue.periods_closed = 4;
    let summary = summary_value(&snap);
    assert_eq!(summary["revenue_period_export_kwh"], 25_000.0);
    assert_eq!(summary["revenue_period_import_kwh"], 0.0);
    assert_eq!(summary["revenue_periods_closed"], 4);
    let price = bess_models::gw01_day_ahead_price_eur_mwh(snap.state.unix_time_s());
    assert_eq!(summary["day_ahead_price_eur_mwh"], price);
    let message = stream_message(&snap, &[], 0);
    assert_eq!(message["revenue_periods_closed"], 4);
}
