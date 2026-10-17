//! The alarm section: a summary of what is active and a journal of what
//! happened, in the shape the alarm-management standards give the two
//! displays (ISA-18.2 and IEC 62682 name them summary and journal).
//!
//! The summary is derived from the words this frame
//! ([`crate::alarms::active`]); the journal is the viewer's ring of the
//! events it saw. Everything this file claims is computed in
//! [`crate::alarms`] and tested there; this file only draws it.

use bess_core::kernel::Event;
use bess_core::state::SiteState;
use egui::{Color32, RichText};

use crate::alarms::{self, row, Entry, Journal, Tone};
use crate::clock;
use crate::style::Palette;

use super::PanelState;

/// Rows shown before a table scrolls.
const VISIBLE_ROWS: f32 = 8.0;

fn color(tone: Tone) -> Color32 {
    let [r, g, b] = tone.color(&Palette::default());
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// The collapsible alarm section.
pub fn section(ui: &mut egui::Ui, state: &SiteState, journal: &Journal, ui_state: &mut PanelState) {
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

fn summary(
    ui: &mut egui::Ui,
    active: &[alarms::ActiveAlarm],
    journal: &Journal,
    ui_state: &PanelState,
) {
    let shown: Vec<_> = active
        .iter()
        .copied()
        .filter(|a| ui_state.alarm_filter.admits_alarm(*a))
        .collect();
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
                        ui.label(alarm.node.path());
                        ui.label(alarm.name());
                        match journal.raised_at(alarm.node, alarm.bit) {
                            Some(t) => ui.label(clock::format_row(t)),
                            None => ui.weak("before the journal"),
                        };
                        ui.end_row();
                    }
                });
        });
}

fn journal_table(ui: &mut egui::Ui, journal: &Journal, ui_state: &PanelState) {
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
                        journal_row(ui, entry);
                        ui.end_row();
                    }
                });
        });
}

fn journal_row(ui: &mut egui::Ui, entry: &Entry) {
    let event = &entry.event;
    ui.weak(entry.seq.to_string());
    ui.label(clock::format_row(entry.unix_time_s));
    ui.label(row::node_path(event));
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
}
