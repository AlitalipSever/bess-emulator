//! The reset endpoint over a real socket, against a plant with a rack that
//! is still too hot to come back.

use std::net::SocketAddr;
use std::time::Duration;

use bess_core::alarms::layout::rack::OVER_TEMP_TRIP;
use bess_core::alarms::AlarmNode;
use bess_core::kernel::Event;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::{fixtures, http, sim};

/// POST a JSON body and return the status and the parsed answer.
async fn post(addr: SocketAddr, path: &str, body: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
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

/// A reset reaches the kernel, says which bits it could not clear, and the
/// clear goes out as an event with the next tick, followed by the raise
/// the cause still demands. A node the site does not have is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_reset_answers_with_what_stayed_and_refuses_unknown_nodes() {
    let addr: SocketAddr = "127.0.0.1:18081".parse().unwrap();
    let (handle, _sim) = sim::start(fixtures::tripped_plant(), 1.0);
    let mut events = handle.events.subscribe();
    tokio::spawn(http::serve(addr, handle));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let hot = json!({
        "node": "block01/container0/rack04",
        "alarm": "rack.over_temp_trip",
        "bit": 8,
    });
    let (status, body) = post(
        addr,
        "/api/v1/alarms/reset",
        r#"{"scope": "rack", "block": 1, "container": 0, "rack": 4}"#,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["node"], "block01/container0/rack04");
    assert_eq!(body["still_present"], json!([hot]));

    // The clear, then the raise again, in that order.
    let node = AlarmNode::Rack {
        block: 1,
        container: 0,
        rack: 4,
    };
    let is_trip = |n: AlarmNode, bit: u8| n == node && bit == OVER_TEMP_TRIP;
    let mut cleared = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    'wait: loop {
        let batch = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("the reset's events never went out")
            .unwrap();
        for event in &batch.events {
            match *event {
                Event::AlarmCleared { node: n, bit, .. } if is_trip(n, bit) => cleared = true,
                // A raise before the clear is the first tick's, which this
                // receiver may or may not have caught.
                Event::AlarmRaised { node: n, bit, .. } if is_trip(n, bit) && cleared => {
                    break 'wait;
                }
                _ => {}
            }
        }
    }

    // The site scope reaches the same rack.
    let (status, body) = post(addr, "/api/v1/alarms/reset", r#"{"scope": "site"}"#).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["still_present"], json!([hot]));

    // Nodes the site does not have, and a scope that is not one.
    for (request, path) in [
        (r#"{"scope": "block", "block": 25}"#, "block25"),
        (
            r#"{"scope": "rack", "block": 1, "container": 5, "rack": 0}"#,
            "block01/container5/rack00",
        ),
    ] {
        let (status, body) = post(addr, "/api/v1/alarms/reset", request).await;
        assert_eq!(status, 422, "{request}");
        assert_eq!(
            body["error"],
            format!("no such node on this site: {path}"),
            "{request}"
        );
    }

    // Bodies the endpoint does not fully understand are refused, never read
    // as the nearest scope: a rack reset sent with the block tag would
    // otherwise clear the whole block, and site with a stray field the site.
    for request in [
        r#"{"scope": "block", "block": 1, "container": 0, "rack": 4}"#,
        r#"{"scope": "site", "block": 2}"#,
        r#"{"scope": "plant"}"#,
        r#"{"scope": "block"}"#,
        r#"{"scope": "block", "block": -1}"#,
        r#"{"scope": "site""#,
    ] {
        let (status, body) = post(addr, "/api/v1/alarms/reset", request).await;
        assert_eq!(status, 400, "{request}");
        assert!(body["error"].is_string(), "{request}: {body}");
    }
}
