//! The publisher: cadence points by class, alarm words on change, events
//! as they happen, every message through the tap.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bess_scenario::Surface;
use rumqttc::{AsyncClient, QoS};
use tracing::{debug, warn};

use super::tap::{Message, Outbox};
use super::{due, point_payload, point_topic, TOPIC_PREFIX};
use crate::events;
use crate::map::Class;
use crate::sim::{SimHandle, Snapshot, TickEvents};

fn class_index(class: Class) -> Option<usize> {
    match class {
        Class::Fast => Some(0),
        Class::Medium => Some(1),
        Class::Slow => Some(2),
        Class::Event => None,
    }
}

/// The device's clock: the plant's time plus whatever drift the tap has
/// open, in whole seconds.
fn device_time_s(snap: &Snapshot) -> i64 {
    let drift = snap.taps.drift_s(Surface::Mqtt, snap.state.tick);
    snap.state.unix_time_s() + drift.round() as i64
}

/// A word with its lowest bit flapped.
fn flapped(word: f64) -> f64 {
    f64::from((word as u32) ^ 1)
}

/// What the publisher remembers between wakes.
pub(super) struct Publisher {
    client: AsyncClient,
    handle: SimHandle,
    topics: Vec<String>,
    /// Simulation timestamp of the last publication per cadence class,
    /// `None` before the first.
    last_pub_s: [Option<i64>; 3],
    /// Last value published per event-class point; `None` until the first.
    last_event_value: Vec<Option<f64>>,
    /// Connections the broker has accepted, counted by the event loop.
    connections: Arc<AtomicU64>,
    /// The connection `last_event_value` was published on.
    published_on: u64,
    /// Events the client refused to queue since the start.
    dropped_events: u64,
    /// The door to the wire, which a dropout closes.
    outbox: Outbox,
}

impl Publisher {
    pub(super) fn new(client: AsyncClient, handle: SimHandle, connections: Arc<AtomicU64>) -> Self {
        let topics = handle.points.iter().map(|p| point_topic(&p.name)).collect();
        let last_event_value = vec![None; handle.points.len()];
        Self {
            client,
            handle,
            topics,
            last_pub_s: [None; 3],
            last_event_value,
            connections,
            published_on: 0,
            dropped_events: 0,
            outbox: Outbox::default(),
        }
    }

    /// Replay what a backfill burst owes, ahead of anything live.
    pub(super) fn backfill(&mut self) {
        let snap = self.handle.snapshot.borrow().clone();
        self.outbox
            .backfill(&mut self.client, &snap.taps, snap.state.tick);
    }

    /// The cadence points whose class period has elapsed in simulation time.
    pub(super) fn publish_cadence(&mut self) {
        let snap = self.handle.snapshot.borrow().clone();
        let sim_time_s = snap.state.unix_time_s();
        let tick = snap.state.tick;
        let ts = device_time_s(&snap);
        for (i, (point, topic)) in self.handle.points.iter().zip(&self.topics).enumerate() {
            let (Some(idx), Some(period)) = (class_index(point.class), point.class.period_s())
            else {
                continue;
            };
            if !due(self.last_pub_s[idx], sim_time_s, period) {
                continue;
            }
            let truth = (point.extract)(&snap.state);
            let value = snap.taps.value(Surface::Mqtt, i, tick, truth);
            let msg = Message {
                topic: topic.clone(),
                payload: point_payload(ts, value, point.unit),
                qos: QoS::AtMostOnce,
                retain: false,
            };
            if let Err(err) = self.outbox.send(&mut self.client, &snap.taps, tick, msg) {
                // Broker down or queue full: drop this batch, the event
                // loop reconnects on its own.
                debug!("mqtt: publish skipped: {err}");
                break;
            }
        }
        for (last, period) in self.last_pub_s.iter_mut().zip([1i64, 10, 60]) {
            if due(*last, sim_time_s, period) {
                *last = Some(sim_time_s);
            }
        }
    }

    /// The event-class points whose value differs from what was last
    /// published, retained. A chatter's flap is such a change.
    pub(super) fn publish_changed(&mut self) {
        let snap = self.handle.snapshot.borrow().clone();
        let tick = snap.state.tick;
        let ts = device_time_s(&snap);
        // A new connection may be to a broker that lost its retained
        // store, and a dropout that ended left the broker with the words
        // from before it: forget what was published, so every word goes
        // out again.
        let connection = self.connections.load(Ordering::Relaxed);
        let returned = self.outbox.returned(&snap.taps, tick);
        if connection != self.published_on || returned {
            self.last_event_value.fill(None);
            self.published_on = connection;
        }
        for (i, point) in self.handle.points.iter().enumerate() {
            if point.class != Class::Event {
                continue;
            }
            let truth = (point.extract)(&snap.state);
            let mut value = snap.taps.value(Surface::Mqtt, i, tick, truth);
            if snap.taps.flipped(Surface::Mqtt, i, tick) {
                value = flapped(value);
            }
            if self.last_event_value[i] == Some(value) {
                continue;
            }
            let msg = Message {
                topic: self.topics[i].clone(),
                payload: point_payload(ts, value, point.unit),
                qos: QoS::AtLeastOnce,
                retain: true,
            };
            match self.outbox.send(&mut self.client, &snap.taps, tick, msg) {
                Ok(()) => self.last_event_value[i] = Some(value),
                // Left unrecorded, so the next wake tries again.
                Err(err) => debug!("mqtt: publish skipped: {err}"),
            }
        }
    }

    /// Every event of one tick, in log order, stamped by the device clock.
    pub(super) fn publish_events(&mut self, batch: &TickEvents) {
        let snap = self.handle.snapshot.borrow().clone();
        let tick = snap.state.tick;
        let drift = snap.taps.drift_s(Surface::Mqtt, tick).round() as i64;
        for (seq, event) in batch.numbered() {
            let msg = Message {
                topic: events::topic(TOPIC_PREFIX, event),
                payload: events::payload(event, seq, batch.unix_time_s + drift).to_string(),
                qos: QoS::AtLeastOnce,
                retain: false,
            };
            if let Err(err) = self.outbox.send(&mut self.client, &snap.taps, tick, msg) {
                // Unlike a skipped telemetry sample, a lost event is never
                // repeated, so it is counted and said out loud; the gap in
                // `seq` tells subscribers.
                self.dropped_events += 1;
                if self.dropped_events == 1 || self.dropped_events.is_multiple_of(1000) {
                    warn!(
                        "mqtt: event {seq} dropped ({} so far): {err}",
                        self.dropped_events
                    );
                }
            }
        }
    }
}
