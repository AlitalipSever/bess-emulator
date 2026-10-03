//! HTTP surface: health, Prometheus metrics, REST state and control, the
//! operator alarm reset, and a WebSocket stream of tick summaries and events.

mod alarms;
mod metrics;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast::error::TryRecvError;
use tracing::{error, info};

use crate::events;
use crate::sim::{Command, SimHandle, Snapshot, TickEvents, MAX_SPEED};

/// Run the HTTP server until the task is aborted.
pub async fn serve(addr: SocketAddr, handle: SimHandle) {
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/v1/state", get(full_state))
        .route("/api/v1/summary", get(summary))
        .route("/api/v1/setpoint", post(set_setpoint))
        .route("/api/v1/speed", post(set_speed))
        .route("/api/v1/alarms/reset", post(alarms::reset))
        .route("/api/v1/stream", get(stream))
        .with_state(handle);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            error!("http: cannot bind {addr}: {err}");
            return;
        }
    };
    info!("http: listening on {addr}");
    if let Err(err) = axum::serve(listener, app).await {
        error!("http: server stopped: {err}");
    }
}

fn summary_value(snap: &Snapshot) -> serde_json::Value {
    let state = &snap.state;
    json!({
        "site": state.meta.site_id,
        "tick": state.tick,
        "unix_time_s": state.unix_time_s(),
        "speed": snap.speed,
        "soc": state.average_soc(),
        "poi_active_power_w": state.substation.poi_active_power_w,
        "site_setpoint_w": state.ems.site_setpoint_w,
        "frequency_hz": state.substation.frequency_hz,
        "ambient_c": state.weather.ambient_c,
        "import_kwh": state.substation.import_wh / 1000.0,
        "export_kwh": state.substation.export_wh / 1000.0,
        "available_discharge_w": state.ems.available_discharge_w,
        "available_charge_w": state.ems.available_charge_w,
        "alarm_bits": state.alarm_bits,
        "event_count": state.event_log.count,
        "blocks": state.blocks.iter().map(|b| json!({
            "p_ac_w": b.pcs.p_ac_w,
            "soc": b.average_soc(),
            "alarm_bits": b.alarm_bits,
        })).collect::<Vec<_>>(),
    })
}

/// One WebSocket message: the latest summary, plus every event since the
/// previous message, and how many ticks of events were lost if the stream
/// fell further behind than the event channel holds.
fn stream_message(snap: &Snapshot, batches: &[Arc<TickEvents>], lost: u64) -> Value {
    let mut message = summary_value(snap);
    message["events"] = batches
        .iter()
        .flat_map(|batch| {
            batch
                .numbered()
                .map(|(seq, event)| events::payload(event, seq, batch.unix_time_s))
        })
        .collect();
    if lost > 0 {
        message["events_lost_ticks"] = json!(lost);
    }
    message
}

async fn health(State(handle): State<SimHandle>) -> impl IntoResponse {
    let snap = handle.snapshot.borrow().clone();
    Json(json!({
        "status": "ok",
        "site": snap.state.meta.site_id,
        "tick": snap.state.tick,
        "kernel_version": bess_core::version(),
        // Which version of the register contract this process serves, so a
        // client can check it without fetching the published CSV.
        "signal_map_version": crate::map::MAP_VERSION,
    }))
}

async fn metrics(State(handle): State<SimHandle>) -> impl IntoResponse {
    let snap = handle.snapshot.borrow().clone();
    let body = metrics::metrics_body(&snap);
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}

async fn full_state(State(handle): State<SimHandle>) -> impl IntoResponse {
    let snap = handle.snapshot.borrow().clone();
    Json(snap.state.clone())
}

async fn summary(State(handle): State<SimHandle>) -> impl IntoResponse {
    let snap = handle.snapshot.borrow().clone();
    Json(summary_value(&snap))
}

/// Body of `POST /api/v1/setpoint`: either `{"watts": -50000000}` to write
/// an external setpoint (positive = discharge) or `{"mode": "plan"}` to
/// return control to the internal dispatch plan.
#[derive(Debug, Deserialize)]
struct SetpointRequest {
    watts: Option<f64>,
    mode: Option<String>,
}

async fn set_setpoint(
    State(handle): State<SimHandle>,
    Json(req): Json<SetpointRequest>,
) -> impl IntoResponse {
    let command = match (&req.mode, req.watts) {
        (Some(mode), _) if mode == "plan" => Command::SetSetpointW(None),
        (None, Some(watts)) if watts.is_finite() => Command::SetSetpointW(Some(watts)),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "expected {\"watts\": <number>} or {\"mode\": \"plan\"}"})),
            );
        }
    };
    match handle.commands.send(command).await {
        Ok(()) => (StatusCode::ACCEPTED, Json(json!({"accepted": true}))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "simulation task unavailable"})),
        ),
    }
}

/// Body of `POST /api/v1/speed`: `{"factor": 60}`.
#[derive(Debug, Deserialize)]
struct SpeedRequest {
    factor: f64,
}

async fn set_speed(
    State(handle): State<SimHandle>,
    Json(req): Json<SpeedRequest>,
) -> impl IntoResponse {
    if !req.factor.is_finite() || !(1.0..=MAX_SPEED).contains(&req.factor) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("factor must be in [1, {MAX_SPEED}]")})),
        );
    }
    match handle.commands.send(Command::SetSpeed(req.factor)).await {
        Ok(()) => (StatusCode::ACCEPTED, Json(json!({"accepted": true}))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "simulation task unavailable"})),
        ),
    }
}

async fn stream(ws: WebSocketUpgrade, State(handle): State<SimHandle>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| push_summaries(socket, handle))
}

/// Push the latest tick summary four times per wall second, with every
/// event emitted since the previous push.
async fn push_summaries(mut socket: WebSocket, handle: SimHandle) {
    let mut events_rx = handle.events.subscribe();
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    loop {
        ticker.tick().await;
        let mut batches = Vec::new();
        let mut lost = 0;
        loop {
            match events_rx.try_recv() {
                Ok(batch) => batches.push(batch),
                Err(TryRecvError::Lagged(n)) => lost += n,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
        let snap: Arc<Snapshot> = handle.snapshot.borrow().clone();
        let text = stream_message(&snap, &batches, lost).to_string();
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
