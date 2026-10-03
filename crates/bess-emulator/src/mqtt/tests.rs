//! The publisher against a broker over real TCP: the chain plant raises an
//! alarm, an operator action clears it, and both messages arrive with the
//! documented payload.
//!
//! The broker is a minimal MQTT 3.1.1 server written for the test: it
//! accepts the client, acknowledges what needs acknowledging, and hands every
//! PUBLISH to the test. Enough protocol to be the other end of the wire, and
//! no new dependency for it.

use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{timeout_at, Instant};

use super::{due, publish};
use crate::fixtures;
use crate::sim::{self, Command};

/// One PUBLISH as the broker received it.
#[derive(Debug)]
struct Received {
    /// Which connection it came on, counting from 0.
    conn: usize,
    topic: String,
    payload: String,
    retain: bool,
    qos: u8,
}

/// Read one control packet: its first byte and its body.
async fn read_packet(stream: &mut TcpStream) -> std::io::Result<(u8, Vec<u8>)> {
    let header = stream.read_u8().await?;
    let mut len = 0usize;
    let mut shift = 0;
    loop {
        let byte = stream.read_u8().await?;
        len |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    let mut body = vec![0; len];
    stream.read_exact(&mut body).await?;
    Ok((header, body))
}

/// Serve one client until it disconnects, or until the first connection
/// publishes `drop_first_on`, when the broker hangs up on it as a broker
/// restart would.
async fn serve_client(
    mut stream: TcpStream,
    conn: usize,
    drop_first_on: Option<&'static str>,
    out: mpsc::UnboundedSender<Received>,
) -> std::io::Result<()> {
    loop {
        let (header, body) = read_packet(&mut stream).await?;
        match header >> 4 {
            // CONNECT: accept.
            1 => stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await?,
            // PUBLISH: hand it over, acknowledge QoS 1.
            3 => {
                let qos = (header >> 1) & 0x03;
                let topic_len = usize::from(u16::from_be_bytes([body[0], body[1]]));
                let topic = String::from_utf8_lossy(&body[2..2 + topic_len]).into_owned();
                let mut rest = 2 + topic_len;
                if qos > 0 {
                    stream
                        .write_all(&[0x40, 0x02, body[rest], body[rest + 1]])
                        .await?;
                    rest += 2;
                }
                let hang_up = conn == 0 && drop_first_on == Some(topic.as_str());
                let _ = out.send(Received {
                    conn,
                    topic,
                    payload: String::from_utf8_lossy(&body[rest..]).into_owned(),
                    retain: header & 0x01 == 1,
                    qos,
                });
                if hang_up {
                    return Ok(());
                }
            }
            // PINGREQ: answer.
            12 => stream.write_all(&[0xd0, 0x00]).await?,
            // DISCONNECT.
            14 => return Ok(()),
            _ => {}
        }
    }
}

/// Start the broker on a free port.
async fn broker(drop_first_on: Option<&'static str>) -> (u16, mpsc::UnboundedReceiver<Received>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut conn = 0;
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(serve_client(stream, conn, drop_first_on, tx.clone()));
            conn += 1;
        }
    });
    (port, rx)
}

/// Wait for the first message on `topic` whose payload says `event`.
async fn wait_for(
    rx: &mut mpsc::UnboundedReceiver<Received>,
    topic: &str,
    event: &str,
    deadline: Instant,
) -> (Value, Received) {
    loop {
        let msg = timeout_at(deadline, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("no {event} on {topic} before the deadline"))
            .expect("broker stopped");
        if msg.topic != topic {
            continue;
        }
        let payload: Value = serde_json::from_str(&msg.payload).expect("payload is JSON");
        if payload["event"] == event {
            return (payload, msg);
        }
    }
}

