//! The operator reset over REST, `POST /api/v1/alarms/reset`: the HMI
//! button.
//!
//! A reset clears the latched bits in its scope and takes a PCS in scope
//! out of fault, between two ticks; the clears go out as events with the
//! next tick. The answer lists the bits whose cause is still present, which
//! that next tick raises again.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use bess_core::alarms::{AlarmNode, ResetError, ResetScope};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::events::{alarm_name, node_path};
use crate::sim::{Command, SimHandle};

/// Body of `POST /api/v1/alarms/reset`: `{"scope": "site"}`,
/// `{"scope": "block", "block": 2}` or
/// `{"scope": "rack", "block": 2, "container": 1, "rack": 5}`.
#[derive(Debug, Deserialize)]
#[serde(tag = "scope", rename_all = "lowercase")]
pub(super) enum ResetRequest {
    Site,
    Block {
        block: usize,
    },
    Rack {
        block: usize,
        container: usize,
        rack: usize,
    },
}

impl From<ResetRequest> for ResetScope {
    fn from(req: ResetRequest) -> Self {
        match req {
            ResetRequest::Site => Self::Site,
            ResetRequest::Block { block } => Self::Block(block),
            ResetRequest::Rack {
                block,
                container,
                rack,
            } => Self::Rack {
                block,
                container,
                rack,
            },
        }
    }
}

/// The scope spelled as the node it names, the way events name nodes.
fn scope_path(scope: ResetScope) -> String {
    node_path(match scope {
        ResetScope::Site => AlarmNode::Site,
        ResetScope::Block(block) => AlarmNode::Block { block },
        ResetScope::Rack {
            block,
            container,
            rack,
        } => AlarmNode::Rack {
            block,
            container,
            rack,
        },
    })
}

fn unavailable() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error": "simulation task unavailable"})),
    )
}

pub(super) async fn reset(
    State(handle): State<SimHandle>,
    Json(req): Json<ResetRequest>,
) -> impl IntoResponse {
    let scope = ResetScope::from(req);
    let (reply, answer) = oneshot::channel();
    if handle
        .commands
        .send(Command::ResetAlarms { scope, reply })
        .await
        .is_err()
    {
        return unavailable();
    }
    match answer.await {
        Ok(Ok(still)) => (
            StatusCode::OK,
            Json(json!({
                "scope": scope_path(scope),
                "still_present": still.iter().map(|&(node, bit)| json!({
                    "node": node_path(node),
                    "alarm": alarm_name(node, bit),
                    "bit": bit,
                })).collect::<Vec<_>>(),
            })),
        ),
        // A well-formed request naming a node the site does not have: the
        // body is the problem, not the endpoint, hence 422 rather than 404.
        Ok(Err(ResetError::NoSuchNode(_))) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": format!("no such node on this site: {}", scope_path(scope))})),
        ),
        Err(_) => unavailable(),
    }
}

#[cfg(test)]
mod tests;
