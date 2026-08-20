//! HTTP surface: health, Prometheus metrics, REST state and control, and a
//! WebSocket stream of tick summaries.

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use bess_core::state::HvacMode;
use bess_core::SiteState;
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info};

use crate::sim::{Command, SimHandle, Snapshot, MAX_SPEED};

/// Run the HTTP server until the task is aborted.
pub async fn serve(addr: SocketAddr, handle: SimHandle) {
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/v1/state", get(full_state))
        .route("/api/v1/summary", get(summary))
        .route("/api/v1/setpoint", post(set_setpoint))
        .route("/api/v1/speed", post(set_speed))
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
        "blocks": state.blocks.iter().map(|b| json!({
            "p_ac_w": b.pcs.p_ac_w,
            "soc": b.average_soc(),
        })).collect::<Vec<_>>(),
    })
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

/// Write one single-sample metric family.
fn metric(out: &mut String, name: &str, kind: &str, help: &str, value: f64) {
    let _ = writeln!(
        out,
        "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
    );
}

/// Write one metric family whose samples are separated by a single label.
fn labeled_metric(
    out: &mut String,
    name: &str,
    kind: &str,
    help: &str,
    label: &str,
    samples: &[(&str, f64)],
) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
    for (value_of_label, value) in samples {
        let _ = writeln!(out, "{name}{{{label}=\"{value_of_label}\"}} {value}");
    }
}

/// Build the Prometheus exposition for a snapshot.
///
/// Kept separate from the handler so the content is testable, and so the
/// dashboard shipped under `deploy/grafana` can be checked against the
/// metrics that actually exist rather than the ones someone remembers.
fn metrics_body(snap: &Snapshot) -> String {
    let mut out = String::with_capacity(4096);
    site_metrics(&mut out, &snap.state, snap.speed);
    thermal_metrics(&mut out, &snap.state);
    house_load_metrics(&mut out, &snap.state);
    out
}

/// Grid-facing KPIs and the run's own bookkeeping.
fn site_metrics(out: &mut String, s: &SiteState, speed: f64) {
    metric(
        out,
        "bess_poi_active_power_watts",
        "gauge",
        "Active power at the POI (positive = export).",
        s.substation.poi_active_power_w,
    );
    metric(
        out,
        "bess_site_soc_ratio",
        "gauge",
        "Mean state of charge over in-service racks.",
        s.average_soc(),
    );
    metric(
        out,
        "bess_site_setpoint_watts",
        "gauge",
        "Site active-power target.",
        s.ems.site_setpoint_w,
    );
    metric(
        out,
        "bess_grid_frequency_hertz",
        "gauge",
        "Grid frequency at the POI.",
        s.substation.frequency_hz,
    );
    metric(
        out,
        "bess_import_watthours_total",
        "counter",
        "POI import meter.",
        s.substation.import_wh,
    );
    metric(
        out,
        "bess_export_watthours_total",
        "counter",
        "POI export meter.",
        s.substation.export_wh,
    );
    metric(
        out,
        "bess_sim_tick",
        "counter",
        "Simulation tick counter.",
        s.tick as f64,
    );
    metric(
        out,
        "bess_sim_speed",
        "gauge",
        "Time acceleration factor.",
        speed,
    );
}

/// The thermal chain M1 added: the weather that drives it, the container air
/// that answers it, the cells that lag it, and what the HVAC is doing about
/// all of it.
fn thermal_metrics(out: &mut String, s: &SiteState) {
    let containers: Vec<_> = s.blocks.iter().flat_map(|b| b.containers.iter()).collect();
    let air_max_c = containers
        .iter()
        .map(|c| c.air_temp_c)
        .fold(f64::NEG_INFINITY, f64::max);
    let air_mean_c = if containers.is_empty() {
        0.0
    } else {
        containers.iter().map(|c| c.air_temp_c).sum::<f64>() / containers.len() as f64
    };
    let mode_count =
        |mode: HvacMode| containers.iter().filter(|c| c.hvac.mode == mode).count() as f64;
    let (cell_min_c, cell_max_c) = s.cell_temp_min_max_c();

    metric(
        out,
        "bess_ambient_celsius",
        "gauge",
        "Ambient temperature.",
        s.weather.ambient_c,
    );
    metric(
        out,
        "bess_irradiance_watts_per_square_meter",
        "gauge",
        "Global horizontal irradiance driving the envelope solar gain.",
        s.weather.irradiance_wm2,
    );
    metric(
        out,
        "bess_container_air_celsius_max",
        "gauge",
        "Hottest container air temperature on site.",
        air_max_c,
    );
    metric(
        out,
        "bess_container_air_celsius_mean",
        "gauge",
        "Mean container air temperature over the site.",
        air_mean_c,
    );
    metric(
        out,
        "bess_cell_celsius_min",
        "gauge",
        "Coldest representative cell temperature on site.",
        cell_min_c,
    );
    metric(
        out,
        "bess_cell_celsius_max",
        "gauge",
        "Hottest representative cell temperature on site.",
        cell_max_c,
    );
    labeled_metric(
        out,
        "bess_hvac_containers",
        "gauge",
        "Containers in each HVAC mode.",
        "mode",
        &[
            ("off", mode_count(HvacMode::Off)),
            ("cool1", mode_count(HvacMode::Cool1)),
            ("cool2", mode_count(HvacMode::Cool2)),
            ("heat", mode_count(HvacMode::Heat)),
        ],
    );
}

