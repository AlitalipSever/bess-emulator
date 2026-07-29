//! Physics invariants over a full simulated day. A violation here is a
//! kernel or model bug by definition, and fails the build.

use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, SyntheticWeather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

#[test]
fn one_simulated_day_conserves_energy_and_respects_bounds() {
    let cfg = PlantConfig::gw01();
    let soc_lo = cfg.rack.soc_min - 0.01;
    let soc_hi = cfg.rack.soc_max + 0.01;
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, START_UNIX_S);
    let weather = SyntheticWeather::default();

    let stored_start_wh = sim.stored_energy_wh();
    let mut last_import_wh = 0.0;
    let mut last_export_wh = 0.0;

    for tick in 0..86_400u64 {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
        let state = sim.state();

        // Meters are monotonic, every tick.
        assert!(
            state.substation.import_wh >= last_import_wh,
            "import meter went backwards"
        );
        assert!(
            state.substation.export_wh >= last_export_wh,
            "export meter went backwards"
        );
        last_import_wh = state.substation.import_wh;
        last_export_wh = state.substation.export_wh;

        // SoC stays inside the BMS window (plus taper tolerance).
        if tick % 300 == 0 {
            for rack in state.racks() {
                assert!(
                    (soc_lo..=soc_hi).contains(&rack.soc),
                    "rack SoC {} outside [{soc_lo}, {soc_hi}] at tick {tick}",
                    rack.soc
                );
            }
        }
    }

    // Energy balance: everything that crossed the POI is stored or lost.
    let state = sim.state();
    let delta_stored_wh = sim.stored_energy_wh() - stored_start_wh;
    let losses_wh = state.energy.battery_loss_wh
        + state.energy.pcs_loss_wh
        + state.energy.transformer_loss_wh
        + state.energy.aux_wh;
    let net_poi_wh = state.substation.import_wh - state.substation.export_wh;
    let residual_wh = net_poi_wh - delta_stored_wh - losses_wh;
    let throughput_wh = state.substation.import_wh + state.substation.export_wh;
    let relative = residual_wh.abs() / throughput_wh.max(1.0);
    assert!(
        relative < 2.0e-3,
        "energy balance residual {residual_wh:.1} Wh ({relative:.5} of throughput)"
    );

    // The day actually cycled the plant (guards against a silently idle run).
    assert!(
        state.substation.export_wh > 50.0e6,
        "plant never discharged"
    );
    assert!(state.substation.import_wh > 50.0e6, "plant never charged");
}
