//! The scenario over REST: `POST /api/v1/scenario` starts the plant over
//! under a file, `GET /api/v1/scenario` narrates the schedule (design D6,
//! D7).
//!
//! The body of a `POST` is the YAML text itself, `application/yaml` or
//! `text/yaml`: a JSON wrapper would mean escaping a document inside a
//! string, and `curl --data-binary @file` is the whole point. The file's
//! values apply; what it leaves out keeps the plant's current seed, start
//! and speed, since over REST there is no flag to disagree with (D5).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use bess_core::PlantConfig;
use bess_scenario::{Reason, RunParams};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::sim::{Command, SimHandle};

/// The media types a scenario file may arrive as.
const YAML_TYPES: [&str; 2] = ["application/yaml", "text/yaml"];

fn unavailable() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "simulation task unavailable"})),
    )
}

/// The request's media type without its parameters, lowercased.
fn media_type(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let bare = value.split(';').next().unwrap_or("").trim();
    Some(bare.to_ascii_lowercase())
}

/// The status with the tick it describes.
fn with_tick(status: &Value, tick: u64) -> Value {
    let mut status = status.clone();
    status["tick"] = json!(tick);
    status
}

pub(super) async fn load(
    State(handle): State<SimHandle>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !media_type(&headers).is_some_and(|t| YAML_TYPES.contains(&t.as_str())) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error": "send the scenario file as application/yaml or text/yaml"})),
        );
    }
    let scenario = match bess_scenario::load(&body, &PlantConfig::gw01()) {
        Ok(scenario) => scenario,
        // A file that does not parse or validate is a 400; a well-formed
        // file naming a node this site does not have is a 422, the split
        // the reset endpoint makes: the body is the problem, not the
        // endpoint. Both carry the position an editor can jump to.
        Err(err) => {
            let status = if matches!(err.reason, Reason::BadTarget(_)) {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::BAD_REQUEST
            };
            let answer = json!({
                "error": err.to_string(),
                "position": err.position.to_string(),
            });
            return (status, Json(answer));
        }
    };
    let (seed, start_unix_s, speed) = {
        let snap = handle.snapshot.borrow();
        (
            snap.state.meta.seed,
            snap.state.meta.start_unix_s,
            snap.speed,
        )
    };
    let params = RunParams {
        seed: scenario.seed.unwrap_or(seed),
        start_unix_s: scenario.start_unix_s.unwrap_or(start_unix_s),
        speed: scenario.speed.unwrap_or(speed),
    };
    let resolved = Arc::new(scenario.resolve(params));
    let (reply, answer) = oneshot::channel();
    let command = Command::LoadScenario { resolved, reply };
    if handle.commands.send(command).await.is_err() {
        return unavailable();
    }
    match answer.await {
        Ok(status) => (StatusCode::OK, Json(with_tick(&status, 0))),
        Err(_) => unavailable(),
    }
}

pub(super) async fn status(State(handle): State<SimHandle>) -> impl IntoResponse {
    let snap = handle.snapshot.borrow().clone();
    Json(match &snap.scenario {
        Some(status) => with_tick(status, snap.state.tick),
        None => json!({"loaded": false}),
    })
}

#[cfg(test)]
mod tests;
