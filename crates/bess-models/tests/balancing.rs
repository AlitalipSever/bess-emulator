//! Cell spread and balancing through the kernel, over a replayed day.
//!
//! A fresh plant needs about four weeks of cycling before its spread reaches
//! the 30 mV start threshold, which is too long a fixture for a debug test
//! run. So this day starts where that month would leave it: every rack's
//! spread just past the threshold. The unit tests in `bms/balance.rs` hold
//! one rack; these hold the chain around it: the kernel calls the step, the
//! bleed heat reaches the loss meter, and the energy balance still closes.

use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};

/// 2026-04-11, a spring day of the replayed year: the plant charges to the
/// top of its window and idles there before the evening discharge.
const START_UNIX_S: i64 = 1_767_225_600 + 100 * 86_400;

/// SoC spread a month of cycling leaves: at SoC 0.95 it reads about 33 mV.
const WIDE_DSOC: f64 = 0.03;

struct Day {
    residual_share: f64,
    balancing_rack_ticks: u64,
    narrowest_dsoc: f64,
    final_dsoc: f64,
}

fn run_wide_day() -> Day {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, START_UNIX_S);
    for block in &mut state.blocks {
        for container in &mut block.containers {
            for rack in &mut container.racks {
                rack.cell_dsoc = WIDE_DSOC;
            }
        }
    }
    let models = gw01_models(&cfg);
    let mut sim = Simulation::from_state(cfg, models, state);
    let weather = gw01_weather();
    let stored_start_wh = sim.stored_energy_wh();
    let mut day = Day {
        residual_share: 0.0,
        balancing_rack_ticks: 0,
        narrowest_dsoc: f64::INFINITY,
        final_dsoc: 0.0,
    };
    for _ in 0..86_400 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        let s = sim.state();
        day.balancing_rack_ticks += s.racks().filter(|r| r.balancing_active).count() as u64;
        day.narrowest_dsoc = day
            .narrowest_dsoc
            .min(s.blocks[0].containers[0].racks[0].cell_dsoc);
    }
    let s = sim.state();
    day.final_dsoc = s.blocks[0].containers[0].racks[0].cell_dsoc;
    let losses_wh = s.energy.battery_loss_wh
        + s.energy.pcs_loss_wh
        + s.energy.transformer_loss_wh
        + s.energy.aux_wh;
    let net_poi_wh = s.substation.import_wh - s.substation.export_wh;
    let residual_wh = net_poi_wh - (sim.stored_energy_wh() - stored_start_wh) - losses_wh;
    day.residual_share =
        residual_wh.abs() / (s.substation.import_wh + s.substation.export_wh).max(1.0);
    day
}

/// Balancing ran, the energy balance closed over it, and the spread traced
/// one tooth of the sawtooth: narrowed while the plant idled at the top,
/// widened again through the evening cycle.
#[test]
fn a_wide_day_balances_closes_and_saws() {
    let day = run_wide_day();
    assert!(
        day.balancing_rack_ticks > 3600,
        "only {} rack-seconds of balancing on a day that starts past the threshold",
        day.balancing_rack_ticks
    );
    assert!(
        day.residual_share < 2.0e-3,
        "energy balance residual {:.5} of throughput with balancing on",
        day.residual_share
    );
    assert!(
        day.narrowest_dsoc < WIDE_DSOC,
        "the top-of-window idle never narrowed the spread"
    );
    assert!(
        day.final_dsoc > day.narrowest_dsoc,
        "cycling after the idle never widened it again"
    );
}
