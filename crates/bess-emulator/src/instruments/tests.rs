//! The instrument inventory holds its classes: every reading of every
//! instrumented point stays inside the class limit at every load, the
//! S-curve has the shape the standards give it, the same tuple yields the
//! same bytes, and with the inventory off the reading is the truth.

use bess_core::config::PlantConfig;

use super::inventory::{instrument_of, Instrument};
use super::model::class_limit;
use super::Instruments;
use crate::map::build_points;

fn instruments(on: bool) -> (Instruments, Vec<String>) {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let names = points.iter().map(|p| p.name.clone()).collect();
    (Instruments::new(&points, &cfg, 7, on), names)
}

#[test]
fn the_inventory_is_explicit_and_small() {
    let (inst, names) = instruments(true);
    let instrumented: Vec<&str> = names
        .iter()
        .enumerate()
        .filter(|(i, _)| inst.instrument(*i).is_some())
        .map(|(_, n)| n.as_str())
        .collect();
    // 15 site points plus 6 per block over 20 blocks.
    assert_eq!(instrumented.len(), 18 + 7 * 20, "{instrumented:?}");
    for exact in [
        "site.soc_pct",
        "site.alarm_bits",
        "site.event_counter",
        "site.poi.frequency_hz",
        "block00.pcs.state",
        "block00.soc_pct",
        "control.site_setpoint_w",
    ] {
        assert!(instrument_of(exact).is_none(), "{exact} should be exact");
    }
}

#[test]
fn the_s_curve_doubles_the_limit_at_one_percent_load() {
    let spec = Instrument::Revenue.class();
    assert!((class_limit(spec, 1.0) - 0.002).abs() < 1.0e-12);
    assert!((class_limit(spec, 0.05) - 0.002).abs() < 1.0e-12);
    assert!((class_limit(spec, 0.01) - 0.004).abs() < 1.0e-12);
    assert!((class_limit(spec, 0.001) - 0.004).abs() < 1.0e-12);
    let mid = class_limit(spec, 0.02);
    assert!(mid > 0.002 && mid < 0.004, "mid-curve {mid}");
    let ct = Instrument::Analog.class();
    assert!((class_limit(ct, 0.01) / class_limit(ct, 1.0) - 3.0).abs() < 1.0e-9);
}

/// Every instrumented point, at loads from zero to full scale and across
/// a year of sampled ticks, reads inside the class limit the standard
/// publishes for that load (plus three sigma of the zero noise).
#[test]
fn every_reading_stays_inside_its_class_at_every_load() {
    let (inst, names) = instruments(true);
    let cfg = PlantConfig::gw01();
    let loads = [0.0, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.3, 0.6, 1.0, 1.2];
    for (i, name) in names.iter().enumerate() {
        let Some(_) = inst.instrument(i) else {
            continue;
        };
        let fs = super::inventory::full_scale(name, &cfg);
        for tick in (0..366 * 86_400).step_by(3_600 * 7 + 13) {
            for &load in &loads {
                for sign in [1.0, -1.0] {
                    let truth = sign * load * fs;
                    let read = inst.measure(i, tick, truth);
                    let allowed = inst.allowed_error(i, truth).unwrap();
                    assert!(
                        (read - truth).abs() <= allowed,
                        "{name} at load {load} tick {tick}: read {read}, truth {truth}, allowed {allowed}"
                    );
                }
            }
        }
    }
}

#[test]
fn low_load_is_measured_worse_relatively_than_rated_load() {
    let (inst, names) = instruments(true);
    let cfg = PlantConfig::gw01();
    let i = names
        .iter()
        .position(|n| n == "site.poi.active_power_w")
        .unwrap();
    let fs = super::inventory::full_scale(&names[i], &cfg);
    let rel = |load: f64| {
        let mut worst: f64 = 0.0;
        for tick in (0..30 * 86_400).step_by(600) {
            let truth = load * fs;
            worst = worst.max(((inst.measure(i, tick, truth) - truth) / truth).abs());
        }
        worst
    };
    assert!(
        rel(0.01) > rel(1.0),
        "low load {} rated {}",
        rel(0.01),
        rel(1.0)
    );
}

