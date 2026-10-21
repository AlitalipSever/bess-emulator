//! The control surface over REST (M4 phase 2): the external limit, the
//! reactive settings and the switchgear, the same commands the holding
//! bank carries, with the refusal's reason in the body.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use bess_core::kernel::{CommandError, ControlCommand};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::oneshot;

use crate::sim::{Command, SimHandle};

/// Body of `POST /api/v1/limit`: `{"pct": 60, "revert_s": 1800}` writes a
/// limit (the revert time optional), `{"clear": true}` lifts it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LimitRequest {
    pct: Option<f64>,
    revert_s: Option<f64>,
    clear: Option<bool>,
}

/// Body of `POST /api/v1/reactive`: `{"mode": "fixed_cos_phi", "cos_phi":
/// 0.95, "under_excited": true}`, `{"mode": "fixed_q", "kvar": 20000}` or
/// `{"mode": "q_of_u"}`; a value a mode does not read is kept for later.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReactiveRequest {
    mode: Option<String>,
    cos_phi: Option<f64>,
    under_excited: Option<bool>,
    kvar: Option<f64>,
}

/// Body of `POST /api/v1/substation/{breaker,disconnectors,earthing}`:
/// `{"closed": true}`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SwitchRequest {
    closed: bool,
}

fn refused(err: &CommandError) -> (StatusCode, Json<serde_json::Value>) {
    let status = match err {
        CommandError::Interlocked(_) => StatusCode::CONFLICT,
        CommandError::OutOfRange(_) => StatusCode::BAD_REQUEST,
        CommandError::NoSuchNode(_) | CommandError::NoSuchContainer { .. } => StatusCode::NOT_FOUND,
    };
    (status, Json(json!({"error": err.to_string()})))
}

async fn answer<T: serde::Serialize>(
    handle: &SimHandle,
    command: Command,
    rx: oneshot::Receiver<Result<T, CommandError>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if handle.commands.send(command).await.is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "simulation task unavailable"})),
        );
    }
    match rx.await {
        Ok(Ok(value)) => (
            StatusCode::ACCEPTED,
            Json(json!({"accepted": true, "changed": value})),
        ),
        Ok(Err(err)) => refused(&err),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "simulation task unavailable"})),
        ),
    }
}

pub(super) async fn limit(
    State(handle): State<SimHandle>,
    Json(req): Json<LimitRequest>,
) -> impl IntoResponse {
    let (tx, rx) = oneshot::channel();
    let command = if req.clear == Some(true) {
        Command::SetLimit {
            pct: None,
            enable: Some(false),
            revert_s: None,
            reply: Some(tx),
        }
    } else if req.pct.is_some() {
        Command::SetLimit {
            pct: req.pct,
            enable: Some(true),
            revert_s: req.revert_s,
            reply: Some(tx),
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": "expected {\"pct\": <0..100>, \"revert_s\"?: <seconds>} or {\"clear\": true}"}),
            ),
        );
    };
    answer(&handle, command, rx).await
}

pub(super) async fn reactive(
    State(handle): State<SimHandle>,
    Json(req): Json<ReactiveRequest>,
) -> impl IntoResponse {
    let mode = match req.mode.as_deref() {
        None => None,
        Some("fixed_cos_phi") => Some(0),
        Some("fixed_q") => Some(1),
        Some("q_of_u") => Some(2),
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "mode is fixed_cos_phi, fixed_q or q_of_u"})),
            )
        }
    };
    let (tx, rx) = oneshot::channel();
    let command = Command::SetReactive {
        mode,
        q_var: req.kvar.map(|k| k * 1000.0),
        cos_phi: req.cos_phi,
        under_excited: req.under_excited,
        reply: Some(tx),
    };
    answer(&handle, command, rx).await
}

pub(super) async fn switchgear(
    State(handle): State<SimHandle>,
    Path(switch): Path<String>,
    Json(req): Json<SwitchRequest>,
) -> impl IntoResponse {
    let command = match (switch.as_str(), req.closed) {
        ("breaker", true) => ControlCommand::BreakerClose,
        ("breaker", false) => ControlCommand::BreakerOpen,
        ("disconnectors", true) => ControlCommand::DisconnectorsClose,
        ("disconnectors", false) => ControlCommand::DisconnectorsOpen,
        ("earthing", true) => ControlCommand::EarthingClose,
        ("earthing", false) => ControlCommand::EarthingOpen,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "the substation has breaker, disconnectors and earthing"})),
            )
        }
    };
    let (tx, rx) = oneshot::channel();
    answer(
        &handle,
        Command::Control {
            command,
            reply: Some(tx),
        },
        rx,
    )
    .await
}
