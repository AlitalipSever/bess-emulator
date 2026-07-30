//! The scene widget: routes egui input to the camera, picks on click, and
//! hands one frame of instance data to the renderer via a paint callback.

use std::sync::{Arc, Mutex};

use bess_core::config::PlantConfig;
use bess_core::state::SiteState;
use eframe::egui_glow::CallbackFn;

use crate::camera::{Camera, CameraMode, FlyInput};
use crate::instances::{self, DynamicInput};
use crate::layout::{Selection, SiteLayout};
use crate::renderer::{FrameData, Renderer};
use crate::style::{Palette, Style};
use crate::sun;

/// Fog start and full distance, m, tuned to the ~160 m site.
const FOG_RANGE: [f32; 2] = [130.0, 460.0];

/// The 3D view: camera, selection, cached geometry, renderer handle.
pub struct SceneView {
    camera: Camera,
    layout: SiteLayout,
    style: Style,
    palette: Palette,
    renderer: Arc<Mutex<Renderer>>,
    ground: Vec<f32>,
    static_objects: Vec<f32>,
    /// Currently selected object, if any. Read by the panels.
    pub selection: Option<Selection>,
}

impl SceneView {
    /// Build the view for a plant configuration on an existing GL context.
    pub fn new(gl: &eframe::glow::Context, cfg: &PlantConfig) -> Result<Self, String> {
        let layout = SiteLayout::new(cfg);
        let style = Style::default();
        let renderer = Renderer::new(gl)?;
        let ground = instances::build_ground(&layout, &style);
        let static_objects = instances::build_static(&layout, &style);
        Ok(Self {
            camera: Camera::overview(),
            layout,
            style,
            palette: Palette::default(),
            renderer: Arc::new(Mutex::new(renderer)),
            ground,
            static_objects,
            selection: None,
        })
    }

    /// Free GL resources (call from `eframe::App::on_exit`).
    pub fn destroy(&mut self, gl: &eframe::glow::Context) {
        if let Ok(mut r) = self.renderer.lock() {
            r.destroy(gl);
        }
    }

    /// Show the scene filling the available space and handle interaction.
    pub fn show(&mut self, ui: &mut egui::Ui, state: &SiteState) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);
        let dt = ui.input(|i| i.stable_dt).min(0.1);

        // -- input ----------------------------------------------------
        let shift = ui.input(|i| i.modifiers.shift);
        if response.dragged_by(egui::PointerButton::Primary) && !shift {
            let d = response.drag_delta();
            self.camera.look_drag(d.x, d.y);
            self.camera.auto_orbit = false;
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged_by(egui::PointerButton::Primary) && shift)
        {
            let d = response.drag_delta();
            self.camera.pan_drag(d.x, d.y);
            self.camera.auto_orbit = false;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.zoom(scroll);
            }
        }
        if self.camera.mode == CameraMode::Fly {
            let fly = ui.input(|i| FlyInput {
                forward: i.key_down(egui::Key::W),
                back: i.key_down(egui::Key::S),
                left: i.key_down(egui::Key::A),
                right: i.key_down(egui::Key::D),
                up: i.key_down(egui::Key::E),
                down: i.key_down(egui::Key::Q),
                fast: i.modifiers.shift,
            });
            self.camera.fly_move(fly, dt);
        }
        self.camera.idle(dt);

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let ndc_x = (pos.x - rect.left()) / rect.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - rect.top()) / rect.height() * 2.0;
                let ray = self.camera.ray_through(ndc_x, ndc_y, aspect);
                let hit = self.layout.pick(&ray);
                // Clicking the same object again deselects it.
                self.selection = if hit == self.selection { None } else { hit };
            }
        }

        // -- mode switcher overlay -------------------------------------
        egui::Area::new(egui::Id::new("scene_mode_overlay"))
            .fixed_pos(rect.min + egui::vec2(10.0, 10.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::window(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let orbit = self.camera.mode == CameraMode::Orbit;
                        if ui.selectable_label(orbit, "orbit").clicked() {
                            self.camera.set_mode(CameraMode::Orbit);
                        }
                        if ui.selectable_label(!orbit, "fly").clicked() {
                            self.camera.set_mode(CameraMode::Fly);
                        }
                        if orbit {
                            ui.checkbox(&mut self.camera.auto_orbit, "auto");
                        } else {
                            ui.weak("WASD + QE, drag to look, shift = fast");
                        }
                    });
                });
            });

        // -- frame data -------------------------------------------------
        let light = sun::sun_at(state.unix_time_s());
        let nightness = (1.0 - light.daylight).powf(1.4);
        let bg = self.style.background;
        let mixv = |a: f32, b: f32| a * (1.0 - nightness * 0.88) + b * nightness * 0.88;
        let scene_bg = [
            mixv(bg[0], bg[0] * 0.12 + 0.015),
            mixv(bg[1], bg[1] * 0.12 + 0.025),
            mixv(bg[2], bg[2] * 0.12 + 0.06),
        ];
        let amb = 0.28 + 0.72 * light.daylight;
        let anim_s = ui.input(|i| i.time) as f32;

        let mut objects = Vec::with_capacity(self.static_objects.len() + 4096 * instances::FPI);
        objects.extend_from_slice(&self.static_objects);
        instances::build_dynamic(
            &mut objects,
            &DynamicInput {
                state,
                layout: &self.layout,
                style: &self.style,
                palette: &self.palette,
                nightness,
                anim_s,
                selection: self.selection,
            },
        );

        let frame = FrameData {
            ground: self.ground.clone(),
            objects,
            view_proj: self.camera.view_proj(aspect),
            eye: self.camera.eye(),
            light_dir: light.dir,
            light_color: light.color,
            sky: [
                self.style.sky[0] * amb,
                self.style.sky[1] * amb,
                self.style.sky[2] * amb * 1.04,
            ],
            ground_light: [
                self.style.ground_light[0] * amb,
                self.style.ground_light[1] * amb,
                self.style.ground_light[2] * amb,
            ],
            fog_color: scene_bg,
            fog_range: FOG_RANGE,
        };

        let renderer = Arc::clone(&self.renderer);
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(CallbackFn::new(move |info, painter| {
                if let Ok(mut r) = renderer.lock() {
                    r.paint(painter.gl(), painter.intermediate_fbo(), &info, &frame);
                }
            })),
        });
    }
}