/// The phase's acceptance check on MQTT: the hot block misses its setpoint
/// minutes into the chain, the operator drops the setpoint to zero, and a
/// subscriber receives the raise and then the clear, each with exactly the
/// documented fields, while the alarm words follow as retained topics.
#[tokio::test(flavor = "multi_thread")]
async fn an_alarm_raise_and_clear_arrive_with_the_documented_payload() {
    let (port, mut rx) = broker(None).await;
    let (handle, _sim) = sim::start(fixtures::hot_plant(), sim::MAX_SPEED);
    let commands = handle.commands.clone();
    tokio::spawn(publish("127.0.0.1".into(), port, handle));
    let deadline = Instant::now() + Duration::from_secs(120);

    // Cadence telemetry flows: the fast tick counter arrives, unretained.
    loop {
        let msg = timeout_at(deadline, rx.recv())
            .await
            .expect("no cadence telemetry before the deadline")
            .expect("broker stopped");
        if msg.topic == "bess/gw01/site/sim/tick" {
            assert!(!msg.retain && msg.qos == 0);
            break;
        }
    }

    let topic = "bess/gw01/events/block02/setpoint_not_met";
    let (raised, msg) = wait_for(&mut rx, topic, "raised", deadline).await;
    assert_eq!(msg.qos, 1, "events go at least once");
    assert!(!msg.retain, "events are a log, not a state");

    commands
        .send(Command::SetSetpointW(Some(0.0)))
        .await
        .unwrap();
    let (cleared, _) = wait_for(&mut rx, topic, "cleared", deadline).await;

    for payload in [&raised, &cleared] {
        let fields = payload.as_object().unwrap();
        assert_eq!(fields.len(), 7, "documented fields only: {payload}");
        assert_eq!(payload["node"], "block02");
        assert_eq!(payload["alarm"], "block.setpoint_not_met");
        assert_eq!(payload["bit"], 0);
        assert_eq!(payload["severity"], "warning");
    }
    assert!(raised["seq"].as_u64().unwrap() < cleared["seq"].as_u64().unwrap());
    assert!(raised["ts"].as_i64().unwrap() < cleared["ts"].as_i64().unwrap());

    // The words are state, published retained whenever they change: the
    // block word the clear just touched shows up with the bit gone.
    let word = loop {
        let msg = timeout_at(deadline, rx.recv())
            .await
            .expect("no retained block word after the clear")
            .unwrap();
        if msg.topic == "bess/gw01/block02/block_alarm_bits" {
            assert!(msg.retain && msg.qos == 1);
            let payload: Value = serde_json::from_str(&msg.payload).unwrap();
            let bits = payload["value"].as_f64().unwrap() as u32;
            if bits & 1 == 0 {
                break bits;
            }
        }
    };
    assert_ne!(word & 0b100, 0, "the HVAC units are still down");
}

/// A broker that restarts without persistence has forgotten every retained
/// word. The publisher sends them all again on the new connection, so a
/// late subscriber still finds the current word, even one that has not
/// changed since it was first published.
#[tokio::test(flavor = "multi_thread")]
async fn retained_words_are_sent_again_after_a_reconnect() {
    let word = "bess/gw01/site/alarm_bits";
    let (port, mut rx) = broker(Some(word)).await;
    let (handle, _sim) = sim::start(fixtures::tripped_plant(), 1.0);
    tokio::spawn(publish("127.0.0.1".into(), port, handle));
    let deadline = Instant::now() + Duration::from_secs(30);

    let mut seen = Vec::new();
    while seen.len() < 2 {
        let msg = timeout_at(deadline, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("{word} seen on {seen:?} only"))
            .expect("broker stopped");
        if msg.topic == word {
            assert!(msg.retain);
            let payload: Value = serde_json::from_str(&msg.payload).unwrap();
            seen.push((msg.conn, payload["value"].as_f64().unwrap()));
        }
    }
    assert_eq!(seen[0].0, 0, "first on the first connection");
    assert_eq!(seen[1].0, 1, "again on the second");
    assert!(
        (seen[0].1 - seen[1].1).abs() < f64::EPSILON,
        "the word did not change, so only the reconnect can have sent it: {seen:?}"
    );
}

/// A class that never published is due at once, whatever the clock reads,
/// and then again only a period later.
#[test]
fn a_cadence_class_is_due_first_at_once_then_by_period() {
    assert!(due(None, 1_767_225_600, 60));
    assert!(!due(Some(1_767_225_600), 1_767_225_659, 60));
    assert!(due(Some(1_767_225_600), 1_767_225_660, 60));
}