#[test]
fn the_same_tuple_reads_the_same_bytes_and_off_reads_the_truth() {
    let (a, names) = instruments(true);
    let (b, _) = instruments(true);
    let (off, _) = instruments(false);
    for i in 0..names.len() {
        for tick in [0u64, 1, 59, 86_400, 9_999_999] {
            let truth = 12_345.678;
            assert_eq!(
                a.measure(i, tick, truth).to_bits(),
                b.measure(i, tick, truth).to_bits()
            );
            assert_eq!(off.measure(i, tick, truth).to_bits(), truth.to_bits());
        }
    }
}

#[test]
fn a_cumulative_register_keeps_a_constant_ratio() {
    let (inst, names) = instruments(true);
    let i = names
        .iter()
        .position(|n| n == "site.meter.export_kwh")
        .unwrap();
    let r0 = inst.measure(i, 0, 1.0e6) / 1.0e6;
    let r1 = inst.measure(i, 5_000_000, 3.0e6) / 3.0e6;
    assert!((r0 - r1).abs() < 1.0e-12);
    assert!((r0 - 1.0).abs() <= 0.002 && (r0 - 1.0).abs() > 1.0e-9);
}

/// Two meters on one quantity disagree by a bounded, wandering amount: the
/// POI analog's error over a simulated day stays inside its class, is not
/// constant, and is correlated from one minute to the next (a wander, not
/// white noise), while a second instrument on the same power, the sum of
/// the blocks' AC readings, wanders on its own.
#[test]
fn two_meters_on_one_quantity_disagree_by_a_bounded_wander() {
    let mut sim = crate::fixtures::hot_plant();
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let inst = Instruments::new(&points, &cfg, sim.state().meta.seed, true);
    let poi = points
        .iter()
        .position(|p| p.name == "site.poi.active_power_w")
        .unwrap();
    let blocks: Vec<usize> = points
        .iter()
        .enumerate()
        .filter(|(_, p)| p.name.ends_with(".pcs.p_ac_kw"))
        .map(|(i, _)| i)
        .collect();
    let weather = bess_models::gw01_weather();
    let mut poi_err = Vec::new();
    let mut gap = Vec::new();
    for tick in 0..6 * 3600u64 {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
        if tick % 60 != 0 {
            continue;
        }
        let state = sim.state();
        let t = state.tick;
        let truth = (points[poi].extract)(state);
        let read = inst.measure(poi, t, truth);
        assert!((read - truth).abs() <= inst.allowed_error(poi, truth).unwrap());
        poi_err.push(read - truth);
        let blocks_w: f64 = blocks
            .iter()
            .map(|&i| inst.measure(i, t, (points[i].extract)(state)) * 1000.0)
            .sum();
        gap.push(read - blocks_w);
    }
    let mean = poi_err.iter().sum::<f64>() / poi_err.len() as f64;
    let var = poi_err.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / poi_err.len() as f64;
    let lag1 = poi_err
        .windows(2)
        .map(|w| (w[0] - mean) * (w[1] - mean))
        .sum::<f64>()
        / (poi_err.len() as f64 * var);
    let worst = poi_err.iter().fold(0.0f64, |m, e| m.max(e.abs()));
    println!(
        "POI analog error over six hours: worst {worst:.0} W, sigma {:.0} W, lag-1 autocorrelation {lag1:.3}; POI minus block sum from {:.0} to {:.0} W",
        var.sqrt(),
        gap.iter().fold(f64::INFINITY, |m, g| m.min(*g)),
        gap.iter().fold(f64::NEG_INFINITY, |m, g| m.max(*g))
    );
    assert!(var > 0.0, "the error never moved");
    assert!(lag1 > 0.5, "the error is white, not a wander: lag-1 {lag1}");
    let gap_span = gap.iter().fold(f64::NEG_INFINITY, |m, g| m.max(*g))
        - gap.iter().fold(f64::INFINITY, |m, g| m.min(*g));
    assert!(gap_span > 0.0);
}
