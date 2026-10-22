//! The outbox against a tap, with a list for a wire: what a dropout
//! stores, what a burst replays, and what live has to wait for.

use bess_core::PlantConfig;
use bess_scenario::{load, RunParams};
use rumqttc::{ClientError, QoS};

use super::{Message, Outbox, Sink};
use crate::map;
use crate::tap::Taps;

/// A dropout over [10, 20), its burst at 30, and a second dropout over
/// [40, 50) with no burst of its own.
const STORY: &[u8] = br"
schema: 1
name: outbox
events:
  - at: PT10S
    surface: mqtt
    fault: dropout
    duration: PT10S
  - at: PT30S
    surface: mqtt
    fault: backfill_burst
  - at: PT40S
    surface: mqtt
    fault: dropout
    duration: PT10S
";

fn taps() -> Taps {
    let cfg = PlantConfig::gw01();
    let params = RunParams {
        seed: 1,
        start_unix_s: 0,
        speed: 1.0,
        age_years: 0,
    };
    let resolved = load(STORY, &cfg).unwrap().resolve(params);
    Taps::new(&resolved, &map::build_points(&cfg))
}

/// The wire as a list.
#[derive(Default)]
struct Wire(Vec<Message>);

impl Sink for Wire {
    fn publish(&mut self, msg: Message) -> Result<(), ClientError> {
        self.0.push(msg);
        Ok(())
    }
}

fn message(payload: &str, retain: bool) -> Message {
    Message {
        topic: "t".into(),
        payload: payload.into(),
        qos: QoS::AtMostOnce,
        retain,
    }
}

fn payloads(wire: &Wire) -> Vec<&str> {
    wire.0.iter().map(|m| m.payload.as_str()).collect()
}

#[test]
fn a_dropout_stores_and_the_burst_replays_it_unretained_ahead_of_live() {
    let taps = taps();
    let mut outbox = Outbox::default();
    let mut wire = Wire::default();
    outbox
        .send(&mut wire, &taps, 9, message("live 9", true))
        .unwrap();
    for tick in 10..20 {
        outbox
            .send(
                &mut wire,
                &taps,
                tick,
                message(&format!("stored {tick}"), true),
            )
            .unwrap();
    }
    outbox
        .send(&mut wire, &taps, 20, message("live 20", true))
        .unwrap();
    assert_eq!(
        payloads(&wire),
        ["live 9", "live 20"],
        "the dropout is silence"
    );
    // The device came back, which the publisher learns once.
    assert!(outbox.returned(&taps, 25));
    assert!(!outbox.returned(&taps, 26));

    // Nothing replays before the burst's instant.
    outbox.backfill(&mut wire, &taps, 29);
    assert_eq!(wire.0.len(), 2);
    outbox.backfill(&mut wire, &taps, 30);
    let replayed: Vec<&str> = payloads(&wire)[2..].to_vec();
    assert_eq!(replayed.len(), 10, "{replayed:?}");
    assert_eq!(replayed[0], "stored 10");
    assert_eq!(replayed[9], "stored 19");
    assert!(
        wire.0[2..].iter().all(|m| !m.retain),
        "a replayed message is history, not state"
    );
    // The burst replays once.
    outbox.backfill(&mut wire, &taps, 31);
    assert_eq!(wire.0.len(), 12);
}

#[test]
fn live_waits_behind_a_replay_and_keeps_its_retain_flag() {
    let taps = taps();
    let mut outbox = Outbox::default();
    let mut wire = Wire::default();
    // More than one slice of stored messages.
    for i in 0..(super::BURST + 3) {
        outbox
            .send(&mut wire, &taps, 15, message(&format!("stored {i}"), false))
            .unwrap();
    }
    outbox.backfill(&mut wire, &taps, 30);
    assert_eq!(wire.0.len(), super::BURST, "one slice per wake");
    // A word changes while the replay is still draining.
    outbox
        .send(&mut wire, &taps, 30, message("word", true))
        .unwrap();
    assert_eq!(wire.0.len(), super::BURST, "live waits its turn");
    outbox.backfill(&mut wire, &taps, 31);
    let tail = &wire.0[super::BURST..];
    assert_eq!(
        tail.iter().map(|m| m.payload.as_str()).collect::<Vec<_>>(),
        [
            format!("stored {}", super::BURST),
            format!("stored {}", super::BURST + 1),
            format!("stored {}", super::BURST + 2),
            "word".into()
        ]
    );
    assert!(tail[3].retain, "the live word stays retained");
    // The replay is over: live goes straight out again.
    outbox
        .send(&mut wire, &taps, 32, message("after", false))
        .unwrap();
    assert_eq!(wire.0.last().unwrap().payload, "after");
}

#[test]
fn a_new_dropout_ends_the_replay_and_its_return_is_reported_once() {
    let taps = taps();
    let mut outbox = Outbox::default();
    let mut wire = Wire::default();
    for i in 0..=super::BURST {
        outbox
            .send(&mut wire, &taps, 15, message(&format!("stored {i}"), false))
            .unwrap();
    }
    outbox.backfill(&mut wire, &taps, 30);
    assert_eq!(wire.0.len(), super::BURST);
    // The second dropout begins before the replay drained its last message.
    outbox
        .send(&mut wire, &taps, 40, message("stored again", false))
        .unwrap();
    outbox
        .send(&mut wire, &taps, 50, message("live 50", false))
        .unwrap();
    assert_eq!(
        wire.0.last().unwrap().payload,
        "live 50",
        "no replay is draining"
    );
    // No burst follows the second dropout: its store is never replayed.
    outbox.backfill(&mut wire, &taps, 55);
    assert_eq!(wire.0.len(), super::BURST + 1);
    assert!(
        !payloads(&wire).contains(&"stored again"),
        "{:?}",
        payloads(&wire)
    );
    // The device came back: once.
    assert!(outbox.returned(&taps, 55));
    assert!(!outbox.returned(&taps, 56));
}
