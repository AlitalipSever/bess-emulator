//! Everything the viewer does except draw: the run, the journal, the
//! clock, the jump, the library, and the commands that change them.
//!
//! Its own type so it can be built and driven without a GL context: the
//! tests and the browser module's JS door call the same methods the panel's
//! buttons reach, and none of them needs a canvas.

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather, HistoricalWeather};
use bess_scenario::{library, status, RunParams, Scenario};

use crate::alarms::{Journal, SeenMark};
use crate::panels::scenario::{CaseListing, StoryNow, StoryView};
use crate::panels::{self, PanelState};
use crate::scenery::Scenery;
use crate::sun::sun_position;
use crate::ViewerCommand;

use super::jump::FastForward;
use super::run::Run;
use super::{library as listing, presets, sky};

/// 2026-07-14 11:00:00 UTC: a bright summer late morning, so the plant
/// opens in full daylight and a 60x session reaches the evening discharge
/// window within minutes. Under replay this is Lindenberg's actual
/// 14 July 2024: 23 C climbing to 25 C, 650-720 W/m2 through the afternoon.
pub const START_UNIX_S: i64 = 1_767_225_600 + 194 * 86_400 + 11 * 3600;

/// PRNG seed for the session. Held in one place because a restart has to
/// reuse it: a jump to another date should show the same plant on a
/// different day, not a different plant.
pub const SEED: u64 = 42;

/// Ticks are capped per frame so a stall (window drag, breakpoint) does not
/// freeze the UI catching up.
const MAX_TICKS_PER_FRAME: u64 = 7_200;

/// Ticks per frame while a jump is running.
///
/// Sized for the slow target rather than the fast one. Natively the kernel
/// turns about 200 000 ticks a second, so this is fifteen milliseconds; the
/// browser is several times slower and still keeps the canvas moving, which
/// is the whole reason the jump is split across frames instead of blocking.
pub const JUMP_TICKS_PER_FRAME: u64 = 3_000;

/// The viewer's state, minus the scene.
pub struct Session {
    /// The plant and its story.
    pub run: Run,
    weather: HistoricalWeather,
    /// The panel's scratch state.
    pub panel: PanelState,
    /// The seed this session opened with; a restart reuses it so the plant
    /// on the new date is the one that would have been there all along.
    seed: u64,
    speed: f64,
    tick_accum: f64,
    /// The jump in progress, if any. While this is set the plant runs at its
    /// own pace rather than against the wall clock.
    jump: Option<FastForward>,
    /// Days the dataset suggests. Computed once: the compiled year does not
    /// change while the viewer is open.
    pub stops: Vec<panels::Stop>,
    /// The events this plant has emitted since the viewer built it.
    pub journal: Journal,
    /// The library as the picker lists it, computed once.
    pub library: Vec<CaseListing>,
    /// The case being played, as the strip shows it.
    story_view: Option<StoryView>,
}

impl Session {
    /// GW-01 on the opening morning, no story.
    pub fn new() -> Self {
        let cfg = PlantConfig::gw01();
        let models = gw01_models(&cfg);
        let library = listing::listing(&cfg);
        let sim = Simulation::new(cfg, models, SEED, START_UNIX_S);
        let weather = gw01_weather();
        Self {
            run: Run::plain(sim),
            weather,
            panel: PanelState::default(),
            seed: SEED,
            speed: 60.0,
            tick_accum: 0.0,
            jump: None,
            stops: presets::stops(weather.year()),
            journal: Journal::new(),
            library,
            story_view: None,
        }
    }

    /// The plant.
    pub fn sim(&self) -> &Simulation {
        &self.run.sim
    }

    /// Advance by `dt_s` wall seconds: a slice of the jump in progress if one
    /// runs, otherwise wall time times the speed.
    pub fn advance(&mut self, dt_s: f64) {
        if let Some(jump) = self.jump {
            // A jump owns the frame: no wall-clock stepping while it runs,
            // or the plant would be advancing for two reasons at once.
            if jump.advance(
                &mut self.run,
                &self.weather,
                &mut self.journal,
                JUMP_TICKS_PER_FRAME,
            ) {
                self.jump = None;
                self.tick_accum = 0.0;
            }
            return;
        }
        self.tick_accum += dt_s.min(0.25) * self.speed;
        let ticks = (self.tick_accum as u64).min(MAX_TICKS_PER_FRAME);
        self.tick_accum = (self.tick_accum - ticks as f64).max(0.0);
        for _ in 0..ticks {
            self.run.tick(&self.weather, &mut self.journal);
        }
    }

    /// Whether a jump is running.
    pub fn jumping(&self) -> bool {
        self.jump.is_some()
    }

    /// Carry out one command from the panel.
    pub fn apply(&mut self, command: ViewerCommand) {
        match command {
            ViewerCommand::SetSpeed(s) => self.speed = s.clamp(1.0, 3600.0),
            ViewerCommand::SetSetpoint(sp) => self.run.sim.set_external_setpoint_w(sp),
            ViewerCommand::RestartAt(unix_s) => self.restart_at(unix_s),
            ViewerCommand::FastForwardTo(unix_s) => self.jump_to(Some(unix_s)),
            ViewerCommand::ResetAlarms(scope) => {
                // The kernel's own door, the one REST uses. The clears go
                // out with the next tick, and the journal shows them.
                let outcome = self.run.sim.reset_alarms(scope).map_err(|e| e.to_string());
                self.panel.reset_answer = Some(panels::ResetAnswer {
                    scope,
                    unix_time_s: self.run.sim.unix_time_s(),
                    outcome,
                });
            }
            ViewerCommand::StartScenario(name) => {
                if let Err(err) = self.start_scenario(name) {
                    log::warn!("{err}");
                }
            }
            ViewerCommand::RunToNextEvent => self.jump_to(self.run.next_event_unix_s()),
            ViewerCommand::RunToEnd => self.jump_to(self.run.end_unix_s()),
        }
    }

