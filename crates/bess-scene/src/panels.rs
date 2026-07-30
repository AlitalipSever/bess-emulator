//! egui panels: the site overview with controls, and the detail panel of
//! whatever is selected in the scene. Panels read `&SiteState` and emit
//! [`ViewerCommand`]s; they never touch the kernel.

use bess_core::state::{EmsMode, PcsOpState, SiteState};
use egui::{Color32, ProgressBar, RichText, Slider};

use crate::layout::Selection;
use crate::sun;
use crate::ViewerCommand;

/// UI scratch state that outlives a frame (slider positions).
pub struct PanelState {
    /// Time acceleration slider value.
    pub speed: f64,
    /// Setpoint slider value, MW (positive = discharge).
    pub setpoint_mw: f64,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            speed: 60.0,
            setpoint_mw: 0.0,
        }
    }
}

fn mw(value_w: f64) -> String {
    format!("{:+.2} MW", value_w / 1.0e6)
}

fn power_color(value_w: f64) -> Color32 {
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
    state: &SiteState,
    selection: Option<Selection>,
    ui_state: &mut PanelState,
) -> Vec<ViewerCommand> {
    let mut commands = Vec::new();
    egui::Panel::right(egui::Id::new("site_panel"))
        .default_size(330.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading(&state.meta.site_id);
            ui.label(sun::format_utc(state.unix_time_s()));
            ui.separator();

            // -- site KPIs -------------------------------------------
            let soc = state.average_soc() as f32;
            ui.label("State of charge");
            ui.add(ProgressBar::new(soc).text(format!("{:.1} %", soc * 100.0)));
            ui.add_space(4.0);

            egui::Grid::new("site_kpis").num_columns(2).show(ui, |ui| {
                let sub = &state.substation;
                ui.label("POI power");
                ui.colored_label(
                    power_color(sub.poi_active_power_w),
                    mw(sub.poi_active_power_w),
                );
                ui.end_row();
                ui.label("Setpoint");
                ui.label(mw(state.ems.site_setpoint_w));
                ui.end_row();
                ui.label("EMS mode");
                ui.label(match state.ems.mode {
                    EmsMode::FollowPlan => "internal plan",
                    EmsMode::External => "external",
                });
                ui.end_row();
                ui.label("Available");
                ui.label(format!(
                    "+{:.0} / -{:.0} MW",
                    state.ems.available_discharge_w / 1.0e6,
                    state.ems.available_charge_w / 1.0e6
                ));
                ui.end_row();
                ui.label("Frequency");
                ui.label(format!("{:.3} Hz", sub.frequency_hz));
                ui.end_row();
                ui.label("Ambient");
                ui.label(format!("{:.1} \u{b0}C", state.weather.ambient_c));
                ui.end_row();
                ui.label("Meters");
                ui.label(format!(
                    "\u{2191} {:.1} / \u{2193} {:.1} MWh",
                    sub.export_wh / 1.0e6,
                    sub.import_wh / 1.0e6
                ));
                ui.end_row();
            });

            // -- controls --------------------------------------------
            ui.separator();
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

            // -- selection detail ------------------------------------
            ui.separator();
            match selection {
                None => {
                    ui.weak("Click a container, a PCS skid, or the transformer.");
                }
                Some(sel) => selection_detail(ui, state, sel),
            }
        });
    commands
}

fn selection_detail(ui: &mut egui::Ui, state: &SiteState, sel: Selection) {
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
                ui.label(if cont.hvac.cooling_on {
                    "cooling"
                } else {
                    "standby"
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    egui::Grid::new("racks")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.strong("rack");
                            ui.strong("SoC");
                            ui.strong("T cell");
                            ui.strong("I");
                            ui.end_row();
                            for (i, rack) in cont.racks.iter().enumerate() {
                                ui.label(format!("{i:02}"));
                                ui.label(format!("{:.1} %", rack.soc * 100.0));
                                ui.label(format!("{:.1} \u{b0}C", rack.cell_temp_c));
                                ui.label(format!("{:+.0} A", rack.current_a));
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
                ui.label(match b.pcs.op_state {
                    PcsOpState::Standby => "standby",
                    PcsOpState::Run => "run",
                    PcsOpState::Fault => "fault",
                });
                ui.end_row();
                ui.label("AC setpoint");
                ui.label(mw(b.pcs.p_ac_setpoint_w));
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
            });
        }
        Selection::Transformer => {
            let sub = &state.substation;
            ui.strong("Substation / main transformer");
            egui::Grid::new("sub_kpis").num_columns(2).show(ui, |ui| {
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
