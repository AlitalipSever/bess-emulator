//! The player in the task: a data event advances the cursor and touches
//! nothing, a physical one reaches the kernel, and the status says what
//! fired, in firing order, with the drawn value visible.

use std::sync::Arc;

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};
use bess_scenario::{load, RunParams};
use serde_json::json;

use super::Active;
use crate::map;

/// Three events, listed out of time order: a data fault at the start, a
/// failure at two seconds, a leak at one.
const STORY: &[u8] = br"
schema: 1
name: two-kinds
events:
  - at: PT0S
    surface: mqtt
    fault: dropout
    duration: PT1M
  - at: PT2S
    target: block[2].container[0].hvac
    fault: failure
  - at: PT1S
    target: block[3].container[1].rack[4]
    fault: self_discharge
    rate_pct_per_month: 18
";

fn active() -> Active {
    let cfg = PlantConfig::gw01();
    let scenario = load(STORY, &cfg).unwrap();
    let params = RunParams {
        seed: 7,
        start_unix_s: 0,
        speed: 60.0,
        age_years: 0,
    };
    let points = Arc::new(map::build_points(&cfg));
    Active::new(Arc::new(scenario.resolve(params)), points)
}

fn plant() -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 7, 0)
}

#[test]
fn a_data_event_advances_the_cursor_and_touches_nothing() {
    let mut active = active();
    let mut sim = plant();
    let before = sim.state().clone();
    assert!(active.fire_due(&mut sim));
    assert_eq!(sim.state(), &before);

    let status = active.status();
    assert_eq!(status["fired"], 1);
    assert_eq!(status["pending"], 2);
    // The leak at one second comes next, though the file lists it last.
    assert_eq!(status["next"]["index"], 2);
    assert_eq!(status["next"]["fired"], false);
    assert_eq!(
        status["events"][0],
        json!({
            "index": 0, "at_s": 0, "tick": 0, "kind": "data", "fired": true,
            "surface": "mqtt", "fault": "dropout", "duration_s": 60,
        })
    );
    // Nothing is due twice.
    assert!(!active.fire_due(&mut sim));
}

#[test]
fn a_physical_event_reaches_the_kernel_and_the_status_says_so() {
    let mut active = active();
    let mut sim = plant();
    let weather = gw01_weather();
    for _ in 0..3 {
        active.fire_due(&mut sim);
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
    assert!(sim.state().blocks[2].containers[0].hvac.failed);
    let rack = &sim.state().blocks[3].containers[1].racks[4];
    assert!(rack.self_discharge_excess_per_s > 0.0);

    let status = active.status();
    assert_eq!(status["loaded"], true);
    assert_eq!(status["name"], "two-kinds");
    assert_eq!(status["seed"], 7);
    assert_eq!(status["speed"], 60.0);
    let hash = status["content_hash"].as_str().unwrap();
    assert!(hash.starts_with("0x") && hash.len() == 18, "{hash}");
    assert_eq!(
        (&status["fired"], &status["pending"]),
        (&json!(3), &json!(0))
    );
    assert!(status["next"].is_null());
    assert_eq!(
        status["events"][1],
        json!({
            "index": 2, "at_s": 1, "tick": 1, "kind": "physical", "fired": true,
            "target": "block[3].container[1].rack[4]", "fault": "self_discharge",
            "rate_ppm_per_day": 6000,
        })
    );
    assert_eq!(status["events"][2]["target"], "block[2].container[0].hvac");
    assert_eq!(status["events"][2]["fault"], "failure");
    assert!(status["events"][2].get("rate_ppm_per_day").is_none());
}

/// The architecture's core claim: data faults corrupt what the surfaces
/// say, never what the kernel does. The physical story with every data
/// fault laid over it produces the tree and the log the physical story
/// alone produces, while the tap shows its windows opened.
#[test]
fn the_kernel_cannot_tell_a_data_fault_happened() {
    const PHYSICAL: &str = "
schema: 1
name: physical
events:
  - at: PT0S
    target: block[2].container[0].hvac
    fault: failure
  - at: PT10S
    target: block[3].container[1].rack[4]
    fault: self_discharge
    rate_pct_per_month: 18
";
    const DATA: &str = "
  - at: PT0S
    surface: mqtt
    fault: dropout
    duration: PT20S
  - at: PT20S
    surface: mqtt
    fault: backfill_burst
  - at: PT5S
    surface: mqtt
    fault: freeze
    duration: PT1M
  - at: PT0S
    surface: modbus
    fault: unit_error
    points: block[2].pcs
  - at: PT30S
    surface: mqtt
    fault: nan_burst
    duration: PT10S
  - at: PT0S
    surface: modbus
    fault: chatter
    duration: PT1M
    rate_per_min: 30
  - at: PT0S
    surface: mqtt
    fault: timestamp_drift
    duration: PT1H
    drift_s_per_h: 60
";
    let cfg = PlantConfig::gw01();
    let points = Arc::new(map::build_points(&cfg));
    let params = RunParams {
        seed: 7,
        start_unix_s: 0,
        speed: 60.0,
        age_years: 0,
    };
    let weather = gw01_weather();
    let run = |text: String| {
        let resolved = load(text.as_bytes(), &cfg).unwrap().resolve(params);
        let mut active = Active::new(Arc::new(resolved), Arc::clone(&points));
        let mut sim = plant();
        for _ in 0..90 {
            active.fire_due(&mut sim);
            sim.step(&weather.inputs_at(sim.unix_time_s()));
        }
        (sim, active)
    };
    let (plain, _) = run(PHYSICAL.to_owned());
    let (faulted, active) = run(format!("{PHYSICAL}{DATA}"));

    assert_eq!(plain.state().event_log, faulted.state().event_log);
    assert_eq!(plain.state(), faulted.state());
    let taps = active.taps();
    assert_eq!(taps.windows().len(), 7);
    let freeze = taps.windows().iter().find(|w| w.index == 4).unwrap();
    assert!(
        freeze.frozen.is_some(),
        "the freeze captured when it opened"
    );
    assert_eq!(active.status()["fired"], 9);
}
