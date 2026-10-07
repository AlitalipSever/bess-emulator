//! The operator reset over REST, `POST /api/v1/alarms/reset`: the HMI
//! button.
//!
//! A reset clears the latched bits in its scope and takes a PCS in scope
//! out of fault, between two ticks; the clears go out as events with the
//! next tick. The answer lists the bits whose cause is still present, which
//! that next tick raises again.
//!
//! The body is read strictly. A reset is an operator action that takes
//! converters out of fault, so a body the endpoint does not fully understand
//! is refused rather than read as the nearest scope it parses to: a rack
//! reset sent with the wrong scope tag must not clear the whole block.

use axum::extract::rejection::JsonRejection;
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
/// `{"scope": "rack", "block": 2, "container": 1, "rack": 5}`, and nothing
/// else: unknown fields are refused. `Site` is a struct variant because
/// serde checks unknown fields only on variants that have fields to check.
#[derive(Debug, Deserialize)]
#[serde(tag = "scope", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum ResetRequest {
    Site {},
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
            ResetRequest::Site {} => Self::Site,
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
    body: Result<Json<ResetRequest>, JsonRejection>,
) -> impl IntoResponse {
    let req = match body {
        Ok(Json(req)) => req,
        // Axum answers a body it cannot use with 422, and 422 is this
        // endpoint's answer for a node the site does not have; a body that
        // is malformed, names an unknown scope, or carries fields no scope
        // has is a 400, with the reason as JSON like every other answer.
        Err(rejection) => {
            let status = match rejection.status() {
                StatusCode::UNPROCESSABLE_ENTITY => StatusCode::BAD_REQUEST,
                other => other,
            };
            return (status, Json(json!({"error": rejection.body_text()})));
        }
    };
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
                "node": scope_path(scope),
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
