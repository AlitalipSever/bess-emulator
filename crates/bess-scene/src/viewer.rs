//! The self-contained viewer application (feature `sim`): owns a
//! `Simulation`, steps it against wall time, and mediates between the
//! kernel and the view. This is the only place in the crate that touches
//! the kernel, keeping the scene/panels strictly read-only.

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather, HistoricalWeather, PrecipForm};

use crate::panels::{self, PanelState};
use crate::scene::SceneView;
use crate::scenery::{Observed, Precip, Scenery};
use crate::sun::sun_position;
use crate::ViewerCommand;

/// 2026-07-14 11:00:00 UTC: a bright summer late morning, so the plant
/// opens in full daylight and a 60x session reaches the evening discharge
/// window within minutes. Under replay this is Lindenberg's actual
/// 14 July 2024: 23 C climbing to 25 C, 650-720 W/m2 through the afternoon.
const START_UNIX_S: i64 = 1_767_225_600 + 194 * 86_400 + 11 * 3600;

/// Ticks are capped per frame so a stall (window drag, breakpoint) does not
/// freeze the UI catching up.
const MAX_TICKS_PER_FRAME: u64 = 7_200;

/// eframe application: kernel + scene + panels in one window or canvas.
pub struct ViewerApp {
    sim: Simulation,
    weather: HistoricalWeather,
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
            weather: gw01_weather(),
            scene,
            panel: PanelState::default(),
            speed: 60.0,
            tick_accum: 0.0,
        })
    }
}

impl ViewerApp {
    /// What the sky is doing right now, from the replayed observations.
    ///
    /// This is the only place the two halves meet: `bess-scene` does not name
    /// a `bess-data` type, and `bess-data` has never heard of a scene. The
    /// viewer owns both, so the translation is its job.
    fn scenery(&self) -> Scenery {
        let now = self.sim.unix_time_s();
        let observed = self.weather.hour_at(now);
        let elevation = sun_position(now, self.sim.config().location).elevation_deg;
        Scenery::from_observed(
            &Observed {
                cloud_okta: observed.cloud_okta,
                irradiance_wm2: observed.ghi_wm2,
                precip_mm_h: observed.precip_mm,
                precip: precip_of(observed.precip_form, observed.temp_c),
                wind_ms: observed.wind_ms,
                wind_dir_deg: observed.wind_dir_deg,
            },
            elevation,
        )
    }
}

/// The dataset's precipitation code, as the scene draws it.
///
/// One judgement call: DWD reports a form of "unknown" for hours where
/// something fell and nobody classified it. Rather than drop those hours or
/// guess a form, they follow the temperature, which is the same thing an
/// observer would have done.
fn precip_of(form: PrecipForm, temp_c: f32) -> Precip {
    match form {
        PrecipForm::NoPrecip => Precip::None,
        PrecipForm::Rain => Precip::Rain,
        PrecipForm::Snow => Precip::Snow,
        PrecipForm::Mixed => Precip::Sleet,
        PrecipForm::Unknown => {
            if temp_c <= 0.5 {
                Precip::Snow
            } else if temp_c <= 2.5 {
                Precip::Sleet
            } else {
                Precip::Rain
            }
        }
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

        let scenery = self.scenery();
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
