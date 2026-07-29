//! MQTT publisher: every signal-map point becomes a topic under
//! `bess/gw01/`, decimated per publication class in simulation time.
//!
//! At 1x speed the cadence matches the class table (fast 1 s, medium 10 s,
//! slow 60 s). When accelerated, publications collapse to at most one batch
//! per wall second carrying the latest values, which mirrors how a real
//! historian would sample a faster-than-life data source.

use std::time::Duration;

use rumqttc::{AsyncClient, MqttOptions, QoS};
use tracing::{debug, info, warn};

use crate::map::Class;
use crate::sim::SimHandle;

const TOPIC_PREFIX: &str = "bess/gw01/";

fn class_index(class: Class) -> usize {
    match class {
        Class::Fast => 0,
        Class::Medium => 1,
        Class::Slow => 2,
    }
}

/// Run the MQTT publisher until the task is aborted. Connection loss is
/// tolerated: rumqttc reconnects and publishing resumes.
pub async fn publish(host: String, port: u16, handle: SimHandle) {
    let mut options = MqttOptions::new("bess-emulator-gw01", host.clone(), port);
    options.set_keep_alive(Duration::from_secs(15));
    let (client, mut event_loop) = AsyncClient::new(options, 256);
    info!("mqtt: publishing to {host}:{port} under {TOPIC_PREFIX}");

    tokio::spawn(async move {
        let mut errors = 0u32;
        loop {
            match event_loop.poll().await {
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

    // Simulation timestamp of the last publication per class.
    let mut last_pub_s = [i64::MIN; 3];
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    let mut topic = String::with_capacity(64);
    loop {
        ticker.tick().await;
        let snap = handle.snapshot.borrow().clone();
        let sim_time_s = snap.state.unix_time_s();

        for point in handle.points.iter() {
            let idx = class_index(point.class);
            if sim_time_s - last_pub_s[idx] < point.class.period_s() {
                continue;
            }
            let value = (point.extract)(&snap.state);
            topic.clear();
            topic.push_str(TOPIC_PREFIX);
            for part in point.name.split('.') {
                topic.push_str(part);
                topic.push('/');
            }
            topic.pop();
            let payload = format!(
                "{{\"ts\":{sim_time_s},\"value\":{value},\"unit\":\"{}\"}}",
                point.unit
            );
            if let Err(err) = client.try_publish(&topic, QoS::AtMostOnce, false, payload) {
                // Broker down or queue full: drop this batch, the event
                // loop reconnects on its own.
                debug!("mqtt: publish skipped: {err}");
                break;
            }
        }
        for (last, period) in last_pub_s.iter_mut().zip([1i64, 10, 60]) {
            if sim_time_s - *last >= period {
                *last = sim_time_s;
            }
        }
    }
}
