//! The self-contained viewer application (feature `sim`): owns a
//! `Simulation`, steps it against wall time, and mediates between the
//! kernel and the view. This is the only place in the crate that touches
//! the kernel, keeping the scene/panels strictly read-only.
//!
//! - [`jump`]    moving the plant to another moment, and what that costs
//! - [`presets`] days worth jumping to, derived from the dataset
//! - [`record`]  one tick, with its events kept in the alarm journal
//! - [`sky`]     the observed hour, in the terms the scene draws

pub mod jump;
pub mod presets;
pub mod record;
pub mod sky;

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather, HistoricalWeather};

use crate::alarms::{self, Journal};
use crate::panels::{self, PanelState};
use crate::scene::SceneView;
use crate::scenery::Scenery;
use crate::sun::sun_position;
use crate::ViewerCommand;

use jump::FastForward;

/// 2026-07-14 11:00:00 UTC: a bright summer late morning, so the plant
/// opens in full daylight and a 60x session reaches the evening discharge
/// window within minutes. Under replay this is Lindenberg's actual
/// 14 July 2024: 23 C climbing to 25 C, 650-720 W/m2 through the afternoon.
const START_UNIX_S: i64 = 1_767_225_600 + 194 * 86_400 + 11 * 3600;

/// PRNG seed for the session. Held in one place because a restart has to
/// reuse it: a jump to another date should show the same plant on a
/// different day, not a different plant.
const SEED: u64 = 42;

/// Ticks are capped per frame so a stall (window drag, breakpoint) does not
/// freeze the UI catching up.
const MAX_TICKS_PER_FRAME: u64 = 7_200;

/// Ticks per frame while a jump is running.
///
/// Sized for the slow target rather than the fast one. Natively the kernel
/// turns about 200 000 ticks a second, so this is fifteen milliseconds; the
/// browser is several times slower and still keeps the canvas moving, which
/// is the whole reason the jump is split across frames instead of blocking.
const JUMP_TICKS_PER_FRAME: u64 = 3_000;

/// eframe application: kernel + scene + panels in one window or canvas.
pub struct ViewerApp {
    sim: Simulation,
    weather: HistoricalWeather,
    scene: SceneView,
    panel: PanelState,
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
    stops: Vec<panels::Stop>,
    /// The events this plant has emitted since the viewer built it.
    journal: Journal,
}

impl ViewerApp {
    /// Build the app on eframe's GL context (requires the glow backend).
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let gl = cc
            .gl
            .as_ref()
            .ok_or("bess-scene requires eframe's glow backend")?;
        let cfg = PlantConfig::gw01();
        let scene = SceneView::new(gl, &cfg)?;
        let models = gw01_models(&cfg);
        let sim = Simulation::new(cfg, models, SEED, START_UNIX_S);
        let weather = gw01_weather();
        Ok(Self {
            sim,
            weather,
            scene,
            panel: PanelState::default(),
            seed: SEED,
            speed: 60.0,
            tick_accum: 0.0,
            jump: None,
            stops: presets::stops(weather.year()),
            journal: Journal::new(),
        })
    }
}

impl ViewerApp {
    /// Throw the plant away and build a fresh one on another date.
    ///
    /// The seed and the configuration are the ones this session started
    /// with, so a restart is the run that would have happened had the
    /// session opened there, not a new experiment.
    fn restart_at(&mut self, unix_s: i64) {
        let cfg = self.sim.config().clone();
        let models = gw01_models(&cfg);
        self.sim = Simulation::new(cfg, models, self.seed, unix_s);
        self.jump = None;
        self.tick_accum = 0.0;
        // The kernel's numbering starts over with the plant, so the journal
        // and the bookmark in it do too.
        self.journal.clear();
        self.panel.alarms_seen = alarms::SeenMark::default();
        self.panel.reset_answer = None;
    }

    /// What the sky is doing right now, from the replayed observations.
    fn scenery(&self) -> Scenery {
        let now = self.sim.unix_time_s();
        let elevation = sun_position(now, self.sim.config().location).elevation_deg;
        Scenery::from_observed(&sky::observed_from(&self.weather.hour_at(now)), elevation)
    }
}

impl eframe::App for ViewerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(jump) = self.jump {
            // A jump owns the frame: no wall-clock stepping while it runs,
            // or the plant would be advancing for two reasons at once.
            if jump.advance(
                &mut self.sim,
                &self.weather,
                &mut self.journal,
                JUMP_TICKS_PER_FRAME,
            ) {
                self.jump = None;
                self.tick_accum = 0.0;
            }
            ctx.request_repaint();
            return;
        }

        // Advance the plant by wall-time x speed.
        let dt = f64::from(ctx.input(|i| i.stable_dt).min(0.25));
        self.tick_accum += dt * self.speed;
        let ticks = (self.tick_accum as u64).min(MAX_TICKS_PER_FRAME);
        self.tick_accum = (self.tick_accum - ticks as f64).max(0.0);
        for _ in 0..ticks {
            record::step(&mut self.sim, &self.weather, &mut self.journal);
        }
        // The plant always moves; repaint continuously.
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The panel needs to know a jump is running so it can show progress
        // and refuse to start a second one on top of it.
        self.panel.jump_progress = self.jump.map(|j| j.progress(&self.sim));
        let scenery = self.scenery();
        let commands = panels::side_panel(
            ui,
            &panels::PanelInput {
                state: self.sim.state(),
                scenery: &scenery,
                stops: &self.stops,
                selection: self.scene.selection,
                journal: &self.journal,
            },
            &mut self.panel,
        );
        for command in commands {
            match command {
                ViewerCommand::SetSpeed(s) => self.speed = s.clamp(1.0, 3600.0),
                ViewerCommand::SetSetpoint(sp) => self.sim.set_external_setpoint_w(sp),
                ViewerCommand::RestartAt(unix_s) => self.restart_at(unix_s),
                ViewerCommand::FastForwardTo(unix_s) => {
                    self.jump = FastForward::toward(&self.sim, unix_s);
                }
                ViewerCommand::ResetAlarms(scope) => {
                    // The kernel's own door, the one REST uses. The clears
                    // go out with the next tick, and the journal shows them.
                    let outcome = self.sim.reset_alarms(scope).map_err(|e| e.to_string());
                    self.panel.reset_answer = Some(panels::ResetAnswer {
                        scope,
                        unix_time_s: self.sim.unix_time_s(),
                        outcome,
                    });
                }
            }
        }
        // A row asked for its object: the scene's selection is the one
        // place a selection lives, so the request moves there.
        if let Some(sel) = self.panel.select_request.take() {
            self.scene.selection = Some(sel);
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.scene.show(ui, self.sim.state(), &scenery);
        });
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            self.scene.destroy(gl);
        }
    }
}
