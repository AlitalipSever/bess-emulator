//! Reactive power in the plant (M4 phase 1 PR2): every converter stays on
//! its circle in every mode, zero active power leaves the whole rating
//! for vars, the POI reads the converters' vars less the transformer's,
//! the default mode is the unity-factor plant, and vars carry no energy.

use bess_core::checkpoint;
use bess_core::state::ReactiveMode;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-07-14 00:00 UTC: the warmest day.
const JULY_MIDNIGHT_S: i64 = 1_767_225_600 + 194 * 86_400;
const PCS_VA: f64 = 5.0e6;

fn plant(start_unix_s: i64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 7, start_unix_s)
}

fn run(sim: &mut Simulation, ticks: u64) {
    let weather = gw01_weather();
    for _ in 0..ticks {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
}

fn circle_holds(sim: &Simulation) {
    for b in &sim.state().blocks {
        let s2 = b.pcs.p_ac_w * b.pcs.p_ac_w + b.pcs.q_var * b.pcs.q_var;
        assert!(
            s2 <= PCS_VA * PCS_VA * (1.0 + 1.0e-9),
            "off the circle: P {} Q {}",
            b.pcs.p_ac_w,
            b.pcs.q_var
        );
    }
}

/// A day on the plan in each mode: the circle holds at every tick, and
/// the POI reads what the mode asks within the transformer's share.
#[test]
fn every_mode_keeps_the_converters_on_their_circle() {
    let modes = [
        ReactiveMode::unity(),
        ReactiveMode::FixedCosPhi {
            cos_phi: 0.9,
            under_excited: true,
        },
        ReactiveMode::FixedQ { var: 30.0e6 },
        ReactiveMode::QofU,
    ];
    for mode in modes {
        let mut sim = plant(JULY_MIDNIGHT_S + 17 * 3600);
        sim.set_reactive_mode(mode);
        let mut q_seen = 0.0_f64;
        for _ in 0..3 * 3600 {
            run(&mut sim, 1);
            circle_holds(&sim);
            let converters: f64 = sim.state().blocks.iter().map(|b| b.pcs.q_var).sum();
            q_seen = q_seen.max(converters.abs());
        }
        let sub = &sim.state().substation;
        if mode == ReactiveMode::unity() {
            assert!(
                sub.poi_power_factor > 0.99,
                "unity mode reads {}",
                sub.poi_power_factor
            );
            continue;
        }
        match mode {
            ReactiveMode::FixedCosPhi { .. } => {
                assert!(q_seen > 20.0e6, "cos φ 0.9 never asked for vars");
                assert!(
                    sub.poi_reactive_power_var <= 0.0,
                    "under-excited reads positive"
                );
            }
            ReactiveMode::FixedQ { .. } => {
                assert!(
                    (sub.poi_reactive_power_var - 30.0e6).abs() < 5.0e6,
                    "fixed Q reads {}",
                    sub.poi_reactive_power_var
                );
            }
            ReactiveMode::QofU => {
                // The grid swings ±2 %, the dead band is ±2 %: the
                // characteristic sits at its edge and the vars stay small.
                assert!(q_seen < 10.0e6, "Q(U) delivered {q_seen} on a 2 % swing");
            }
        }
    }
}

/// At zero active power the converters hand the whole rating to vars:
/// a fixed 100 Mvar on an idle plant reaches the converters' total less
/// the transformer's consumption at that current.
#[test]
fn zero_active_power_leaves_the_rating_for_vars() {
    let mut sim = plant(JULY_MIDNIGHT_S + 3 * 3600);
    sim.set_external_setpoint_w(Some(0.0));
    sim.set_reactive_mode(ReactiveMode::FixedQ { var: 100.0e6 });
    run(&mut sim, 120);
    circle_holds(&sim);
    let total: f64 = sim.state().blocks.iter().map(|b| b.pcs.q_var).sum();
    assert!((total - 100.0e6).abs() < 1.0e6, "converters {total}");
    let sub = &sim.state().substation;
    // I²X at rated current on a 12 % reactance: 12 Mvar.
    assert!(
        (sub.poi_reactive_power_var - 88.0e6).abs() < 2.0e6,
        "POI {}",
        sub.poi_reactive_power_var
    );
    assert!(sub.poi_power_factor < 0.1);
    let bytes = checkpoint::save(sim.state()).expect("save");
    assert_eq!(checkpoint::load(&bytes).expect("load"), *sim.state());
}

/// Vars carry no energy: the same day in unity and in fixed-Q mode books
/// the same import, export and losses.
#[test]
fn reactive_power_moves_no_energy() {
    let mut unity = plant(JULY_MIDNIGHT_S);
    let mut vars = plant(JULY_MIDNIGHT_S);
    vars.set_reactive_mode(ReactiveMode::FixedQ { var: 20.0e6 });
    run(&mut unity, 6 * 3600);
    run(&mut vars, 6 * 3600);
    assert_eq!(unity.state().energy, vars.state().energy);
    assert_eq!(
        unity.state().substation.export_wh,
        vars.state().substation.export_wh
    );
    assert!(vars.state().substation.poi_reactive_power_var > 10.0e6);
    assert_eq!(unity.state().substation.poi_reactive_power_var, 0.0);
}
