//! The scenario endpoints over a real socket: a load starts the plant over
//! and narrates the story, the status follows the schedule, the four
//! refusals carry their codes, and health names the file.

use std::net::SocketAddr;
use std::time::Duration;

use bess_core::kernel::{Event, FaultAction};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::{fixtures, http, sim};

/// A failure on the first tick, its repair an hour later, a second failure
/// two seconds in and a data fault at five, listed out of time order.
const STORY: &str = "schema: 1
name: hot-start
seed: 7
start: 2026-07-14T14:00:00Z
speed: 600
events:
  - at: PT0S
    target: block[2].container[0].hvac
    fault: failure
  - at: PT1H
    target: block[2].container[0].hvac
    fault: repair
  - at: PT2S
    target: block[2].container[1].hvac
    fault: failure
  - at: PT5S
    surface: mqtt
    fault: dropout
    duration: PT1M
";

/// One request; the status and the parsed answer.
async fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    content_type: &str,
    body: &str,
) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {content_type}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

async fn get(addr: SocketAddr, path: &str) -> (u16, Value) {
    request(addr, "GET", path, "text/plain", "").await
}

async fn post_yaml(addr: SocketAddr, body: &str) -> (u16, Value) {
    request(addr, "POST", "/api/v1/scenario", "application/yaml", body).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_load_starts_the_plant_over_and_the_status_follows_the_schedule() {
    let addr: SocketAddr = "127.0.0.1:18082".parse().unwrap();
    let (handle, sim_task) = sim::start(fixtures::tripped_plant(), 1.0, None);
    let mut events = handle.events.subscribe();
    tokio::spawn(http::serve(addr, handle));
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Nothing loaded: the status says so, and health carries null.
    let (status, body) = get(addr, "/api/v1/scenario").await;
    assert_eq!((status, body), (200, json!({"loaded": false})));
    let (_, health) = get(addr, "/health").await;
    assert!(health["scenario"].is_null(), "{health}");

    // The load answers the status at tick 0: nothing fired yet, the
    // schedule in firing order, the file's parameters.
    let (status, body) = post_yaml(addr, STORY).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["loaded"], true);
    assert_eq!(body["name"], "hot-start");
    assert_eq!(body["seed"], 7);
    assert_eq!(body["speed"], 600.0);
    assert_eq!(body["tick"], 0);
    assert_eq!((&body["fired"], &body["pending"]), (&json!(0), &json!(4)));
    let order: Vec<u64> = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["index"].as_u64().unwrap())
        .collect();
    assert_eq!(order, [0, 2, 3, 1]);
    assert_eq!(body["next"]["index"], 0);
    let hash = body["content_hash"].as_str().unwrap().to_owned();

    // The log starts over at 1, the first mark ahead of its consequences.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let first = loop {
        let batch = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("the first tick's events never came")
            .unwrap();
        if batch
            .events
            .iter()
            .any(|e| matches!(e, Event::FaultApplied(_)))
        {
            break batch;
        }
    };
    assert_eq!(first.first_seq, 1);
    let failure = FaultAction::HvacFailure {
        block: 2,
        container: 0,
    };
    assert_eq!(first.events[0], Event::FaultApplied(failure));

    // Health names the file, and the status follows the schedule: three
    // fired within seconds, the repair pending.
    let (_, health) = get(addr, "/health").await;
    assert_eq!(
        health["scenario"],
        json!({"name": "hot-start", "content_hash": hash})
    );
    let body = loop {
        let (_, body) = get(addr, "/api/v1/scenario").await;
        if body["fired"] == 3 {
            break body;
        }
        assert!(tokio::time::Instant::now() < deadline, "{body}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(body["pending"], 1);
    assert_eq!(body["next"]["index"], 1);
    assert_eq!(body["next"]["tick"], 3600);
    assert_eq!(body["events"][2]["fired"], true);
    assert_eq!(body["events"][3]["fired"], false);
    assert!(body["tick"].as_u64().unwrap() >= 5, "{body}");

    // Loading again starts over again.
    let (status, body) = post_yaml(addr, STORY).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!((&body["tick"], &body["fired"]), (&json!(0), &json!(0)));

    // The refusals: not YAML, a file that does not validate, a well-formed
    // file naming a node this site lacks.
    let (status, body) = request(addr, "POST", "/api/v1/scenario", "application/json", "{}").await;
    assert_eq!(status, 415, "{body}");
    let (status, body) = post_yaml(addr, "schema: 2\nname: x\nevents: []\n").await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["position"], "line 1, column 9");
    assert_eq!(
        body["error"],
        "line 1, column 9: schema 2 is not supported; this engine reads schema 1"
    );
    let (status, body) = post_yaml(
        addr,
        "schema: 1\nname: x\nevents:\n  - at: PT1H\n    target: block[25].pcs\n    fault: trip\n",
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["position"], "events[0].target (line 4)");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .ends_with("block[25]: this site has block[0] to block[19]"),
        "{body}"
    );

    // The task gone: 503.
    sim_task.abort();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (status, body) = post_yaml(addr, STORY).await;
    assert_eq!(status, 503, "{body}");
}
