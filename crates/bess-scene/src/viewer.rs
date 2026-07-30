//! The self-contained viewer application (feature `sim`): owns a
//! `Simulation`, steps it against wall time, and mediates between the
//! kernel and the view. This is the only place in the crate that touches
//! the kernel, keeping the scene/panels strictly read-only.

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, SyntheticWeather};

use crate::panels::{self, PanelState};
use crate::scene::SceneView;
use crate::ViewerCommand;

/// 2026-07-14 11:00:00 UTC: a bright summer late morning, so the plant
/// opens in full daylight and a 60x session reaches the evening discharge
/// window within minutes.
const START_UNIX_S: i64 = 1_767_225_600 + 194 * 86_400 + 11 * 3600;

/// Ticks are capped per frame so a stall (window drag, breakpoint) does not
/// freeze the UI catching up.
const MAX_TICKS_PER_FRAME: u64 = 7_200;

/// eframe application: kernel + scene + panels in one window or canvas.
pub struct ViewerApp {
    sim: Simulation,
    weather: SyntheticWeather,
    scene: SceneView,
    panel: PanelState,
    speed: f64,
    tick_accum: f64,
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
        let sim = Simulation::new(cfg, models, 42, START_UNIX_S);
        Ok(Self {
            sim,
            weather: SyntheticWeather::default(),
            scene,
            panel: PanelState::default(),
            speed: 60.0,
            tick_accum: 0.0,
        })
    }
}

impl eframe::App for ViewerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Advance the plant by wall-time x speed.
        let dt = f64::from(ctx.input(|i| i.stable_dt).min(0.25));
        self.tick_accum += dt * self.speed;
        let ticks = (self.tick_accum as u64).min(MAX_TICKS_PER_FRAME);
        self.tick_accum = (self.tick_accum - ticks as f64).max(0.0);
        for _ in 0..ticks {
            let inputs = self.weather.inputs_at(self.sim.unix_time_s());
            self.sim.step(&inputs);
        }
        // The plant always moves; repaint continuously.
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let commands =
            panels::side_panel(ui, self.sim.state(), self.scene.selection, &mut self.panel);
        for command in commands {
            match command {
                ViewerCommand::SetSpeed(s) => self.speed = s.clamp(1.0, 3600.0),
                ViewerCommand::SetSetpoint(sp) => self.sim.set_external_setpoint_w(sp),
            }
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.scene.show(ui, self.sim.state());
        });
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            self.scene.destroy(gl);
        }
    }
}
