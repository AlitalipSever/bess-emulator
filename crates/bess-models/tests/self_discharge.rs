//! A leaking rack: the self-discharge fault through the spread law.
//!
//! The arithmetic the cases rest on. The healthy string drifts 1.8 % of
//! SoC per month (CALIBRATION.md, cell spread), 0.06 % a day. The bleed
//! narrows at most 0.24 A over 314 Ah, 0.076 % an hour, and only while the
//! rack sits at the top of its window: about 7.5 hours a day in the
//! replayed spring, so about 0.57 % a day, or 17 %/month. A leak well
//! under that is absorbed and shows only as longer balancing; one above it
//! outruns the bleed and the spread climbs to the imbalance warning.
//! Ten times the healthy drift (18 %/month, 6,000 ppm a day) is the case
//! held here; the measured day is in CALIBRATION.md.

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::rack as bit;
use bess_core::config::PlantConfig;
use bess_core::kernel::FaultAction;
use bess_core::state::RackState;
use bess_core::{Simulation, SiteState};
use bess_models::bms::Balancing;
use bess_models::{gw01_models, gw01_weather};

/// 2026-04-11, the spring start the balancing measurements use.
const SPRING_S: i64 = 1_767_225_600 + 100 * 86_400;
const DAY_S: f64 = 86_400.0;
/// 18 %/month as the fault carries it: 0.18 / 30 days, in ppm.
const TEN_TIMES_HEALTHY_PPM: u32 = 6_000;
const LEAKING_RACK: usize = 5;

fn idle_rack(dsoc: f64, excess_per_s: f64) -> RackState {
    let cfg = PlantConfig::gw01();
    let mut r = SiteState::new(&cfg, 7, SPRING_S).blocks[0].containers[0].racks[0].clone();
    r.soc = 0.5;
    r.current_a = 0.0;
    r.cell_dsoc = dsoc;
    r.self_discharge_excess_per_s = excess_per_s;
    r
}

/// Below the balancing window nothing bleeds, so a day of idle widens the
/// leaking rack's spread by exactly its excess rate over the healthy one.
#[test]
fn the_excess_widens_an_idle_spread_at_its_rate() {
    let cfg = PlantConfig::gw01();
    let bal = Balancing::default();
    let excess = f64::from(TEN_TIMES_HEALTHY_PPM) * 1.0e-6 / DAY_S;
    let mut healthy = idle_rack(0.004, 0.0);
    let mut leaking = idle_rack(0.004, excess);
    for _ in 0..86_400 {
        bal.step(&mut healthy, &cfg.rack, 1.0);
        bal.step(&mut leaking, &cfg.rack, 1.0);
    }
    let gap = leaking.cell_dsoc - healthy.cell_dsoc;
    assert!(
        (gap - 0.006).abs() < 1.0e-12,
        "a day added {gap}, not 0.6 %"
    );
    assert!(!leaking.balancing_active);
}

/// At zero the field is invisible: the healthy law, step for step.
#[test]
fn a_healthy_rack_drifts_as_before() {
    let cfg = PlantConfig::gw01();
    let bal = Balancing::default();
    let mut r = idle_rack(0.004, 0.0);
    let mut expected = r.cell_dsoc;
    for _ in 0..3600 {
        bal.step(&mut r, &cfg.rack, 1.0);
        expected += bal.growth_per_s * 1.0;
    }
    assert_eq!(r.cell_dsoc, expected);
}

/// The fastest leak the schema allows, held for a quarter: the spread stops
/// at the whole window instead of claiming a cell below empty.
#[test]
fn the_spread_never_exceeds_the_window() {
    let cfg = PlantConfig::gw01();
    let bal = Balancing::default();
    let mut r = idle_rack(0.004, 33_333.0e-6 / DAY_S);
    for _ in 0..90 {
        bal.step(&mut r, &cfg.rack, DAY_S);
    }
    assert!(r.cell_dsoc <= 1.0, "spread {}", r.cell_dsoc);
}

/// Through the kernel, on one container of the spring plant: the rack
/// leaking at ten times the healthy drift raises its imbalance warning
/// within a month (measured: day 20), while its eleven healthy neighbours,
/// balancing on the same days, never do.
#[test]
fn a_leaking_rack_reaches_the_warning_its_neighbours_do_not() {
    let mut cfg = PlantConfig::gw01();
    cfg.blocks = 1;
    cfg.containers_per_block = 1;
    cfg.grid.site_rated_w = cfg.pcs_rated_w / 2.0;
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, SPRING_S);
    sim.apply(FaultAction::RackSelfDischarge {
        block: 0,
        container: 0,
        rack: LEAKING_RACK,
        excess_ppm_per_day: TEN_TIMES_HEALTHY_PPM,
    })
    .unwrap();
    let weather = gw01_weather();

    let mut warned_day = None;
    for day in 1..=30u32 {
        for _ in 0..86_400 {
            sim.step(&weather.inputs_at(sim.unix_time_s()));
            let racks = &sim.state().blocks[0].containers[0].racks;
            for (k, r) in racks.iter().enumerate() {
                let warns = has_bit(r.alarm_bits, bit::IMBALANCE_WARNING);
                assert!(
                    !warns || k == LEAKING_RACK,
                    "healthy rack {k} warned on day {day}"
                );
            }
            if warned_day.is_none()
                && has_bit(racks[LEAKING_RACK].alarm_bits, bit::IMBALANCE_WARNING)
            {
                warned_day = Some(day);
            }
        }
    }
    // Even with no bleed at all, 0.66 % a day needs most of a week to carry
    // the spread from its commissioning draw to the warning; a warning
    // sooner than that is a unit slip, not physics.
    let day = warned_day.expect("the leak never outran the bleed in 30 days");
    assert!(
        day > 5,
        "warned on day {day}: faster than the arithmetic allows"
    );
}
