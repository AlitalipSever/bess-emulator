//! The alarm section: a summary of what is active and a journal of what
//! happened, in the shape the alarm-management standards give the two
//! displays (ISA-18.2 and IEC 62682 name them summary and journal).
//!
//! A node in either table is a link that selects the object holding it in
//! the scene; the site reset sits in the section header, the block and rack
//! resets on the selected object ([`super::detail`]).
//!
//! The summary is derived from the words this frame
//! ([`crate::alarms::active`]); the journal is the viewer's ring of the
//! events it saw. Everything this file claims is computed in
//! [`crate::alarms`] and tested there; this file only draws it.

use bess_core::alarms::ResetScope;
use bess_core::kernel::{Event, FaultAction};
use bess_core::state::SiteState;
use egui::{Color32, RichText};

use crate::alarms::{self, link, row, Entry, Journal, Tone};
use crate::clock;
use crate::style::Palette;
use crate::ViewerCommand;

use super::{PanelState, ResetAnswer};

/// Rows shown before a table scrolls.
const VISIBLE_ROWS: f32 = 8.0;

pub(crate) fn color(tone: Tone) -> Color32 {
    let [r, g, b] = tone.color(&Palette::default());
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// The collapsible alarm section.
pub fn section(
    ui: &mut egui::Ui,
    state: &SiteState,
    journal: &Journal,
    ui_state: &mut PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    let active = alarms::active(state);
    let count = state.event_log.count;
    let unseen = ui_state.alarms_seen.unseen(count);
    let worst = active.iter().map(|a| a.tone()).max();
    let mut title = RichText::new(match active.len() {
        0 => "Alarms: none active".to_owned(),
        1 => "Alarms: 1 active".to_owned(),
        n => format!("Alarms: {n} active"),
    })
    .strong();
    if let Some(tone) = worst {
        title = title.color(color(tone));
    }
    let header = egui::CollapsingHeader::new(title)
        .id_salt("alarm_section")
        .default_open(true)
        .show(ui, |ui| {
            filters(ui, state.blocks.len(), ui_state);
            ui.add_space(4.0);
            reset_site(ui, state, ui_state, commands);
            summary(ui, &active, journal, ui_state);
            ui.add_space(6.0);
            journal_table(ui, journal, ui_state);
        });
    if header.body_returned.is_some() {
        // Open and drawn: everything up to now has been in front of them.
        ui_state.alarms_seen.mark(count);
    } else if unseen > 0 {
        ui.weak(format!("{unseen} events since you last looked"));
    }
}

fn filters(ui: &mut egui::Ui, blocks: usize, ui_state: &mut PanelState) {
    let f = &mut ui_state.alarm_filter;
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut f.trips, "trips");
        ui.checkbox(&mut f.warnings, "warnings");
        ui.checkbox(&mut f.alarms_only, "alarms only");
        egui::ComboBox::from_id_salt("alarm_block_filter")
            .selected_text(
                f.block
                    .map_or("all blocks".to_owned(), |b| format!("block{b:02}")),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.block, None, "all blocks");
                for b in 0..blocks {
                    ui.selectable_value(&mut f.block, Some(b), format!("block{b:02}"));
                }
            });
    });
}

/// The site reset, and what the last reset answered.
fn reset_site(
    ui: &mut egui::Ui,
    state: &SiteState,
    ui_state: &PanelState,
    commands: &mut Vec<ViewerCommand>,
) {
    ui.horizontal(|ui| {
        let latched = alarms::latched(state, ResetScope::Site);
        if ui
            .add_enabled(latched, egui::Button::new("Reset site"))
            .on_hover_text("Clear every latched bit on site and take every PCS out of fault")
            .on_disabled_hover_text("Nothing is latched")
            .clicked()
        {
            commands.push(ViewerCommand::ResetAlarms(ResetScope::Site));
        }
        if let Some(answer) = &ui_state.reset_answer {
            reset_answer(ui, answer);
        }
    });
}

