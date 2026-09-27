//! Prometheus exposition: what `/metrics` publishes, family by family.
//!
//! Hand-written rather than generated from the signal map, per the
//! architecture's Prometheus rule: these families are operator views, not
//! register contracts, and they carry no map version.

use std::fmt::Write as _;

use bess_core::state::HvacMode;
use bess_core::SiteState;

use crate::sim::Snapshot;

/// Write one single-sample metric family.
fn metric(out: &mut String, name: &str, kind: &str, help: &str, value: f64) {
    let _ = writeln!(
        out,
        "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
    );
}

/// Write one metric family whose samples are separated by a single label.
fn labeled_metric(
    out: &mut String,
    name: &str,
    kind: &str,
    help: &str,
    label: &str,
    samples: &[(&str, f64)],
) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
    for (value_of_label, value) in samples {
        let _ = writeln!(out, "{name}{{{label}=\"{value_of_label}\"}} {value}");
    }
}

/// Build the Prometheus exposition for a snapshot.
///
/// Kept separate from the handler so the content is testable, and so the
/// dashboard shipped under `deploy/grafana` can be checked against the
/// metrics that actually exist rather than the ones someone remembers.
pub(super) fn metrics_body(snap: &Snapshot) -> String {
    let mut out = String::with_capacity(4096);
    site_metrics(&mut out, &snap.state, snap.speed);
    thermal_metrics(&mut out, &snap.state);
    house_load_metrics(&mut out, &snap.state);
    bms_metrics(&mut out, &snap.state);
    out
}

/// Grid-facing KPIs and the run's own bookkeeping.
fn site_metrics(out: &mut String, s: &SiteState, speed: f64) {
    metric(
        out,
        "bess_poi_active_power_watts",
        "gauge",
        "Active power at the POI (positive = export).",
        s.substation.poi_active_power_w,
    );
    metric(
        out,
        "bess_site_soc_ratio",
        "gauge",
        "Mean state of charge over in-service racks.",
        s.average_soc(),
    );
    metric(
        out,
        "bess_site_setpoint_watts",
        "gauge",
        "Site active-power target.",
        s.ems.site_setpoint_w,
    );
    metric(
        out,
        "bess_grid_frequency_hertz",
        "gauge",
        "Grid frequency at the POI.",
        s.substation.frequency_hz,
    );
    metric(
        out,
        "bess_import_watthours_total",
        "counter",
        "POI import meter.",
        s.substation.import_wh,
    );
    metric(
        out,
        "bess_export_watthours_total",
        "counter",
        "POI export meter.",
        s.substation.export_wh,
    );
    metric(
        out,
        "bess_sim_tick",
        "counter",
        "Simulation tick counter.",
        s.tick as f64,
    );
    metric(
        out,
        "bess_sim_speed",
        "gauge",
        "Time acceleration factor.",
        speed,
    );
}

