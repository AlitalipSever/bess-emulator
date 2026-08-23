//! egui panels: the site overview with its controls, and the detail panel of
//! whatever is selected in the scene. Panels read `&SiteState` and emit
//! [`ViewerCommand`]s; they never touch the kernel.
//!
//! - [`site`]     readouts: what the plant and the weather are doing
//! - [`controls`] the parts that issue commands
//! - [`detail`]   the selected object

pub mod controls;
pub mod detail;
pub mod site;

use bess_core::state::SiteState;
use egui::Color32;

use crate::clock;
use crate::layout::Selection;
use crate::scenery::Scenery;
use crate::ViewerCommand;

/// A day worth jumping to, and what the button says.
///
/// The type lives here rather than with the code that derives it, because a
/// label and a date are panel vocabulary and the panel must stay buildable
/// without the `sim` feature. `viewer::presets` produces these from the
/// compiled weather year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop {
    /// What the button says.
    pub label: &'static str,
    /// Month of the year, 1 to 12.
    pub month: u32,
    /// Day of that month.
    pub day: u32,
    /// Hour of day the jump lands on.
    pub hour: u32,
}

/// Everything the panel draws from this frame.
pub struct PanelInput<'a> {
    /// The plant.
    pub state: &'a SiteState,
    /// What the sky is doing.
    pub scenery: &'a Scenery,
    /// Days the dataset suggests, if any.
    pub stops: &'a [Stop],
    /// The object selected in the scene.
    pub selection: Option<Selection>,
}

/// UI scratch state that outlives a frame: slider positions, and where the
/// date fields are pointing.
pub struct PanelState {
    /// Time acceleration slider value.
    pub speed: f64,
    /// Setpoint slider value, MW (positive = discharge).
    pub setpoint_mw: f64,
    /// Month the date fields are showing.
    pub month: u32,
    /// Day the date fields are showing.
    pub day: u32,
    /// Hour a jump lands on.
    pub hour: u32,
    /// Progress of a jump in progress, if one is running. The viewer sets
    /// this; the panel only shows it, and hides the date controls while it
    /// is set so a second jump cannot be started on top of the first.
    pub jump_progress: Option<f32>,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            speed: 60.0,
            setpoint_mw: 0.0,
            month: 7,
            day: 14,
            hour: 9,
            jump_progress: None,
        }
    }
}

/// Megawatts, signed.
pub(crate) fn mw(value_w: f64) -> String {
    format!("{:+.2} MW", value_w / 1.0e6)
}

/// Discharging reads warm, charging reads cool, idle reads grey.
pub(crate) fn power_color(value_w: f64) -> Color32 {
    if value_w > 0.5e6 {
        Color32::from_rgb(255, 168, 61) // discharging
    } else if value_w < -0.5e6 {
        Color32::from_rgb(51, 191, 158) // charging
    } else {
        Color32::GRAY
    }
}

/// Right-hand side panel. Returns the commands the user issued this frame.
/// Call before the central panel (panels wrap outside-in in egui).
pub fn side_panel(
    ui: &mut egui::Ui,
    input: &PanelInput,
    ui_state: &mut PanelState,
) -> Vec<ViewerCommand> {
    let mut commands = Vec::new();
    let now = input.state.unix_time_s();
    egui::Panel::right(egui::Id::new("site_panel"))
        .default_size(330.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading(&input.state.meta.site_id);
            ui.label(clock::format_utc(now));
            ui.separator();

            site::electrical(ui, input.state);
            ui.separator();
            site::weather_and_thermal(ui, input.state, input.scenery);

            ui.separator();
            controls::dispatch(ui, ui_state, &mut commands);

            ui.separator();
            controls::time_travel(ui, now, ui_state, &mut commands);
            controls::stops(ui, now, input.stops, ui_state, &mut commands);

            ui.separator();
            match input.selection {
                None => ui.weak("Click a container, a PCS skid, or the transformer."),
                Some(sel) => {
                    detail::selection_detail(ui, input.state, sel);
                    ui.label("")
                }
            };
        });
    commands
}
