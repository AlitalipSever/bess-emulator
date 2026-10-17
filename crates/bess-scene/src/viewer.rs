//! The self-contained viewer application (feature `sim`): owns a
//! `Simulation`, steps it against wall time, plays the scenario library,
//! and mediates between the kernel and the view. This is the only place in
//! the crate that touches the kernel, keeping the scene/panels strictly
//! read-only.
//!
//! - [`session`] everything the viewer does except draw
//! - [`run`]     the plant and its story, stepped one way everywhere
//! - [`jump`]    moving the plant to another moment, and what that costs
//! - [`record`]  one tick, with its events kept in the alarm journal
//! - [`library`] the scenario engine's types in the panel's terms
//! - [`presets`] days worth jumping to, derived from the dataset
//! - [`sky`]     the observed hour, in the terms the scene draws

pub mod jump;
pub mod library;
pub mod presets;
pub mod record;
pub mod run;
pub mod session;
pub mod sky;

use crate::panels;
use crate::scene::SceneView;

pub use session::Session;

/// eframe application: kernel + scene + panels in one window or canvas.
pub struct ViewerApp {
    session: Session,
    scene: SceneView,
}

impl ViewerApp {
    /// Build the app on eframe's GL context (requires the glow backend).
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let gl = cc
            .gl
            .as_ref()
            .ok_or("bess-scene requires eframe's glow backend")?;
        let session = Session::new();
        let scene = SceneView::new(gl, session.sim().config())?;
        Ok(Self { session, scene })
    }

    /// The session, for a shell that drives the viewer from outside (the
    /// browser module's JS door).
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}

impl eframe::App for ViewerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = f64::from(ctx.input(|i| i.stable_dt));
        self.session.advance(dt);
        // The plant always moves; repaint continuously.
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let s = &mut self.session;
        // The panel's scratch state is lent out for the frame, because the
        // input it reads borrows the rest of the session.
        let mut panel = std::mem::take(&mut s.panel);
        // The panel needs to know a jump is running so it can show progress
        // and refuse to start a second one on top of it.
        panel.jump_progress = s.jump_progress();
        let scenery = s.scenery();
        let input = panels::PanelInput {
            state: s.run.sim.state(),
            scenery: &scenery,
            stops: &s.stops,
            selection: self.scene.selection,
            journal: &s.journal,
            library: &s.library,
            story: s.story(),
        };
        let commands = panels::side_panel(ui, &input, &mut panel);
        s.panel = panel;
        for command in commands {
            s.apply(command);
        }
        // A row asked for its object: the scene's selection is the one
        // place a selection lives, so the request moves there.
        if let Some(sel) = s.panel.select_request.take() {
            self.scene.selection = Some(sel);
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.scene.show(ui, s.sim().state(), &scenery);
        });
    }

    // The browser module's JS door reaches the session through
    // `WebRunner::app_mut`, which needs the app as `Any`.
    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            self.scene.destroy(gl);
        }
    }
}
