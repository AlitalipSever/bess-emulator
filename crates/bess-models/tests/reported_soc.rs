//! M2.5 phase 2: the reported SoC drifts, then snaps, and the truth never
//! notices.
//!
//! Three claims. The estimator does not touch the physics: the truth
//! trajectory with the imperfect estimator equals the one with the perfect
//! estimator, byte for byte, once the estimator's own fields are cut out.
//! On the plateau the estimate walks away from the truth at the rate the
//! sensor's offset explains, and no correction happens there. Over a
//! replayed week the reported SoC shows the sawtooth, drift and knee
//! corrections, while the truth stays smooth.

use bess_core::config::PlantConfig;
use bess_core::digest::fnv1a_64;
use bess_core::kernel::Event;
use bess_core::Simulation;
use bess_models::{gw01_models, gw01_weather, BasicBms};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;
/// 2026-04-11, the spring start the balancing measurements use.
const SPRING_S: i64 = START_UNIX_S + 100 * 86_400;

fn run_with(bms: BasicBms, start: i64, ticks: u64) -> (Simulation, Vec<Event>) {
    let cfg = PlantConfig::gw01();
    let mut models = gw01_models(&cfg);
    models.bms = Box::new(bms);
    let mut sim = Simulation::new(cfg, models, 7, start);
    let weather = gw01_weather();
    let mut events = Vec::new();
    for _ in 0..ticks {
        let inputs = weather.inputs_at(sim.unix_time_s());
        events.extend_from_slice(sim.step(&inputs));
    }
    (sim, events)
}

/// Cut the estimator's three rack fields and the event log out of a
/// serialized tree, leaving the physics.
fn physics_only(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut rest = json;
    while let Some(start) = rest.find("\"soc_reported\":") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let gain = after.find("\"sensor_gain\":").expect("fields sit together");
        let end = gain + after[gain..].find(',').expect("a field follows") + 1;
        rest = &after[end..];
    }
    out.push_str(rest);
    let start = out.find("\"event_log\":").expect("the log is in the tree");
    let end = start + out[start..].find('}').expect("the log closes") + 2;
    out.replace_range(start..end, "");
    out
}

#[test]
fn the_estimator_never_touches_the_truth() {
    let (real, _) = run_with(BasicBms::default(), START_UNIX_S, 4 * 3600);
    let (oracle, _) = run_with(BasicBms::with_perfect_estimator(), START_UNIX_S, 4 * 3600);
    let a = physics_only(&serde_json::to_string(real.state()).unwrap());
    let b = physics_only(&serde_json::to_string(oracle.state()).unwrap());
    assert_eq!(fnv1a_64(a.as_bytes()), fnv1a_64(b.as_bytes()));
    // The perfect estimator reports the truth as the BMS step saw it, one
    // cell-model step ago: at most a 1C tick of SoC behind.
    let tick_soc = 1.0 / 3600.0;
    assert!(oracle
        .state()
        .racks()
        .all(|r| (r.soc_reported - r.soc).abs() <= tick_soc));
    assert!(real
        .state()
        .racks()
        .any(|r| (r.soc_reported - r.soc).abs() > 1.0e-5));
}

#[test]
fn on_the_plateau_the_estimate_drifts_and_nothing_corrects_it() {
    // Four idle hours from the start: the plant sits at 0.5 until the
    // night charge, so every rack is on the plateau the whole time.
    let (sim, events) = run_with(BasicBms::default(), START_UNIX_S, 3 * 3600);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::SocResnapped { .. })),
        "a correction on the plateau"
    );
    let cfg = PlantConfig::gw01().rack;
    for rack in sim.state().racks() {
        // Idle: the offset alone moves the count, 3 h worth.
        let expected = -rack.sensor_offset_a * 3.0 / cfg.cell_capacity_ah;
        let drift = rack.soc_reported - rack.soc;
        assert!(
            (drift - expected).abs() < 0.05 * expected.abs() + 1.0e-6,
            "drift {drift} against {expected} from offset {} A",
            rack.sensor_offset_a
        );
    }
}

#[test]
fn a_replayed_week_shows_the_sawtooth_over_a_smooth_truth() {
    let cfg = PlantConfig::gw01();
    let mut models = gw01_models(&cfg);
    models.bms = Box::new(BasicBms::default());
    let mut sim = Simulation::new(cfg, models, 7, SPRING_S);
    let weather = gw01_weather();
    let mut resnaps = 0u64;
    let mut worst_error = 0.0f64;
    let mut worst_truth_step = 0.0f64;
    let mut last_truth = sim.state().blocks[0].containers[0].racks[0].soc;
    for _ in 0..7 * 86_400 {
        let inputs = weather.inputs_at(sim.unix_time_s());
        resnaps += sim
            .step(&inputs)
            .iter()
            .filter(|e| matches!(e, Event::SocResnapped { .. }))
            .count() as u64;
        let rack = &sim.state().blocks[0].containers[0].racks[0];
        worst_error = worst_error.max((rack.soc_reported - rack.soc).abs());
        worst_truth_step = worst_truth_step.max((rack.soc - last_truth).abs());
        last_truth = rack.soc;
    }
    println!("week: {resnaps} corrections on site, worst error {worst_error:.4}, worst truth step {worst_truth_step:.2e}");
    assert!(resnaps > 0, "no correction in a cycling week");
    assert!(
        worst_error > 0.003,
        "the estimate never drifted: {worst_error}"
    );
    assert!(
        worst_error < 0.03,
        "the estimate drifted past the field range: {worst_error}"
    );
    // A 1C rack moves 1/3600 of SoC per tick at most.
    assert!(worst_truth_step <= 1.0 / 3600.0 + 1.0e-9);
}
