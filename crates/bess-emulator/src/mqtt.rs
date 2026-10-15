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
//!
//! Everything goes through the tap (`mqtt/tap.rs`): a scenario's data
//! faults make this publisher lie the way a device lies, while the kernel
//! underneath stays right.

mod publisher;
mod tap;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet};
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

use crate::sim::SimHandle;
use publisher::Publisher;

const TOPIC_PREFIX: &str = "bess/gw01/";

/// Requests the client may queue before publishing fails. A wall second at
/// full acceleration publishes every cadence point at once, and events need
/// room beside them.
const CLIENT_QUEUE: usize = 1024;

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

/// Payload of a signal-map point. A NaN prints as `NaN`, which is not
/// JSON: that is what a device's printf emits, and what a NaN burst is.
fn point_payload(ts: i64, value: f64, unit: &str) -> String {
    format!("{{\"ts\":{ts},\"value\":{value},\"unit\":\"{unit}\"}}")
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
                publisher.backfill();
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
