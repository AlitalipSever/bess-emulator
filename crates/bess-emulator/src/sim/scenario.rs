//! A scenario in force: the player's home in the shell (design D6, D7).
//!
//! Before each tick the task asks what is due. Physical actions go through
//! the kernel's door; a data fault opens its window on the tap, which the
//! surfaces read, and touches nothing in the kernel. The status the
//! surfaces publish is built here once per change: `GET /api/v1/scenario`'s
//! answer but for the tick, which the snapshot carries.

use std::sync::Arc;

use bess_core::Simulation;
use bess_scenario::{status, DataFaultKind, Due, Player, Resolved, Target};
use serde_json::Value;
use tracing::{info, warn};

use crate::map::Point;
use crate::tap::Taps;

/// The schedule, the cursor over it, and the tap its data faults open.
pub struct Active {
    resolved: Arc<Resolved>,
    player: Player,
    points: Arc<Vec<Point>>,
    taps: Arc<Taps>,
}

impl Active {
    /// At the start of `resolved`'s schedule, with the map the data faults
    /// select points from.
    pub fn new(resolved: Arc<Resolved>, points: Arc<Vec<Point>>) -> Self {
        let player = Player::new(&resolved);
        let taps = Arc::new(Taps::new(&resolved, &points));
        Self {
            resolved,
            player,
            points,
            taps,
        }
    }

    /// The data faults as the surfaces read them.
    pub fn taps(&self) -> Arc<Taps> {
        Arc::clone(&self.taps)
    }

    /// Hand the kernel what is due before it steps the next tick, through
    /// the player's one door for it, and open the data faults' windows on
    /// the tap; whether anything fired.
    pub fn fire_due(&mut self, sim: &mut Simulation) -> bool {
        let tick = sim.state().tick;
        let fired = self.player.fire_due(sim);
        for scheduled in fired.due {
            let event = scheduled.index;
            match &scheduled.due {
                // The file was validated against this site at load, so a
                // refusal here is a bug, not an operator mistake; the run
                // goes on without the action.
                Due::Physical(_) if fired.refused.is_some_and(|r| r.index == event) => {
                    if let Some(refused) = fired.refused {
                        warn!(event, tick, "scenario: fault refused: {}", refused.error);
                    }
                }
                Due::Physical(action) => info!(
                    event,
                    tick,
                    target = %Target::of(action),
                    fault = action.name(),
                    "scenario: fault applied"
                ),
                Due::Data(fault) => {
                    // A freeze holds what the points read now; the other
                    // windows are a function of the tick alone.
                    if fault.fault == DataFaultKind::Freeze {
                        let state = sim.state();
                        let values = self.points.iter().map(|p| (p.extract)(state)).collect();
                        Arc::make_mut(&mut self.taps).open(event, values);
                    }
                    info!(
                        event,
                        tick,
                        surface = fault.surface.name(),
                        fault = fault.fault.name(),
                        "scenario: data fault opens on the tap"
                    );
                }
            }
        }
        !fired.due.is_empty()
    }

    /// The status: identity, parameters, progress, and the whole schedule in
    /// firing order with every drawn value visible (D7), each event marked
    /// fired or not. Built by `bess_scenario::status`, which the browser
    /// module's door uses too, so the two cannot drift. The tick is the
    /// snapshot's.
    pub fn status(&self) -> Value {
        status::of(&self.resolved, &self.player)
    }
}

#[cfg(test)]
mod tests;
