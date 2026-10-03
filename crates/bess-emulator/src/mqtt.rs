//! MQTT publisher: every signal-map point becomes a topic under
//! `bess/gw01/`, and every kernel event a message under `bess/gw01/events/`.
//!
//! Cadence points are decimated per publication class in simulation time.
//! At 1x speed that matches the class table (fast 1 s, medium 10 s, slow
//! 60 s). When accelerated, publications collapse to at most one batch per
//! wall second carrying the latest values, which mirrors how a real
//! historian would sample a faster-than-life data source.
//!
//! Event-class points (the alarm words and the counts read off them) are
//! published when they change, retained, so a subscriber arriving late gets
//! the current word from the broker, and all of them again after every
//! reconnect, because a broker that restarted without persistence has
//! forgotten them. Events themselves are published as they happen, whatever
//! the speed: they come off the simulation's event channel rather than the
//! latest snapshot, which only ever holds one tick. An event the publisher
//! could not keep up with, or the client could not queue, is lost and
//! logged, and the gap in `seq` tells subscribers.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, info, warn};

use crate::events;
use crate::map::Class;
use crate::sim::{SimHandle, TickEvents};

const TOPIC_PREFIX: &str = "bess/gw01/";

/// Requests the client may queue before publishing fails. A wall second at
/// full acceleration publishes every cadence point at once, and events need
/// room beside them.
const CLIENT_QUEUE: usize = 1024;

fn class_index(class: Class) -> Option<usize> {
    match class {
        Class::Fast => Some(0),
        Class::Medium => Some(1),
        Class::Slow => Some(2),
        Class::Event => None,
    }
}

/// Whether a cadence class last published at `last` is due again at `now`.
///
/// "Never published" is `None` rather than a sentinel timestamp: the
/// `i64::MIN` this replaced overflowed on the first subtraction, which
/// panicked in debug builds and, wrapping in release, kept every cadence
/// point from ever being published.
fn due(last: Option<i64>, now: i64, period: i64) -> bool {
    last.is_none_or(|t| now - t >= period)
}

/// Topic of a signal-map point: its dotted name as a path.
fn point_topic(name: &str) -> String {
    format!("{TOPIC_PREFIX}{}", name.replace('.', "/"))
}

/// Payload of a signal-map point.
fn point_payload(ts: i64, value: f64, unit: &str) -> String {
    format!("{{\"ts\":{ts},\"value\":{value},\"unit\":\"{unit}\"}}")
}

/// What the publisher remembers between wakes.
struct Publisher {
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
}

impl Publisher {
    fn new(client: AsyncClient, handle: SimHandle, connections: Arc<AtomicU64>) -> Self {
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
        }
    }

    /// The cadence points whose class period has elapsed in simulation time.
    fn publish_cadence(&mut self) {
        let snap = self.handle.snapshot.borrow().clone();
        let sim_time_s = snap.state.unix_time_s();
        for (point, topic) in self.handle.points.iter().zip(&self.topics) {
            let (Some(idx), Some(period)) = (class_index(point.class), point.class.period_s())
            else {
                continue;
            };
            if !due(self.last_pub_s[idx], sim_time_s, period) {
                continue;
            }
            let value = (point.extract)(&snap.state);
            let payload = point_payload(sim_time_s, value, point.unit);
            if let Err(err) = self
                .client
                .try_publish(topic, QoS::AtMostOnce, false, payload)
            {
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
    /// published, retained.
    fn publish_changed(&mut self) {
        // A new connection may be to a broker that lost its retained
        // store: forget what was published, so every word goes out again.
        let connection = self.connections.load(Ordering::Relaxed);
        if connection != self.published_on {
            self.last_event_value.fill(None);
            self.published_on = connection;
        }
        let snap = self.handle.snapshot.borrow().clone();
        let sim_time_s = snap.state.unix_time_s();
        for (i, point) in self.handle.points.iter().enumerate() {
            if point.class != Class::Event {
                continue;
            }
            let value = (point.extract)(&snap.state);
            if self.last_event_value[i] == Some(value) {
                continue;
            }
            let payload = point_payload(sim_time_s, value, point.unit);
            match self
                .client
                .try_publish(&self.topics[i], QoS::AtLeastOnce, true, payload)
            {
                Ok(()) => self.last_event_value[i] = Some(value),
                // Left unrecorded, so the next wake tries again.
                Err(err) => debug!("mqtt: publish skipped: {err}"),
            }
        }
    }

    /// Every event of one tick, in log order.
    fn publish_events(&mut self, batch: &TickEvents) {
        for (seq, event) in batch.numbered() {
            let topic = events::topic(TOPIC_PREFIX, event);
            let payload = events::payload(event, seq, batch.unix_time_s).to_string();
            if let Err(err) = self
                .client
                .try_publish(topic, QoS::AtLeastOnce, false, payload)
            {
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

/// Run the MQTT publisher until the task is aborted. Connection loss is
/// tolerated: rumqttc reconnects and publishing resumes.
pub async fn publish(host: String, port: u16, handle: SimHandle) {
    let mut options = MqttOptions::new("bess-emulator-gw01", host.clone(), port);
    options.set_keep_alive(Duration::from_secs(15));
    let (client, mut event_loop) = AsyncClient::new(options, CLIENT_QUEUE);
    info!("mqtt: publishing to {host}:{port} under {TOPIC_PREFIX}");

    let connections = Arc::new(AtomicU64::new(0));
    let accepted = Arc::clone(&connections);
    tokio::spawn(async move {
        let mut errors = 0u32;
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    errors = 0;
                    accepted.fetch_add(1, Ordering::Relaxed);
                }
                Ok(_) => errors = 0,
                Err(err) => {
                    errors += 1;
                    if errors == 1 || errors.is_multiple_of(30) {
                        warn!("mqtt: connection problem ({errors}): {err}");
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });

    let mut events_rx = handle.events.subscribe();
    let mut publisher = Publisher::new(client, handle, connections);
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                publisher.publish_cadence();
                publisher.publish_changed();
            }
            batch = events_rx.recv() => match batch {
                Ok(batch) => {
                    publisher.publish_events(&batch);
                    publisher.publish_changed();
                }
                Err(RecvError::Lagged(lost)) => {
                    warn!("mqtt: fell behind the plant, {lost} ticks of events lost");
                }
                Err(RecvError::Closed) => return,
            },
        }
    }
}

#[cfg(test)]
mod tests;
