//! The scenario section: the library to pick a case from, and the
//! progress strip of the case being played.
//!
//! Panel vocabulary only. The viewer turns the scenario engine's types into
//! the plain rows below, so the panel stays buildable without the `sim`
//! feature and never names a `bess-scenario` type.

use egui::{Color32, RichText, Sense, Stroke};

use crate::clock;
use crate::ViewerCommand;

use super::PanelState;

/// One case in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseListing {
    /// The case's name, which is also what starts it.
    pub name: &'static str,
    /// Its taxonomy tag as the file spells it.
    pub taxonomy: &'static str,
    /// Its stated length, seconds.
    pub length_s: Option<u64>,
    /// What it demonstrates: the file's header comment.
    pub headline: String,
    /// Events in the file.
    pub events: usize,
    /// Of those, the ones that act on MQTT and Modbus only.
    pub wire_events: usize,
}

/// The case being played, fixed when it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryView {
    /// The case's name.
    pub name: String,
    /// Its header comment, or empty for a file handed in from outside.
    pub headline: String,
    /// Its taxonomy tag, if it states one.
    pub taxonomy: Option<&'static str>,
    /// Its stated length, seconds.
    pub length_s: Option<u64>,
    /// The schedule in firing order.
    pub rows: Vec<StoryRow>,
}

/// One scheduled event, as the strip lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryRow {
    /// Offset from the start, seconds.
    pub at_s: u64,
    /// Target and fault as the file spells them, with a drawn rate.
    pub what: String,
    /// A data fault: it acts on MQTT and Modbus only, which the browser
    /// does not have.
    pub wire_only: bool,
}

/// Where the case is this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryNow {
    /// Seconds since the start.
    pub elapsed_s: u64,
    /// Events fired so far.
    pub fired: usize,
}

/// Note on a data fault, which the browser lists and does not execute.
const WIRE_NOTE: &str = "on MQTT and Modbus only; this view has neither, so it shows nothing here";

/// The section: the strip when a case plays, then the library.
pub fn section(
    ui: &mut egui::Ui,
    library: &[CaseListing],
    story: Option<(&StoryView, StoryNow)>,
    ui_state: &PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    egui::CollapsingHeader::new(RichText::new("Scenarios").strong())
        .id_salt("scenario_section")
        .default_open(true)
        .show(ui, |ui| {
            if let Some((view, now)) = story {
                strip(ui, view, now, ui_state, commands);
                ui.add_space(6.0);
            }
            picker(ui, library, story.is_none(), ui_state, commands);
        });
}

fn strip(
    ui: &mut egui::Ui,
    view: &StoryView,
    now: StoryNow,
    ui_state: &PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.strong(&view.name);
        if let Some(tag) = view.taxonomy {
            ui.weak(tag);
        }
    });
    if !view.headline.is_empty() {
        ui.label(RichText::new(&view.headline).small());
    }
    let total = view.rows.len();
    let over = view.length_s.is_some_and(|l| now.elapsed_s >= l);
    ui.label(match view.length_s {
        Some(l) => format!(
            "{} of {}, {} of {total} events fired{}",
            clock::format_offset(now.elapsed_s),
            clock::format_offset(l),
            now.fired,
            if over { "; the story is over" } else { "" }
        ),
        None => format!(
            "{}, {} of {total} events fired",
            clock::format_offset(now.elapsed_s),
            now.fired
        ),
    });
    timeline(ui, view, now);
    let next = view.rows.get(now.fired);
    if let Some(row) = next {
        ui.label(format!(
            "next: {} {}",
            clock::format_offset(row.at_s),
            row.what
        ));
    }
    if ui_state.jump_progress.is_none() {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(next.is_some(), egui::Button::new("Run to next event"))
                .on_hover_text("Lands on the event's tick; it fires on the next frame")
                .clicked()
            {
                commands.push(ViewerCommand::RunToNextEvent);
            }
            let can_end = view.length_s.is_some() && !over;
            if ui
                .add_enabled(can_end, egui::Button::new("Run to end"))
                .clicked()
            {
                commands.push(ViewerCommand::RunToEnd);
            }
        });
    }
    egui::CollapsingHeader::new(format!("Schedule ({total} events)"))
        .id_salt("scenario_schedule")
        .show(ui, |ui| {
            egui::Grid::new("scenario_rows")
                .striped(true)
                .num_columns(3)
                .show(ui, |ui| {
                    for (i, row) in view.rows.iter().enumerate() {
                        ui.label(clock::format_offset(row.at_s));
                        if row.wire_only {
                            ui.weak(&row.what).on_hover_text(WIRE_NOTE);
                        } else {
                            ui.label(&row.what);
                        }
                        if i < now.fired {
                            ui.label("fired");
                        } else {
                            ui.weak("pending");
                        }
                        ui.end_row();
                    }
                });
            if view.rows.iter().any(|r| r.wire_only) {
                ui.weak(RichText::new(format!("Greyed events act {WIRE_NOTE}.")).small());
            }
        });
}

/// The schedule on a time axis, with a cursor at now.
fn timeline(ui: &mut egui::Ui, view: &StoryView, now: StoryNow) {
    let last = view.rows.iter().map(|r| r.at_s).max().unwrap_or(0);
    let span = view.length_s.unwrap_or(last).max(last).max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let x_at = |s: u64| rect.left() + rect.width() * (s as f32 / span).min(1.0);
    painter.line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, visuals.weak_text_color()),
    );
    for (i, row) in view.rows.iter().enumerate() {
        let x = x_at(row.at_s);
        let color = if row.wire_only {
            visuals.weak_text_color()
        } else if i < now.fired {
            visuals.strong_text_color()
        } else {
            Color32::from_rgb(255, 194, 8)
        };
        painter.line_segment(
            [
                egui::pos2(x, rect.top() + 3.0),
                egui::pos2(x, rect.bottom() - 3.0),
            ],
            Stroke::new(2.0, color),
        );
    }
    let x = x_at(now.elapsed_s);
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        Stroke::new(2.0, visuals.selection.bg_fill),
    );
}

fn picker(
    ui: &mut egui::Ui,
    library: &[CaseListing],
    open: bool,
    ui_state: &PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    egui::CollapsingHeader::new(format!("Library ({} cases)", library.len()))
        .id_salt("scenario_library")
        .default_open(open)
        .show(ui, |ui| {
            ui.weak(
                RichText::new("Starting a case throws the plant away and builds the file's run.")
                    .small(),
            );
            let mut group = "";
            egui::Grid::new("scenario_library_rows")
                .num_columns(3)
                .show(ui, |ui| {
                    for case in library {
                        if case.taxonomy != group {
                            group = case.taxonomy;
                            ui.strong(group);
                            ui.end_row();
                        }
                        let start = ui.add_enabled(
                            ui_state.jump_progress.is_none(),
                            egui::Button::new("Start").small(),
                        );
                        if start.clicked() {
                            commands.push(ViewerCommand::StartScenario(case.name));
                        }
                        ui.label(case.name).on_hover_text(&case.headline);
                        let length = case.length_s.map_or(String::new(), clock::format_length);
                        if case.wire_events == case.events && case.events > 0 {
                            ui.weak(format!("{length}, on the wire"))
                                .on_hover_text(format!("Every event acts {WIRE_NOTE}."));
                        } else if case.wire_events > 0 {
                            ui.weak(format!("{length}, {} on the wire", case.wire_events))
                                .on_hover_text(format!(
                                    "{} of {} events act {WIRE_NOTE}.",
                                    case.wire_events, case.events
                                ));
                        } else {
                            ui.weak(length);
                        }
                        ui.end_row();
                    }
                });
        });
}
