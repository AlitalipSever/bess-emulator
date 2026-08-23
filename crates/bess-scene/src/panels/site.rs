//! The site panel's readouts: what the plant is doing, and what the weather
//! is doing to it.
//!
//! Reads the state tree and the scenery; writes nothing and commands nothing.
//! The controls that do issue commands are in [`super::controls`].

use bess_core::state::{EmsMode, HvacMode, SiteState};
use egui::ProgressBar;

use crate::scenery::Scenery;

use super::{mw, power_color};

/// How many containers are cooling, and how many are heating.
///
/// A pure read of the tree, separated out so it can be tested: the panel that
/// displays it cannot be, and a count that quietly drifts from the fleet size
/// is the kind of thing nobody notices on a screenshot.
pub fn hvac_counts(state: &SiteState) -> (usize, usize, usize) {
    let mut cooling = 0;
    let mut heating = 0;
    let mut total = 0;
    for container in state.blocks.iter().flat_map(|b| b.containers.iter()) {
        total += 1;
        match container.hvac.mode {
            HvacMode::Cool1 | HvacMode::Cool2 => cooling += 1,
            HvacMode::Heat => heating += 1,
            HvacMode::Off => {}
        }
    }
    (cooling, heating, total)
}

/// Coldest and warmest container air on site, degrees Celsius.
pub fn container_air_min_max_c(state: &SiteState) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for container in state.blocks.iter().flat_map(|b| b.containers.iter()) {
        min = min.min(container.air_temp_c);
        max = max.max(container.air_temp_c);
    }
    if min > max {
        (0.0, 0.0)
    } else {
        (min, max)
    }
}

/// State of charge and the site's electrical readings.
pub fn electrical(ui: &mut egui::Ui, state: &SiteState) {
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
        ui.label("Meters");
        ui.label(format!(
            "\u{2191} {:.1} / \u{2193} {:.1} MWh",
            sub.export_wh / 1.0e6,
            sub.import_wh / 1.0e6
        ));
        ui.end_row();
    });
}

/// The weather the plant is standing in, and what it is doing to the boxes.
///
/// The M1 milestone gave the plant thermal memory and staged cooling, and
/// until now none of it was legible without opening a container. These six
/// rows are the chain: what the sky is doing, what the air inside is doing
/// about it, where the cells ended up, and how hard the cooling is working.
pub fn weather_and_thermal(ui: &mut egui::Ui, state: &SiteState, scenery: &Scenery) {
    let (air_min, air_max) = container_air_min_max_c(state);
    let (cell_min, cell_max) = state.cell_temp_min_max_c();
    let (cooling, heating, containers) = hvac_counts(state);

    egui::Grid::new("weather_kpis")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Ambient");
            ui.label(format!("{:.1} \u{b0}C", state.weather.ambient_c));
            ui.end_row();
            ui.label("Irradiance");
            ui.label(format!(
                "{:.0} W/m\u{b2} ({:.0}% of clear sky)",
                state.weather.irradiance_wm2,
                f64::from(scenery.dimming) * 100.0
            ));
            ui.end_row();
            ui.label("Cloud");
            ui.label(format!("{:.0} okta", f64::from(scenery.cloud) * 8.0));
            ui.end_row();
            ui.label("Container air");
            ui.label(format!("{air_min:.1} to {air_max:.1} \u{b0}C"));
            ui.end_row();
            ui.label("Cells");
            ui.label(format!("{cell_min:.1} to {cell_max:.1} \u{b0}C"));
            ui.end_row();
            ui.label("HVAC");
            ui.label(if heating > 0 {
                format!("{heating} of {containers} heating")
            } else {
                format!("{cooling} of {containers} cooling")
            });
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::{container_air_min_max_c, hvac_counts};
    use bess_core::state::HvacMode;
    use bess_core::{PlantConfig, SiteState};

    fn site() -> SiteState {
        SiteState::new(&PlantConfig::gw01(), 7, 1_767_225_600)
    }

    #[test]
    fn the_hvac_count_covers_every_container_on_site() {
        let mut state = site();
        let (cooling, heating, total) = hvac_counts(&state);
        assert_eq!(total, 40, "GW-01 has 40 containers");
        assert_eq!((cooling, heating), (0, 0), "a fresh plant is not running");

        // One of each mode, in different blocks, so a counter that reads only
        // the first block or only the first container comes up short.
        state.blocks[0].containers[0].hvac.mode = HvacMode::Cool1;
        state.blocks[7].containers[1].hvac.mode = HvacMode::Cool2;
        state.blocks[19].containers[1].hvac.mode = HvacMode::Heat;
        let (cooling, heating, total) = hvac_counts(&state);
        assert_eq!((cooling, heating, total), (2, 1, 40));
    }

    #[test]
    fn the_air_range_spans_the_whole_site() {
        let mut state = site();
        state.blocks[3].containers[1].air_temp_c = 31.5;
        state.blocks[18].containers[0].air_temp_c = 8.25;
        let (min, max) = container_air_min_max_c(&state);
        assert!((min - 8.25).abs() < 1e-9, "min was {min}");
        assert!((max - 31.5).abs() < 1e-9, "max was {max}");
    }
}
