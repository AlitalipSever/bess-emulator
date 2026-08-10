//! Round-trip efficiency calibration gate on a full-depth cycle at 0.5C.
//!
//! History: the M0 gate was [0.87, 0.90] with the flat 97.5% PCS. M0.5
//! replaced it with the fitted partial-load curve, which is MORE efficient
//! than the flat placeholder at the 50% load this cycle runs at (~98.6%
//! one-way), so the measured value moved up to 0.917. The band was re-drawn
//! around that measurement per CALIBRATION.md; it still sits inside the
//! 88-94% nameplate band for modern LFP systems. The field band (80-85%)
//! arrives with thermal and auxiliary modeling in M1.

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, SyntheticWeather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

#[test]
fn full_cycle_round_trip_efficiency_hits_the_calibration_band() {
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
        "round-trip efficiency: {rte:.4} (import {:.1} MWh, export {:.1} MWh)",
        sub.import_wh / 1.0e6,
        sub.export_wh / 1.0e6
    );
    assert!(
        (0.90..=0.93).contains(&rte),
        "round-trip efficiency {rte:.4} outside the M0.5 gate band \
         [0.90, 0.93] (import {:.1} MWh, export {:.1} MWh); see CALIBRATION.md",
        sub.import_wh / 1.0e6,
        sub.export_wh / 1.0e6
    );
}
