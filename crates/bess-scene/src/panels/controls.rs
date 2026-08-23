//! The controls: everything in the panel that issues a command.
//!
//! Separated from the readouts in [`super::site`] because the split is the
//! crate's design rule made visible. Those functions read the state tree;
//! these emit [`ViewerCommand`]s and touch nothing.

use egui::Slider;

use crate::clock::{civil_from_unix, unix_from_civil};
use crate::ViewerCommand;

use super::{PanelState, Stop};

/// Time acceleration and the external setpoint.
pub fn dispatch(ui: &mut egui::Ui, ui_state: &mut PanelState, commands: &mut Vec<ViewerCommand>) {
    ui.label("Time acceleration");
    if ui
        .add(
            Slider::new(&mut ui_state.speed, 1.0..=3600.0)
                .logarithmic(true)
                .suffix("x"),
        )
        .changed()
    {
        commands.push(ViewerCommand::SetSpeed(ui_state.speed));
    }
    ui.add_space(4.0);
    ui.label("External setpoint (positive = discharge)");
    ui.add(Slider::new(&mut ui_state.setpoint_mw, -100.0..=100.0).suffix(" MW"));
    ui.horizontal(|ui| {
        if ui.button("Write setpoint").clicked() {
            commands.push(ViewerCommand::SetSetpoint(Some(
                ui_state.setpoint_mw * 1.0e6,
            )));
        }
        if ui.button("Follow plan").clicked() {
            commands.push(ViewerCommand::SetSetpoint(None));
        }
    });
}

/// Going somewhere else in the year.
pub fn time_travel(
    ui: &mut egui::Ui,
    now_unix_s: i64,
    ui_state: &mut PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    if let Some(progress) = ui_state.jump_progress {
        ui.label("Running forward");
        ui.add(egui::ProgressBar::new(progress).show_percentage());
        return;
    }

    let (year, ..) = civil_from_unix(now_unix_s);
    ui.horizontal(|ui| {
        ui.label("Go to");
        ui.add(Slider::new(&mut ui_state.month, 1..=12).prefix("m "));
        ui.add(Slider::new(&mut ui_state.day, 1..=31).prefix("d "));
    });
    let target = unix_from_civil(year, ui_state.month, ui_state.day, ui_state.hour, 0, 0);
    ui.horizontal(|ui| {
        if ui.button("Restart there").clicked() {
            commands.push(ViewerCommand::RestartAt(target));
        }
        let forwards = target > now_unix_s;
        if ui
            .add_enabled(forwards, egui::Button::new("Run forward to it"))
            .clicked()
        {
            commands.push(ViewerCommand::FastForwardTo(target));
        }
    });
    ui.weak(
        egui::RichText::new(if target > now_unix_s {
            "Running forward keeps the plant's state; restarting throws it away."
        } else {
            "That date is behind the plant. There is no rewind, so going back means \
             restarting there."
        })
        .small(),
    );
}

/// One-click days worth watching, derived from the dataset.
pub fn stops(
    ui: &mut egui::Ui,
    now_unix_s: i64,
    offered: &[Stop],
    ui_state: &mut PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    if offered.is_empty() || ui_state.jump_progress.is_some() {
        return;
    }
    let (year, ..) = civil_from_unix(now_unix_s);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for stop in offered {
            if ui.button(stop.label).clicked() {
                ui_state.month = stop.month;
                ui_state.day = stop.day;
                ui_state.hour = stop.hour;
                commands.push(ViewerCommand::RestartAt(unix_from_civil(
                    year, stop.month, stop.day, stop.hour, 0, 0,
                )));
            }
        }
    });
}
