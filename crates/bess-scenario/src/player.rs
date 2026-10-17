//! A deterministic cursor over a resolved schedule.
//!
//! The shell asks it, before stepping each tick, what is due; physical
//! actions go to `Simulation::apply`, data faults to the shell's tap.
//! [`Player::fire_due`] is that step written once for every caller: it
//! applies through the kernel's door and returns what came due, and it
//! never steps. The player's whole state is how far along the schedule it
//! is, which is a function of the tick, so a resumed run rebuilds it with
//! `catch_up` and the checkpoint carries none of it.

use bess_core::kernel::FaultError;
use bess_core::Simulation;

use crate::resolve::{Due, Resolved, Scheduled};

/// Where a player is along its schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Events handed out so far.
    pub fired: usize,
    /// Events still to come.
    pub pending: usize,
    /// The tick the next one fires before, if any are left.
    pub next_tick: Option<u64>,
}

/// A physical event the kernel refused. The file was validated against the
/// site at load, so a refusal is a bug, not an operator mistake; it names
/// its event so the message can say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("events[{index}] refused: {error}")]
pub struct Refused {
    /// The event's position in the file.
    pub index: usize,
    /// What the kernel said.
    pub error: FaultError,
}

/// What one call to [`Player::fire_due`] did.
#[derive(Debug)]
pub struct Fired<'a> {
    /// Everything that came due, physical and data, in schedule order.
    pub due: &'a [Scheduled],
    /// The first physical event the kernel refused, if any. The ones after
    /// it were still applied.
    pub refused: Option<Refused>,
}

/// The cursor and its copy of the schedule.
#[derive(Debug, Clone)]
pub struct Player {
    schedule: Vec<Scheduled>,
    cursor: usize,
}

impl Player {
    /// A player at the start of `resolved`'s schedule.
    pub fn new(resolved: &Resolved) -> Self {
        Self {
            schedule: resolved.schedule.clone(),
            cursor: 0,
        }
    }

    /// Everything due before the kernel steps `tick` and not yet handed
    /// out, in schedule order, advancing the cursor past it. Called once
    /// per tick that is exactly the events scheduled at `tick`; a caller
    /// that skipped ticks gets the ones it skipped as well, never loses
    /// them.
    pub fn due(&mut self, tick: u64) -> &[Scheduled] {
        let from = self.cursor;
        while self
            .schedule
            .get(self.cursor)
            .is_some_and(|s| s.tick <= tick)
        {
            self.cursor += 1;
        }
        &self.schedule[from..self.cursor]
    }

    /// Apply what is due before `sim` steps its next tick, in schedule
    /// order, through `Simulation::apply`, and return everything that came
    /// due so a shell can act on the data faults. Does not step.
    pub fn fire_due(&mut self, sim: &mut Simulation) -> Fired<'_> {
        let due = self.due(sim.state().tick);
        let mut refused = None;
        for scheduled in due {
            if let Due::Physical(action) = scheduled.due {
                if let Err(error) = sim.apply(action) {
                    refused.get_or_insert(Refused {
                        index: scheduled.index,
                        error,
                    });
                }
            }
        }
        Fired { due, refused }
    }

    /// Mark everything scheduled before `tick` as fired without handing it
    /// out: a run resumed at `tick` already applied those, and its tree
    /// says so.
    pub fn catch_up(&mut self, tick: u64) {
        self.cursor = self.schedule.partition_point(|s| s.tick < tick);
    }

    /// How far along the schedule the player is.
    pub fn progress(&self) -> Progress {
        Progress {
            fired: self.cursor,
            pending: self.schedule.len() - self.cursor,
            next_tick: self.schedule.get(self.cursor).map(|s| s.tick),
        }
    }

    /// The schedule, in firing order.
    pub fn schedule(&self) -> &[Scheduled] {
        &self.schedule
    }
}

#[cfg(test)]
mod tests {
    use bess_core::kernel::FaultAction;
    use bess_core::state::ScenarioId;

    use super::{Player, Progress};
    use crate::resolve::{Due, Resolved, RunParams, Scheduled};

    fn at(index: usize, tick: u64) -> Scheduled {
        Scheduled {
            index,
            at_s: tick,
            tick,
            due: Due::Physical(FaultAction::ProtectionTrip),
        }
    }

    fn resolved(ticks: &[u64]) -> Resolved {
        Resolved {
            id: ScenarioId {
                name: "t".to_owned(),
                content_hash: 0,
            },
            params: RunParams {
                seed: 1,
                start_unix_s: 0,
                speed: 1.0,
            },
            ticks: None,
            schedule: ticks.iter().enumerate().map(|(i, &t)| at(i, t)).collect(),
        }
    }

    #[test]
    fn hands_out_each_event_once_on_its_tick() {
        let mut p = Player::new(&resolved(&[0, 0, 5, 9]));
        assert_eq!(p.due(0).len(), 2);
        assert_eq!(p.due(0), []);
        assert_eq!(p.due(4), []);
        assert_eq!(p.due(5)[0].index, 2);
        assert_eq!(
            p.progress(),
            Progress {
                fired: 3,
                pending: 1,
                next_tick: Some(9)
            }
        );
        // A skipped tick does not lose its event.
        assert_eq!(p.due(20)[0].index, 3);
        assert_eq!(p.progress().next_tick, None);
    }

    /// The one step every caller shares: physical events go through the
    /// kernel's door in schedule order, data events come back untouched,
    /// and a refusal names its event without stopping the ones after it.
    #[test]
    fn fire_due_applies_in_order_and_names_a_refusal() {
        use bess_core::kernel::{Event, FaultError};
        use bess_core::{PlantConfig, Simulation};

        let cfg = PlantConfig::gw01();
        let models = bess_models::gw01_models(&cfg);
        let mut sim = Simulation::new(cfg, models, 1, 1_767_225_600);
        let mut schedule = resolved(&[0, 0, 0, 3]);
        schedule.schedule[0].due = Due::Physical(FaultAction::PcsTrip { block: 2 });
        schedule.schedule[1].due = Due::Physical(FaultAction::PcsTrip { block: 99 });
        schedule.schedule[2].due = Due::Physical(FaultAction::PcsTrip { block: 4 });
        let mut p = Player::new(&schedule);

        let fired = p.fire_due(&mut sim);
        assert_eq!(fired.due.len(), 3);
        let refused = fired.refused.expect("block 99 does not exist");
        assert_eq!(refused.index, 1);
        assert!(matches!(refused.error, FaultError::Command(_)));
        assert!(refused.to_string().starts_with("events[1] refused: "));

        // Both real trips went in, in order, ahead of the tick they mark.
        let weather = bess_models::gw01_weather();
        let events = sim.step(&weather.inputs_at(sim.unix_time_s())).to_vec();
        let marks: Vec<&Event> = events
            .iter()
            .filter(|e| matches!(e, Event::FaultApplied(_)))
            .collect();
        assert_eq!(
            marks,
            [
                &Event::FaultApplied(FaultAction::PcsTrip { block: 2 }),
                &Event::FaultApplied(FaultAction::PcsTrip { block: 4 }),
            ]
        );
        // Nothing more until tick 3.
        assert_eq!(p.fire_due(&mut sim).due, []);
    }

    #[test]
    fn catch_up_skips_what_a_resumed_tree_already_applied() {
        let mut p = Player::new(&resolved(&[0, 5, 5, 9]));
        p.catch_up(5);
        assert_eq!(p.progress().fired, 1);
        assert_eq!(p.due(5).len(), 2);
        p.catch_up(10);
        assert_eq!(p.progress().pending, 0);
    }
}