fn reset_answer(ui: &mut egui::Ui, answer: &ResetAnswer) {
    let scope = FaultAction::AlarmReset(answer.scope).node_path();
    let at = clock::format_row(answer.unix_time_s);
    let text = match &answer.outcome {
        Ok(still) if still.is_empty() => format!("{scope} reset at {at}, nothing still present"),
        Ok(still) => {
            let names: Vec<String> = still.iter().map(|(n, b)| n.alarm_name(*b)).collect();
            format!(
                "{scope} reset at {at}; still present, raising again: {}",
                names.join(", ")
            )
        }
        Err(why) => format!("{scope} reset refused: {why}"),
    };
    ui.label(RichText::new(text).small());
}

fn summary(
    ui: &mut egui::Ui,
    active: &[alarms::ActiveAlarm],
    journal: &Journal,
    ui_state: &mut PanelState,
) {
    let shown: Vec<_> = active
        .iter()
        .copied()
        .filter(|a| ui_state.alarm_filter.admits_alarm(*a))
        .collect();
    let mut select = None;
    ui.label(if shown.len() == active.len() {
        "Active now".to_owned()
    } else {
        format!("Active now, {} of {} shown", shown.len(), active.len())
    });
    if shown.is_empty() {
        ui.weak("Nothing in alarm.");
        return;
    }
    let row_h = ui.spacing().interact_size.y;
    egui::ScrollArea::vertical()
        .id_salt("alarm_summary")
        .max_height(row_h * VISIBLE_ROWS)
        .show_rows(ui, row_h, shown.len(), |ui, range| {
            egui::Grid::new("alarm_summary_rows")
                .striped(true)
                .num_columns(4)
                .start_row(range.start)
                .show(ui, |ui| {
                    for alarm in &shown[range] {
                        ui.colored_label(color(alarm.tone()), alarm.severity().name());
                        if ui.link(alarm.node.path()).clicked() {
                            select = Some(link::object_of(alarm.node));
                        }
                        ui.label(alarm.name());
                        match journal.raised_at(alarm.node, alarm.bit) {
                            Some(t) => ui.label(clock::format_row(t)),
                            None => ui.weak("before the journal"),
                        };
                        ui.end_row();
                    }
                });
        });
    if select.is_some() {
        ui_state.select_request = select;
    }
}

fn journal_table(ui: &mut egui::Ui, journal: &Journal, ui_state: &mut PanelState) {
    let shown: Vec<&Entry> = journal
        .iter_newest_first()
        .filter(|e| ui_state.alarm_filter.admits_event(&e.event))
        .collect();
    ui.label(match journal.dropped() {
        0 => "Journal, newest first".to_owned(),
        n => format!("Journal, newest first ({n} older events dropped)"),
    });
    if shown.is_empty() {
        ui.weak("No events yet.");
        return;
    }
    let mut select = None;
    let row_h = ui.spacing().interact_size.y;
    egui::ScrollArea::vertical()
        .id_salt("alarm_journal")
        .max_height(row_h * VISIBLE_ROWS)
        .show_rows(ui, row_h, shown.len(), |ui, range| {
            egui::Grid::new("alarm_journal_rows")
                .striped(true)
                .num_columns(5)
                .start_row(range.start)
                .show(ui, |ui| {
                    for entry in &shown[range] {
                        if journal_row(ui, entry) {
                            select = Some(link::object_of_event(&entry.event));
                        }
                        ui.end_row();
                    }
                });
        });
    if select.is_some() {
        ui_state.select_request = select;
    }
}

/// One journal row; true when its node was clicked.
fn journal_row(ui: &mut egui::Ui, entry: &Entry) -> bool {
    let event = &entry.event;
    ui.weak(entry.seq.to_string());
    ui.label(clock::format_row(entry.unix_time_s));
    let clicked = ui.link(row::node_path(event)).clicked();
    let kind = row::kind(event);
    let what = row::what(event);
    match *event {
        Event::AlarmRaised { node, bit, .. } => {
            ui.colored_label(color(Tone::of(node, bit)), kind);
            ui.colored_label(color(Tone::of(node, bit)), what);
        }
        Event::AlarmCleared { .. } => {
            ui.weak(kind);
            ui.weak(what);
        }
        _ => {
            ui.label(kind);
            ui.label(what);
        }
    }
    clicked
}
