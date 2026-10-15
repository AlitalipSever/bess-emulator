//! The MQTT tap: what a lying device publishes (phase 5 design D1).
//!
//! Every message the publisher sends passes through the outbox. During a
//! dropout the device is offline: nothing reaches the broker, and the
//! messages are kept in order, the way a device with a store buffers what
//! it could not send. A backfill burst, once its instant has passed,
//! replays that store ahead of anything live, each message with the
//! timestamp and the sequence number it was born with: old device times
//! arriving in a burst under monotone receive times, the pipeline trap the
//! fault exists to teach. Without a burst the store is kept until the next
//! dropout starts over.
//!
//! The values and the clock are the shared tap's: a frozen point repeats
//! what it read when the freeze began, a unit error scales it, a NaN burst
//! writes `NaN` (the literal a device's printf emits, which is not JSON,
//! which is the point), a drifting clock stamps every message with the
//! device's time, and a chatter flaps the alarm words.

use std::collections::VecDeque;

use bess_scenario::Surface;
use rumqttc::{AsyncClient, ClientError, QoS};
use tracing::{info, warn};

use crate::tap::Taps;

/// Messages a dropout keeps before it starts forgetting the oldest. A wall
/// second publishes every cadence point at most once, so this is minutes
/// of a dropout at any speed.
const STORE: usize = 65_536;

/// Stored messages a replay hands the client per wake, so a burst does not
/// overflow the client's queue and arrives over a few seconds instead.
const BURST: usize = 512;

/// One message as the publisher built it.
pub struct Message {
    /// Full topic.
    pub topic: String,
    /// The payload text.
    pub payload: String,
    /// Delivery guarantee.
    pub qos: QoS,
    /// Whether the broker keeps it for late subscribers.
    pub retain: bool,
}

/// The publisher's door to the wire.
#[derive(Default)]
pub struct Outbox {
    store: VecDeque<Message>,
    /// Whether the last message found the device offline, to log the edges.
    offline: bool,
    /// Backfill bursts begun, by event index.
    replayed: Vec<usize>,
    /// Whether a burst is still draining the store.
    replaying: bool,
    /// Messages the full store forgot during the current dropout.
    forgotten: u64,
}

impl Outbox {
    /// Publish `msg`, or keep it when the surface is in a dropout at `tick`.
    pub fn send(
        &mut self,
        client: &AsyncClient,
        taps: &Taps,
        tick: u64,
        msg: Message,
    ) -> Result<(), ClientError> {
        let offline = taps.dropout(Surface::Mqtt, tick);
        if offline != self.offline {
            self.offline = offline;
            if offline {
                info!("mqtt: dropout begins at tick {tick}; publishing stops, messages are stored");
                self.store.clear();
                self.forgotten = 0;
            } else {
                info!(
                    "mqtt: dropout ends at tick {tick} with {} messages stored",
                    self.store.len()
                );
            }
        }
        if offline {
            if self.store.len() == STORE {
                self.store.pop_front();
                self.forgotten += 1;
            }
            self.store.push_back(msg);
            return Ok(());
        }
        client.try_publish(msg.topic, msg.qos, msg.retain, msg.payload)
    }

    /// Replay the store when a backfill burst's instant has passed, a
    /// slice of it per call; nothing while a dropout is open.
    pub fn backfill(&mut self, client: &AsyncClient, taps: &Taps, tick: u64) {
        if taps.dropout(Surface::Mqtt, tick) {
            return;
        }
        if !self.replaying {
            let due = taps
                .backfills_due(Surface::Mqtt, tick)
                .find(|burst| !self.replayed.contains(&burst.index));
            let Some(burst) = due else {
                return;
            };
            self.replayed.push(burst.index);
            self.replaying = true;
            info!(
                "mqtt: backfill burst at tick {tick} replays {} stored messages ({} forgotten)",
                self.store.len(),
                self.forgotten
            );
        }
        for _ in 0..BURST {
            let Some(msg) = self.store.pop_front() else {
                self.replaying = false;
                return;
            };
            if let Err(err) = client.try_publish(msg.topic, msg.qos, msg.retain, msg.payload) {
                warn!("mqtt: backfill paused: {err}");
                return;
            }
        }
    }
}