/// The thermal chain M1 added: the weather that drives it, the container air
/// that answers it, the cells that lag it, and what the HVAC is doing about
/// all of it.
fn thermal_metrics(out: &mut String, s: &SiteState) {
    let containers: Vec<_> = s.blocks.iter().flat_map(|b| b.containers.iter()).collect();
    // A site with no containers is not a configuration that ships, but the
    // three temperatures here have to agree about what it would mean, and
    // `cell_temp_min_max_c` already answers zero.
    let (air_max_c, air_mean_c) = if containers.is_empty() {
        (0.0, 0.0)
    } else {
        let max = containers
            .iter()
            .map(|c| c.air_temp_c)
            .fold(f64::NEG_INFINITY, f64::max);
        let mean = containers.iter().map(|c| c.air_temp_c).sum::<f64>() / containers.len() as f64;
        (max, mean)
    };
    let mode_count =
        |mode: HvacMode| containers.iter().filter(|c| c.hvac.mode == mode).count() as f64;
    let (cell_min_c, cell_max_c) = s.cell_temp_min_max_c();

    metric(
        out,
        "bess_ambient_celsius",
        "gauge",
        "Ambient temperature.",
        s.weather.ambient_c,
    );
    metric(
        out,
        "bess_irradiance_watts_per_square_meter",
        "gauge",
        "Global horizontal irradiance driving the envelope solar gain.",
        s.weather.irradiance_wm2,
    );
    metric(
        out,
        "bess_container_air_max_celsius",
        "gauge",
        "Hottest container air temperature on site.",
        air_max_c,
    );
    metric(
        out,
        "bess_container_air_mean_celsius",
        "gauge",
        "Mean container air temperature over the site.",
        air_mean_c,
    );
    metric(
        out,
        "bess_cell_min_celsius",
        "gauge",
        "Coldest representative cell temperature on site.",
        cell_min_c,
    );
    metric(
        out,
        "bess_cell_max_celsius",
        "gauge",
        "Hottest representative cell temperature on site.",
        cell_max_c,
    );
    labeled_metric(
        out,
        "bess_hvac_containers",
        "gauge",
        "Containers in each HVAC mode.",
        "mode",
        &[
            ("off", mode_count(HvacMode::Off)),
            ("cool1", mode_count(HvacMode::Cool1)),
            ("cool2", mode_count(HvacMode::Cool2)),
            ("heat", mode_count(HvacMode::Heat)),
        ],
    );
}

/// The house load and the loss waterfall: what the site spent on itself and
/// where the rest of the gap between nameplate and meter went.
fn house_load_metrics(out: &mut String, s: &SiteState) {
    let aux = &s.aux;
    let items = &s.energy.aux_items;

    metric(
        out,
        "bess_aux_metered_power_watts",
        "gauge",
        "House load as the substation meters it, accumulated on its own path; \
         the itemized gauges have to add up to this.",
        s.substation.aux_power_w,
    );
    labeled_metric(
        out,
        "bess_aux_power_watts",
        "gauge",
        "House load by the item that drew it; the items sum to the metered total.",
        "item",
        &[
            ("hvac", aux.hvac_w),
            ("bms", aux.bms_w),
            ("pcs_standby", aux.pcs_standby_w),
            ("controls", aux.controls_w),
            ("lighting_and_safety", aux.lighting_and_safety_w),
        ],
    );
    labeled_metric(
        out,
        "bess_loss_watthours_total",
        "counter",
        "Energy that did not reach the POI, by category: the loss waterfall.",
        "category",
        &[
            ("battery", s.energy.battery_loss_wh),
            ("pcs", s.energy.pcs_loss_wh),
            ("transformer", s.energy.transformer_loss_wh),
            ("aux_hvac", items.hvac_wh),
            ("aux_bms", items.bms_wh),
            ("aux_pcs_standby", items.pcs_standby_wh),
            ("aux_controls", items.controls_wh),
            ("aux_lighting_and_safety", items.lighting_and_safety_wh),
        ],
    );
}

/// What the BMS is doing about cell spread: the widest and narrowest
/// max-minus-min cell voltage on site, and how many racks are bleeding.
fn bms_metrics(out: &mut String, s: &SiteState) {
    let (dv_min_v, dv_max_v) = s
        .racks()
        .map(|r| r.cell_dv_v)
        .fold(None, |acc: Option<(f64, f64)>, v| {
            Some(acc.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))))
        })
        .unwrap_or((0.0, 0.0));
    labeled_metric(
        out,
        "bess_rack_cell_dv_volts",
        "gauge",
        "Highest minus lowest cell voltage within a rack, narrowest and widest rack on site.",
        "stat",
        &[("min", dv_min_v), ("max", dv_max_v)],
    );
    metric(
        out,
        "bess_racks_balancing",
        "gauge",
        "Racks whose passive balancing resistors are on.",
        s.racks().filter(|r| r.balancing_active).count() as f64,
    );
}

#[cfg(test)]
mod tests;
