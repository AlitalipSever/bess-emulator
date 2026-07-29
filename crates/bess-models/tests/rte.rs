//! M0 calibration gate: point-of-interconnection round-trip efficiency in
//! the 87-90% band on a full-depth cycle at 0.5C. Losses are present
//! (battery, conversion, transformer, house load) but thermal and auxiliary
//! behavior is not yet calibrated; the field band (80-85%) is the M1 gate.

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, SyntheticWeather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

#[test]
fn full_cycle_round_trip_efficiency_hits_the_m0_band() {
    let mut cfg = PlantConfig::gw01();
    cfg.initial_soc = 0.10;
    let power_w = 50.0e6; // 0.5C on the 100 MW / 200 MWh site
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 11, START_UNIX_S);
    let weather = SyntheticWeather::default();

    let step = |sim: &mut Simulation| {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
    };

    // Charge to 90%, rest, discharge back to the starting SoC.
    sim.set_external_setpoint_w(Some(-power_w));
    let mut guard = 0u32;
    while sim.state().average_soc() < 0.90 {
        step(&mut sim);
        guard += 1;
        assert!(guard < 8 * 3600, "charge phase never completed");
    }

    sim.set_external_setpoint_w(Some(0.0));
    for _ in 0..600 {
        step(&mut sim);
    }

    sim.set_external_setpoint_w(Some(power_w));
    guard = 0;
    while sim.state().average_soc() > 0.10 {
        step(&mut sim);
        guard += 1;
        assert!(guard < 8 * 3600, "discharge phase never completed");
    }

    let sub = &sim.state().substation;
    let rte = sub.export_wh / sub.import_wh;
    println!(
        "M0 round-trip efficiency: {rte:.4} (import {:.1} MWh, export {:.1} MWh)",
        sub.import_wh / 1.0e6,
        sub.export_wh / 1.0e6
    );
    assert!(
        (0.87..=0.90).contains(&rte),
        "round-trip efficiency {rte:.4} outside the M0 gate band [0.87, 0.90] \
         (import {:.1} MWh, export {:.1} MWh)",
        sub.import_wh / 1.0e6,
        sub.export_wh / 1.0e6
    );
}
