//! A deterministic cursor over a resolved schedule.
//!
//! The shell asks it, before stepping each tick, what is due; physical
//! actions go to `Simulation::apply`, data faults to the shell's tap. The
//! player never touches a simulation: its whole state is how far along the
//! schedule it is, which is a function of the tick, so a resumed run
//! rebuilds it with `catch_up` and the checkpoint carries none of it.

use crate::resolve::{Resolved, Scheduled};

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
