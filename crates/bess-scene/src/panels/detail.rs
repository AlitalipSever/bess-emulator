//! The detail panel of whatever is selected in the scene: a container and
//! its racks, a converter skid, or the substation.
//!
//! Its own file because it answers a different question from the site panel.
//! That one is "how is the plant", this one is "what is this thing I clicked".
//!
//! Each object lists the alarms it holds, the other half of the link from a
//! row to the scene, and offers the operator reset for its scope: a rack on
//! its row, a block on its PCS skid. The site reset is in the alarm section.

use bess_core::alarms::{AlarmNode, ResetScope};
use bess_core::state::{HvacMode, SiteState};
use egui::RichText;

use crate::alarms::{self, link, Tone};
use crate::layout::Selection;
use crate::ViewerCommand;

use super::alarms::color;
use super::{mw, power_color};

/// The set bits of one word, in the tone of the loudest, or a quiet dash.
fn alarm_cell(ui: &mut egui::Ui, node: AlarmNode, word: u32) {
    match Tone::worst(node, word) {
        Some(tone) => ui.colored_label(color(tone), link::short_names(node, word)),
        None => ui.weak("-"),
    };
}

/// A reset button for a scope, enabled only while it has a latched bit.
fn reset_button(
    ui: &mut egui::Ui,
    state: &SiteState,
    scope: ResetScope,
    label: &str,
    commands: &mut Vec<ViewerCommand>,
) {
    if ui
        .add_enabled(
            alarms::latched(state, scope),
            egui::Button::new(label).small(),
        )
        .on_disabled_hover_text("Nothing is latched")
        .clicked()
    {
        commands.push(ViewerCommand::ResetAlarms(scope));
    }
}

/// Draw whatever is selected: a container and its racks, a converter skid,
/// or the substation. The only commands are the resets.
pub fn selection_detail(
    ui: &mut egui::Ui,
    state: &SiteState,
    sel: Selection,
    commands: &mut Vec<ViewerCommand>,
) {
    match sel {
        Selection::Container { block, container } => {
            let Some(cont) = state
                .blocks
                .get(block)
                .and_then(|b| b.containers.get(container))
            else {
                return;
            };
            ui.strong(format!("Block {block:02} / container {container} (BMS)"));
            egui::Grid::new("cont_kpis").num_columns(2).show(ui, |ui| {
                ui.label("Air temperature");
                ui.label(format!("{:.1} \u{b0}C", cont.air_temp_c));
                ui.end_row();
                ui.label("HVAC");
                ui.label(if cont.hvac.failed {
                    "failed".to_owned()
                } else {
                    match cont.hvac.mode {
                        HvacMode::Off => "standby".to_owned(),
                        HvacMode::Cool1 => {
                            format!("cooling, 1 unit ({:.0} kW)", cont.hvac.thermal_w / 1000.0)
                        }
                        HvacMode::Cool2 => {
                            format!("cooling, 2 units ({:.0} kW)", cont.hvac.thermal_w / 1000.0)
                        }
                        HvacMode::Heat => {
                            format!("heating ({:.0} kW)", -cont.hvac.thermal_w / 1000.0)
                        }
                    }
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    egui::Grid::new("racks")
                        .striped(true)
                        .num_columns(6)
                        .show(ui, |ui| {
                            ui.strong("rack");
                            ui.strong("SoC");
                            ui.strong("T cell");
                            ui.strong("I");
                            ui.strong("alarms");
                            ui.label("");
                            ui.end_row();
                            for (i, rack) in cont.racks.iter().enumerate() {
                                let node = AlarmNode::Rack {
                                    block,
                                    container,
                                    rack: i,
                                };
                                ui.label(format!("{i:02}"));
                                ui.label(format!("{:.1} %", rack.soc_reported * 100.0));
                                ui.label(format!("{:.1} \u{b0}C", rack.cell_temp_c));
                                ui.label(format!("{:+.0} A", rack.current_a));
                                alarm_cell(ui, node, rack.alarm_bits);
                                let scope = ResetScope::Rack {
                                    block,
                                    container,
                                    rack: i,
                                };
                                reset_button(ui, state, scope, "reset", commands);
                                ui.end_row();
                            }
                        });
                });
        }
        Selection::Pcs { block } => {
            let Some(b) = state.blocks.get(block) else {
                return;
            };
            ui.strong(format!("Block {block:02} / PCS"));
            egui::Grid::new("pcs_kpis").num_columns(2).show(ui, |ui| {
                ui.label("State");
                ui.label(b.pcs.op_state.name());
                ui.end_row();
                ui.label("Block alarms");
                alarm_cell(ui, AlarmNode::Block { block }, b.alarm_bits);
                ui.end_row();
                ui.label("AC setpoint");
                ui.label(mw(b.pcs.p_ac_setpoint_w));
                ui.end_row();
                ui.label("Followed");
                ui.label(mw(b.pcs.setpoint_followed_w));
                ui.end_row();
                ui.label("AC power");
                ui.colored_label(power_color(b.pcs.p_ac_w), mw(b.pcs.p_ac_w));
                ui.end_row();
                ui.label("DC power");
                ui.label(mw(b.pcs.p_dc_w));
                ui.end_row();
                ui.label("Loss");
                ui.label(format!("{:.0} kW", b.pcs.loss_w / 1.0e3));
                ui.end_row();
                ui.label("Temperature");
                ui.label(format!("{:.1} °C", b.pcs.temperature_c));
                ui.end_row();
                ui.label("Overload budget");
                ui.label(format!("{:.0} %", b.pcs.overload_budget * 100.0));
                ui.end_row();
            });
            reset_button(ui, state, ResetScope::Block(block), "Reset block", commands);
        }
        Selection::Transformer => {
            let sub = &state.substation;
            ui.strong("Substation / main transformer");
            egui::Grid::new("sub_kpis").num_columns(2).show(ui, |ui| {
                ui.label("Site alarms");
                alarm_cell(ui, AlarmNode::Site, state.alarm_bits);
                ui.end_row();
                ui.label("HV breaker");
                ui.label(match sub.hv_breaker {
                    bess_core::state::BreakerState::Closed => "closed",
                    bess_core::state::BreakerState::Open => "open",
                });
                ui.end_row();
                ui.label("POI voltage");
                ui.label(format!("{:.1} kV", sub.poi_voltage_kv));
                ui.end_row();
                ui.label("Transformer loss");
                ui.label(format!("{:.0} kW", sub.transformer_loss_w / 1.0e3));
                ui.end_row();
                ui.label("Auxiliaries");
                ui.label(format!("{:.0} kW", sub.aux_power_w / 1.0e3));
                ui.end_row();
            });
        }
    }
    ui.add_space(4.0);
    ui.weak(RichText::new("Click the object again to deselect.").small());
}
