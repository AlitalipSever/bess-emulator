//! The viewer's one run loop: the plant, and the story it is playing, if
//! any.
//!
//! Both ways the viewer moves the plant, against the wall clock and in a
//! jump, call [`Run::tick`], so a jump across an event's tick fires it on
//! that tick like any other frame would. Firing goes through
//! `Player::fire_due`, the step the native shell and the headless run use,
//! so the browser plays a case the way CI does and lands on its digest.
//! Data faults come due and are listed, not executed: the browser has no
//! MQTT and no Modbus for them to act on.

use bess_core::{Models, PlantConfig, Simulation, TICK_SECONDS};
use bess_models::HistoricalWeather;
use bess_scenario::{Player, Resolved};

use crate::alarms::Journal;

use super::record;

/// A case in force: its fixed schedule and the cursor over it.
#[derive(Debug, Clone)]
pub struct Story {
    /// The resolved case: identity, parameters, schedule.
    pub resolved: Resolved,
    /// How far along the schedule the run is.
    pub player: Player,
}

/// The plant the viewer steps, and its story.
pub struct Run {
    /// The plant.
    pub sim: Simulation,
    /// The case being played, if any.
    pub story: Option<Story>,
}

impl Run {
    /// A run with no story: the plain viewer loop.
    pub fn plain(sim: Simulation) -> Self {
        Self { sim, story: None }
    }

    /// A fresh plant at the case's seed and start, its tree naming the case,
    /// with the player at the start of the schedule.
    pub fn playing(resolved: Resolved, cfg: PlantConfig, models: Models) -> Self {
        let sim = resolved.simulation(cfg, models);
        let player = Player::new(&resolved);
        Self {
            sim,
            story: Some(Story { resolved, player }),
        }
    }

    /// One tick: fire what the story has due, step, journal the events.
    pub fn tick(&mut self, weather: &HistoricalWeather, journal: &mut Journal) {
        if let Some(story) = &mut self.story {
            if let Some(refused) = story.player.fire_due(&mut self.sim).refused {
                // Validated against this site at load: a bug, not a user
                // error, and the run goes on without the action, as the
                // shell's does.
                log::warn!("scenario {}: {refused}", story.resolved.id.name);
            }
        }
        record::step(&mut self.sim, weather, journal);
    }

    /// Seconds since the story's start, while one plays.
    pub fn elapsed_s(&self) -> Option<u64> {
        self.story
            .as_ref()
            .map(|_| self.sim.state().tick * TICK_SECONDS)
    }

    /// When the next scheduled event fires, unix seconds.
    pub fn next_event_unix_s(&self) -> Option<i64> {
        let story = self.story.as_ref()?;
        let tick = story.player.progress().next_tick?;
        Some(self.at_tick(tick))
    }

    /// When the story's stated length runs out, unix seconds.
    pub fn end_unix_s(&self) -> Option<i64> {
        let ticks = self.story.as_ref()?.resolved.ticks?;
        Some(self.at_tick(ticks))
    }

    fn at_tick(&self, tick: u64) -> i64 {
        self.sim.state().meta.start_unix_s + (tick * TICK_SECONDS) as i64
    }
}
