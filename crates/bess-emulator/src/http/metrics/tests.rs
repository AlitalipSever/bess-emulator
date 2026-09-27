//! The exposition held to the state it reads and to the dashboard that reads it.

use bess_core::state::AuxPower;
use bess_core::{PlantConfig, SiteState};

use super::*;

/// A snapshot with a plausible house load and one container in each
/// cooling stage, so the exposition has something to say.
fn snapshot() -> Snapshot {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 1, 0);
    state.aux = AuxPower {
        hvac_w: 41_234.6,
        bms_w: 17_232.4,
        pcs_standby_w: 6_795.5,
        controls_w: 15_000.3,
        lighting_and_safety_w: 9_999.7,
    };
    state.substation.aux_power_w = state.aux.total_w();
    state.blocks[0].containers[0].hvac.mode = HvacMode::Cool1;
    state.blocks[0].containers[1].hvac.mode = HvacMode::Cool2;
    state.blocks[1].containers[0].hvac.mode = HvacMode::Heat;
    for (i, rack) in state.blocks[2].containers[0].racks.iter_mut().enumerate() {
        rack.cell_dv_v = 0.004 + 0.001 * i as f64;
        rack.balancing_active = i < 3;
    }
    Snapshot {
        state,
        input_regs: Vec::new(),
        holding_regs: Vec::new(),
        speed: 60.0,
    }
}

/// Value of one sample line, by its full name and labels.
fn value_of(body: &str, sample: &str) -> f64 {
    let line = body
        .lines()
        .find(|l| l.starts_with(sample) && l[sample.len()..].starts_with(' '))
        .unwrap_or_else(|| panic!("no sample {sample} in the exposition"));
    line.rsplit(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("unparsable sample line: {line}"))
}

/// Each gauge carries its own item. A sum cannot see a swap, and a swap
/// would publish wrong values under right names.
#[test]
fn every_house_load_gauge_carries_its_own_item() {
    let snap = snapshot();
    let body = metrics_body(&snap);
    let aux = &snap.state.aux;
    for (item, watts) in [
        ("hvac", aux.hvac_w),
        ("bms", aux.bms_w),
        ("pcs_standby", aux.pcs_standby_w),
        ("controls", aux.controls_w),
        ("lighting_and_safety", aux.lighting_and_safety_w),
    ] {
        let read = value_of(&body, &format!("bess_aux_power_watts{{item=\"{item}\"}}"));
        assert!(
            (read - watts).abs() < 1.0e-9,
            "item {item} reads {read} W, drew {watts} W"
        );
    }
}

/// The auxiliary identity, on the scraping surface this time: the five
/// itemized gauges are accumulated per consumer, the metered gauge comes
/// off the substation, and they have to agree.
#[test]
fn the_house_load_gauges_add_up_to_the_metered_total() {
    let body = metrics_body(&snapshot());
    let items: f64 = [
        "hvac",
        "bms",
        "pcs_standby",
        "controls",
        "lighting_and_safety",
    ]
    .iter()
    .map(|item| value_of(&body, &format!("bess_aux_power_watts{{item=\"{item}\"}}")))
    .sum();
    let metered = value_of(&body, "bess_aux_metered_power_watts");
    assert!(
        (items - metered).abs() < 1.0e-6,
        "items sum to {items} W, meter reads {metered} W"
    );
}

/// Every container is in exactly one mode, so the mode counts have to
/// cover the site. A missing arm would silently drop containers out of
/// the staging panel instead of failing.
#[test]
fn the_hvac_mode_counts_cover_every_container() {
    let cfg = PlantConfig::gw01();
    let body = metrics_body(&snapshot());
    let counted: f64 = ["off", "cool1", "cool2", "heat"]
        .iter()
        .map(|mode| value_of(&body, &format!("bess_hvac_containers{{mode=\"{mode}\"}}")))
        .sum();
    let containers = (cfg.blocks * cfg.containers_per_block) as f64;
    assert!(
        (counted - containers).abs() < f64::EPSILON,
        "{counted} containers counted, site has {containers}"
    );
}

/// Dashboards rot by outliving the metrics they query. Every `bess_*`
/// name in the shipped dashboard has to be a family this exposition
/// actually publishes.
#[test]
fn every_dashboard_query_names_a_metric_we_publish() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../deploy/grafana/dashboards/gw01.json");
    let text = std::fs::read_to_string(&path).expect("dashboard file");
    let dashboard: serde_json::Value = serde_json::from_str(&text).expect("dashboard json");
    let body = metrics_body(&snapshot());

    // Grafana rows carry their panels as a nested array, so a guard that
    // only walks the top level stops covering everything the day someone
    // groups the dashboard into rows.
    let mut queue: Vec<&serde_json::Value> = dashboard["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .collect();
    let mut checked = 0;
    while let Some(panel) = queue.pop() {
        queue.extend(panel["panels"].as_array().into_iter().flatten());
        for target in panel["targets"].as_array().into_iter().flatten() {
            let expr = target["expr"].as_str().unwrap_or_default();
            for name in expr
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|token| token.starts_with("bess_"))
            {
                assert!(
                    body.contains(&format!("# TYPE {name} ")),
                    "panel {} queries {name}, which /metrics does not publish",
                    panel["title"]
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 4,
        "the dashboard scan found only {checked} queries, so it is proving nothing"
    );
}

/// The spread gauges read the racks, not a stale copy: the widest rack
/// sets the max, a rack still at zero sets the min, and the balancing count
/// is the racks with bleeders on.
#[test]
fn the_bms_gauges_read_the_racks() {
    let snap = snapshot();
    let body = metrics_body(&snap);
    let widest = snap
        .state
        .racks()
        .map(|r| r.cell_dv_v)
        .fold(0.0_f64, f64::max);
    assert!((value_of(&body, "bess_rack_cell_dv_volts{stat=\"max\"}") - widest).abs() < 1.0e-12);
    assert!(value_of(&body, "bess_rack_cell_dv_volts{stat=\"min\"}").abs() < 1.0e-12);
    assert!((value_of(&body, "bess_racks_balancing") - 3.0).abs() < f64::EPSILON);
}
