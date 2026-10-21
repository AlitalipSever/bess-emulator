//! Topics and payloads, one test per event kind.

use bess_core::alarms::layout::{block, rack};
use bess_core::alarms::ResetScope;
use bess_core::state::PcsOpState;

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

/// A fault goes out under the node it acted on, named as the scenario
/// file names it, with the parameter it carried.
#[test]
fn a_fault_is_published_under_its_node_with_its_parameter() {
    let leak = Event::FaultApplied(FaultAction::RackSelfDischarge {
        block: 2,
        container: 1,
        rack: 5,
        excess_ppm_per_day: 6_667,
    });
    assert_eq!(
        topic(PREFIX, &leak),
        "bess/gw01/events/block02/container1/rack05/fault"
    );
    assert_eq!(
        payload(&leak, 12, 1_720_569_600),
        json!({"seq": 12, "ts": 1_720_569_600, "node": "block02/container1/rack05",
               "event": "fault", "fault": "self_discharge", "rate_ppm_per_day": 6_667})
    );

    let hvac = Event::FaultApplied(FaultAction::HvacFailure {
        block: 2,
        container: 0,
    });
    assert_eq!(
        topic(PREFIX, &hvac),
        "bess/gw01/events/block02/container0/hvac/fault"
    );
    assert_eq!(
        payload(&hvac, 1, 0),
        json!({"seq": 1, "ts": 0, "node": "block02/container0/hvac",
               "event": "fault", "fault": "failure"})
    );
}

/// A protection trip and a site reset act on the site; a reset says
/// which scope it reached.
#[test]
fn site_faults_and_resets_name_their_scope() {
    let trip = Event::FaultApplied(FaultAction::ProtectionTrip);
    assert_eq!(topic(PREFIX, &trip), "bess/gw01/events/site/fault");
    assert_eq!(payload(&trip, 3, 0)["fault"], "protection_trip");

    let reset = Event::FaultApplied(FaultAction::AlarmReset(ResetScope::Block(7)));
    assert_eq!(topic(PREFIX, &reset), "bess/gw01/events/block07/fault");
    assert_eq!(
        payload(&reset, 4, 0),
        json!({"seq": 4, "ts": 0, "node": "block07", "event": "fault",
               "fault": "alarm_reset", "scope": "block07"})
    );
}

#[test]
fn a_free_bit_still_has_a_name() {
    assert_eq!(alarm_name(AlarmNode::Site, 12), "site.bit12");
}
