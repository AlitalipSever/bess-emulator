//! Where temperature derating sits against the replayed year.
//!
//! The unit tests in `bms/derate.rs` pin the datasheet curve. These pin what
//! CALIBRATION.md says about it: on the reference site, with the M1 HVAC
//! holding its containers, derating moves on a winter morning (cells dip
//! below the 15 C knee of the charging table) but never binds, because the
//! plant draws half of what a rack is rated for and the factor never falls
//! that far. It is capability waiting for a scenario (an HVAC failure, a cold
//! start) rather than a change to the annual record.

use bess_core::traits::BmsLogic;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather, BasicBms};

/// 2026-01-01 00:00:00 UTC.
const NEW_YEAR_S: i64 = 1_767_225_600;
/// 2026-07-14, the warmest stretch of the replayed year.
const JULY_S: i64 = NEW_YEAR_S + 194 * 86_400;

struct DayExtremes {
    min_cell_c: f64,
    max_cell_c: f64,
    /// Lowest temperature factor any rack saw in either direction.
    min_factor: f64,
}

fn run_day(start_unix_s: i64) -> DayExtremes {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, start_unix_s);
    let weather = gw01_weather();
    let bms = BasicBms::default();
    let mut out = DayExtremes {
        min_cell_c: f64::INFINITY,
        max_cell_c: f64::NEG_INFINITY,
        min_factor: 1.0,
    };
    for tick in 0..86_400u32 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        if tick % 60 != 0 {
            continue;
        }
        let racks = sim
            .state()
            .blocks
            .iter()
            .flat_map(|b| b.containers.iter())
            .flat_map(|c| c.racks.iter());
        for rack in racks {
            let t = rack.cell_temp_c;
            out.min_cell_c = out.min_cell_c.min(t);
            out.max_cell_c = out.max_cell_c.max(t);
            let f = bms.charge_temp.factor(t).min(bms.discharge_temp.factor(t));
            out.min_factor = out.min_factor.min(f);
        }
    }
    out
}

/// Share of rack rating the plant draws at full PCS power: below this, a
/// temperature factor would start taking power the dispatch asked for.
fn operating_fraction() -> f64 {
    let cfg = PlantConfig::gw01();
    let rack_w = cfg.rack.max_current_a * cfg.rack.nominal_v();
    cfg.pcs_rated_w / (cfg.racks_per_block() as f64 * rack_w)
}

#[test]
fn summer_leaves_derating_untouched() {
    let july = run_day(JULY_S);
    assert!(
        (july.min_factor - 1.0).abs() < f64::EPSILON,
        "14 July: a rack derated to {:.3} (cells up to {:.1} C)",
        july.min_factor,
        july.max_cell_c
    );
}

#[test]
fn winter_moves_derating_without_binding() {
    let january = run_day(NEW_YEAR_S);
    // CALIBRATION.md publishes cells down to 13.6 C and a lowest factor of
    // 0.89 for this day. The band keeps the claim, not the decimal.
    assert!(
        (0.8..1.0).contains(&january.min_factor),
        "1 January: lowest factor {:.3} (cells {:.1} to {:.1} C)",
        january.min_factor,
        january.min_cell_c,
        january.max_cell_c
    );
    assert!(
        january.min_factor > operating_fraction(),
        "1 January: factor {:.3} fell below the {:.3} the plant draws",
        january.min_factor,
        operating_fraction()
    );
}

/// The chain phase 1 exists for, with the HVAC taken out of it: a rack from
/// the July afternoon, pushed onto the datasheet's hot shoulder, loses power
/// in both directions, and past the absolute limit it loses all of it.
#[test]
fn a_rack_past_the_hot_shoulder_loses_power() {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg.clone(), models, 7, JULY_S + 15 * 3600);
    let weather = gw01_weather();
    sim.step(&weather.inputs_at(sim.unix_time_s()));
    let mut rack = sim.state().blocks[0].containers[0].racks[0].clone();
    let bms = BasicBms::default();
    let normal = bms.rack_limits(&rack, &cfg.rack);

    rack.cell_temp_c = 57.5;
    let hot = bms.rack_limits(&rack, &cfg.rack);
    assert!((hot.max_charge_w - normal.max_charge_w * 0.5).abs() < 1.0e-6);
    assert!((hot.max_discharge_w - normal.max_discharge_w * 0.5).abs() < 1.0e-6);

    rack.cell_temp_c = 60.5;
    let tripped = bms.rack_limits(&rack, &cfg.rack);
    assert!(tripped.max_charge_w.abs() < f64::EPSILON);
    assert!(tripped.max_discharge_w.abs() < f64::EPSILON);
}
