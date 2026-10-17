//! A scenario's status as JSON: the answer of `GET /api/v1/scenario`, and
//! of the browser module's `scenario_status()`.
//!
//! One builder for both, here beside the player whose progress it reports,
//! so a host page reading the browser door and a client reading REST see
//! the same shape for the same facts. COMPATIBILITY.md publishes it.

use bess_core::kernel::FaultAction;
use bess_core::TICK_SECONDS;
use serde_json::{json, Value};

use crate::player::Player;
use crate::resolve::{Due, Resolved, Scheduled};
use crate::schema::target::Target;

/// Identity, parameters, progress, and the whole schedule in firing order
/// with every drawn value visible, each event marked fired or not. Without
/// the tick, which the caller adds with [`with_tick`] from the snapshot it
/// answers from.
pub fn of(resolved: &Resolved, player: &Player) -> Value {
    let progress = player.progress();
    let schedule = player.schedule();
    let id = &resolved.id;
    let params = resolved.params;
    json!({
        "loaded": true,
        "name": id.name,
        "content_hash": format!("{:#018x}", id.content_hash),
        "seed": params.seed,
        "start_unix_s": params.start_unix_s,
        "speed": params.speed,
        "length_s": resolved.ticks.map(|t| t * TICK_SECONDS),
        "fired": progress.fired,
        "pending": progress.pending,
        "next": schedule.get(progress.fired).map(|s| event_value(s, false)),
        "events": schedule
            .iter()
            .enumerate()
            .map(|(i, s)| event_value(s, i < progress.fired))
            .collect::<Vec<_>>(),
    })
}

/// The answer when no scenario is loaded.
pub fn not_loaded() -> Value {
    json!({"loaded": false})
}

/// A status with the tick it describes.
pub fn with_tick(status: &Value, tick: u64) -> Value {
    let mut status = status.clone();
    status["tick"] = json!(tick);
    status
}

/// One scheduled event: its place in the file and in time, what it does,
/// and whether it has fired. A physical event names its target as the file
/// spells it and carries its drawn rate; a data event carries its
/// parameters as validated, defaults applied.
fn event_value(scheduled: &Scheduled, fired: bool) -> Value {
    let mut value = match &scheduled.due {
        Due::Physical(action) => {
            let mut v = json!({
                "kind": "physical",
                "target": Target::of(action).to_string(),
                "fault": action.name(),
            });
            if let FaultAction::RackSelfDischarge {
                excess_ppm_per_day, ..
            } = action
            {
                v["rate_ppm_per_day"] = json!(excess_ppm_per_day);
            }
            v
        }
        Due::Data(fault) => {
            let mut v = serde_json::to_value(fault).expect("a data fault is plain data");
            v["kind"] = json!("data");
            v
        }
    };
    value["index"] = json!(scheduled.index);
    value["at_s"] = json!(scheduled.at_s);
    value["tick"] = json!(scheduled.tick);
    value["fired"] = json!(fired);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_loaded_is_one_field_and_the_tick_is_added_not_replaced() {
        assert_eq!(not_loaded(), json!({"loaded": false}));
        let status = json!({"loaded": true, "fired": 1});
        assert_eq!(
            with_tick(&status, 42),
            json!({"loaded": true, "fired": 1, "tick": 42})
        );
    }
}