/// The house load and the loss waterfall: what the site spent on itself and
/// where the rest of the gap between nameplate and meter went.
fn house_load_metrics(out: &mut String, s: &SiteState) {
    let aux = &s.aux;
    let items = &s.energy.aux_items;

    metric(
        out,
        "bess_aux_metered_power_watts",
        "gauge",
        "House load as the substation meters it, accumulated on its own path; \
         the itemized gauges have to add up to this.",
        s.substation.aux_power_w,
    );
    labeled_metric(
        out,
        "bess_aux_power_watts",
        "gauge",
        "House load by the item that drew it; the items sum to the metered total.",
        "item",
        &[
            ("hvac", aux.hvac_w),
            ("bms", aux.bms_w),
            ("pcs_standby", aux.pcs_standby_w),
            ("controls", aux.controls_w),
            ("lighting_and_safety", aux.lighting_and_safety_w),
        ],
    );
    labeled_metric(
        out,
        "bess_loss_watthours_total",
        "counter",
        "Energy that did not reach the POI, by category: the loss waterfall.",
        "category",
        &[
            ("battery", s.energy.battery_loss_wh),
            ("pcs", s.energy.pcs_loss_wh),
            ("transformer", s.energy.transformer_loss_wh),
            ("aux_hvac", items.hvac_wh),
            ("aux_bms", items.bms_wh),
            ("aux_pcs_standby", items.pcs_standby_wh),
            ("aux_controls", items.controls_wh),
            ("aux_lighting_and_safety", items.lighting_and_safety_wh),
        ],
    );
}

async fn metrics(State(handle): State<SimHandle>) -> impl IntoResponse {
    let snap = handle.snapshot.borrow().clone();
    let body = metrics_body(&snap);
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

/// Push the latest tick summary four times per wall second.
async fn push_summaries(mut socket: WebSocket, handle: SimHandle) {
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    loop {
        ticker.tick().await;
        let snap: Arc<Snapshot> = handle.snapshot.borrow().clone();
        let text = summary_value(&snap).to_string();
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use bess_core::state::AuxPower;
    use bess_core::{PlantConfig, SiteState};

    use super::*;

    /// A snapshot with a plausible house load and one container in each
    /// cooling stage, so the exposition has something to say.
    fn snapshot() -> Snapshot {
        let cfg = PlantConfig::gw01();
        let mut state = SiteState::new(&cfg, 1, 0);
        state.aux = AuxPower {
            hvac_w: 41_234.6,
            bms_w: 17_232.4,
            pcs_standby_w: 6_795.5,
            controls_w: 15_000.3,
            lighting_and_safety_w: 9_999.7,
        };
        state.substation.aux_power_w = state.aux.total_w();
        state.blocks[0].containers[0].hvac.mode = HvacMode::Cool1;
        state.blocks[0].containers[1].hvac.mode = HvacMode::Cool2;
        state.blocks[1].containers[0].hvac.mode = HvacMode::Heat;
        Snapshot {
            state,
            input_regs: Vec::new(),
            holding_regs: Vec::new(),
            speed: 60.0,
        }
    }

    /// Value of one sample line, by its full name and labels.
    fn value_of(body: &str, sample: &str) -> f64 {
        let line = body
            .lines()
            .find(|l| l.starts_with(sample) && l[sample.len()..].starts_with(' '))
            .unwrap_or_else(|| panic!("no sample {sample} in the exposition"));
        line.rsplit(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap_or_else(|_| panic!("unparsable sample line: {line}"))
    }

    /// The auxiliary identity, on the scraping surface this time: the five
    /// itemized gauges are accumulated per consumer, the metered gauge comes
    /// off the substation, and they have to agree.
    #[test]
    fn the_house_load_gauges_add_up_to_the_metered_total() {
        let body = metrics_body(&snapshot());
        let items: f64 = [
            "hvac",
            "bms",
            "pcs_standby",
            "controls",
            "lighting_and_safety",
        ]
        .iter()
        .map(|item| value_of(&body, &format!("bess_aux_power_watts{{item=\"{item}\"}}")))
        .sum();
        let metered = value_of(&body, "bess_aux_metered_power_watts");
        assert!(
            (items - metered).abs() < 1.0e-6,
            "items sum to {items} W, meter reads {metered} W"
        );
    }

    /// Every container is in exactly one mode, so the mode counts have to
    /// cover the site. A missing arm would silently drop containers out of
    /// the staging panel instead of failing.
    #[test]
    fn the_hvac_mode_counts_cover_every_container() {
        let cfg = PlantConfig::gw01();
        let body = metrics_body(&snapshot());
        let counted: f64 = ["off", "cool1", "cool2", "heat"]
            .iter()
            .map(|mode| value_of(&body, &format!("bess_hvac_containers{{mode=\"{mode}\"}}")))
            .sum();
        let containers = (cfg.blocks * cfg.containers_per_block) as f64;
        assert!(
            (counted - containers).abs() < f64::EPSILON,
            "{counted} containers counted, site has {containers}"
        );
    }

    /// Dashboards rot by outliving the metrics they query. Every `bess_*`
    /// name in the shipped dashboard has to be a family this exposition
    /// actually publishes.
    #[test]
    fn every_dashboard_query_names_a_metric_we_publish() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../deploy/grafana/dashboards/gw01.json");
        let text = std::fs::read_to_string(&path).expect("dashboard file");
        let dashboard: serde_json::Value = serde_json::from_str(&text).expect("dashboard json");
        let body = metrics_body(&snapshot());

        let mut checked = 0;
        for panel in dashboard["panels"].as_array().expect("panels") {
            for target in panel["targets"].as_array().into_iter().flatten() {
                let expr = target["expr"].as_str().unwrap_or_default();
                for name in expr
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .filter(|token| token.starts_with("bess_"))
                {
                    assert!(
                        body.contains(&format!("# TYPE {name} ")),
                        "panel {} queries {name}, which /metrics does not publish",
                        panel["title"]
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked >= 4,
            "the dashboard scan found only {checked} queries, so it is proving nothing"
        );
    }
}
