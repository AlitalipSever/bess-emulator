//! Native viewer: `cargo run -p bess-scene --features sim --example viewer`
//!
//! Opens the GW-01 site in a window with the simulation running inside.
//! Orbit with the mouse (right-drag or shift-drag pans, scroll zooms),
//! switch to fly mode for a walk between the containers (WASD + QE),
//! click a container, PCS or the transformer to open its panel.

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_title("bess-emulator: GW-01"),
        ..Default::default()
    };
    eframe::run_native(
        "bess-emulator viewer",
        options,
        Box::new(|cc| {
            bess_scene::viewer::ViewerApp::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(Into::into)
        }),
    )
}