    fn jump_to(&mut self, target: Option<i64>) {
        if self.jump.is_none() {
            self.jump = target.and_then(|t| FastForward::toward(&self.run.sim, t));
        }
    }

    /// Throw the plant away and build a fresh one on another date, leaving
    /// any story: a schedule is offsets from a start, and a plant restarted
    /// elsewhere is not the file's run.
    ///
    /// The seed and the configuration are the ones this session started
    /// with, so a restart is the run that would have happened had the
    /// session opened there, not a new experiment.
    pub fn restart_at(&mut self, unix_s: i64) {
        let cfg = self.run.sim.config().clone();
        let models = gw01_models(&cfg);
        self.run = Run::plain(Simulation::new(cfg, models, self.seed, unix_s));
        self.story_view = None;
        self.forget_the_old_plant();
    }

    /// Play the library case named `name`. Unknown names are refused with
    /// the library's names in the message.
    pub fn start_scenario(&mut self, name: &str) -> Result<(), String> {
        let Some(case) = library::find(name) else {
            let names: Vec<&str> = library::CASES.iter().map(|c| c.name).collect();
            return Err(format!(
                "no library case named {name:?}; the library has {}",
                names.join(", ")
            ));
        };
        let scenario = case
            .load(self.run.sim.config())
            .map_err(|e| format!("{name}: {e}"))?;
        self.play(scenario, case.headline());
        Ok(())
    }

    /// Validate a scenario file's text against the site and play it.
    pub fn load_scenario_yaml(&mut self, text: &str) -> Result<(), String> {
        let scenario = bess_scenario::load(text.as_bytes(), self.run.sim.config())
            .map_err(|e| e.to_string())?;
        self.play(scenario, String::new());
        Ok(())
    }

    /// Start a case as a REST load does: a fresh plant at the file's seed and
    /// start, at the file's speed if it states one, the log back at 1, no
    /// external setpoint. What the file leaves out comes from the session:
    /// its seed, the date the plant is on, the speed it runs at.
    fn play(&mut self, scenario: Scenario, headline: String) {
        let params = RunParams {
            seed: scenario.seed.unwrap_or(self.seed),
            start_unix_s: scenario
                .start_unix_s
                .unwrap_or_else(|| self.run.sim.unix_time_s()),
            speed: scenario.speed.unwrap_or(self.speed),
        };
        let taxonomy = scenario.taxonomy.map(bess_scenario::Taxonomy::name);
        let resolved = scenario.resolve(params);
        self.story_view = Some(listing::story_view(&resolved, headline, taxonomy));
        let cfg = self.run.sim.config().clone();
        let models = gw01_models(&cfg);
        self.run = Run::playing(resolved, cfg, models);
        self.speed = params.speed.clamp(1.0, 3600.0);
        self.panel.speed = self.speed;
        self.forget_the_old_plant();
    }

    /// The kernel's numbering starts over with a new plant, so the journal,
    /// the bookmark in it, the last reset's answer and any jump go too. The
    /// selection and the camera stay: they are the person's.
    fn forget_the_old_plant(&mut self) {
        self.jump = None;
        self.tick_accum = 0.0;
        self.journal.clear();
        self.panel.alarms_seen = SeenMark::default();
        self.panel.reset_answer = None;
    }

    /// The embedded library as JSON: an array of `{name, taxonomy,
    /// length_s, headline}`, the browser door's `scenarios()`.
    pub fn scenarios_json(&self) -> String {
        library::catalogue(self.run.sim.config()).to_string()
    }

    /// The story's status as JSON, in the shape `GET /api/v1/scenario`
    /// publishes, built by the same function; `{"loaded":false}` when no
    /// case plays. The browser door's `scenario_status()`.
    pub fn scenario_status_json(&self) -> String {
        match &self.run.story {
            Some(story) => status::with_tick(
                &status::of(&story.resolved, &story.player),
                self.run.sim.state().tick,
            ),
            None => status::not_loaded(),
        }
        .to_string()
    }

    /// The case being played and where it is, for the strip.
    pub fn story(&self) -> Option<(&StoryView, StoryNow)> {
        let view = self.story_view.as_ref()?;
        let story = self.run.story.as_ref()?;
        Some((
            view,
            StoryNow {
                elapsed_s: self.run.elapsed_s().unwrap_or(0),
                fired: story.player.progress().fired,
            },
        ))
    }

    /// Progress of the jump in progress, for the panel.
    pub fn jump_progress(&self) -> Option<f32> {
        self.jump.map(|j| j.progress(&self.run.sim))
    }

    /// What the sky is doing right now, from the replayed observations.
    pub fn scenery(&self) -> Scenery {
        let now = self.run.sim.unix_time_s();
        let elevation = sun_position(now, self.run.sim.config().location).elevation_deg;
        Scenery::from_observed(&sky::observed_from(&self.weather.hour_at(now)), elevation)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
