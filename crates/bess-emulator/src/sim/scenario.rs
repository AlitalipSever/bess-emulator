//! A scenario in force: the player's home in the shell (design D6, D7).
//!
//! Before each tick the task asks what is due. Physical actions go through
//! the kernel's door; data faults advance the cursor and are logged as
//! scheduled only, until phase 5's tap executes them. The status the
//! surfaces publish is built here once per change: `GET /api/v1/scenario`'s
//! answer but for the tick, which the snapshot carries.

use std::sync::Arc;

use bess_core::kernel::FaultAction;
use bess_core::Simulation;
use bess_scenario::{Due, Player, Resolved, Scheduled, Target};
use serde_json::{json, Value};
use tracing::{info, warn};

/// The schedule and the cursor over it.
pub struct Active {
    resolved: Arc<Resolved>,
    player: Player,
}

impl Active {
    /// At the start of `resolved`'s schedule.
    pub fn new(resolved: Arc<Resolved>) -> Self {
        let player = Player::new(&resolved);
        Self { resolved, player }
    }

    /// Hand the kernel what is due before it steps the next tick; whether
    /// anything fired.
    pub fn fire_due(&mut self, sim: &mut Simulation) -> bool {
        let tick = sim.state().tick;
        let due = self.player.due(tick);
        for scheduled in due {
            let event = scheduled.index;
            match &scheduled.due {
                Due::Physical(action) => match sim.apply(*action) {
                    Ok(changed) => info!(
                        event,
                        tick,
                        target = %Target::of(action),
                        fault = action.name(),
                        changed,
                        "scenario: fault applied"
                    ),
                    // The file was validated against this site at load, so
                    // a refusal here is a bug, not an operator mistake; the
                    // run goes on without the action.
                    Err(err) => warn!(event, tick, "scenario: fault refused: {err}"),
                },
                Due::Data(fault) => info!(
                    event,
                    tick,
                    surface = fault.surface.name(),
                    fault = fault.fault.name(),
                    "scenario: data fault scheduled only; the tap arrives in phase 5"
                ),
            }
        }
        !due.is_empty()
    }

    /// The status: identity, parameters, progress, and the whole schedule in
    /// firing order with every drawn value visible (D7), each event marked
    /// fired or not. The tick is the snapshot's.
    pub fn status(&self) -> Value {
        let progress = self.player.progress();
        let schedule = self.player.schedule();
        let id = &self.resolved.id;
        let params = self.resolved.params;
        json!({
            "loaded": true,
            "name": id.name,
            "content_hash": format!("{:#018x}", id.content_hash),
            "seed": params.seed,
            "start_unix_s": params.start_unix_s,
            "speed": params.speed,
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
mod tests;
