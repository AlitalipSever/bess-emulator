//! M5 phase 1 gate: the racks age, and the plant feels it.
//!
//! The model's own figures are held in its unit tests; this file holds the
//! kernel's use of it: a rack at rest ages by the calendar alone, a cycled
//! rack faster, an aged rack stores less at the same state of charge and
//! heats more at the same current, two racks with different rates
//! diverge, the daily pass books the written-off energy, a checkpoint
//! carries the age, and the switch-off proof: with `NoAging` no state of
//! health moves.

use bess_core::config::PlantConfig;
use bess_core::state::aging::{RackAge, StressDay, DAY_S};
use bess_core::state::RackState;
use bess_core::traits::{AgingModel, CellModel};
use bess_core::{checkpoint, Simulation};
use bess_models::cell::Ecm1Rc;
use bess_models::{gw01_models, gw01_weather, LfpAging, NoAging};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

fn fresh_rack() -> RackState {
    let cfg = PlantConfig::gw01();
    let mut r =
        bess_core::SiteState::new(&cfg, 7, START_UNIX_S).blocks[0].containers[0].racks[0].clone();
    r.resistance_scale = 1.0;
    r.age = RackAge::fresh(1.0);
    r.soc = 0.5;
    r
}

fn rest_day(temp_c: f64) -> StressDay {
    StressDay {
        days: 1.0,
        efc: 0.0,
        soc_mean: 0.5,
        temp_k_mean: temp_c + 273.15,
        dod: 0.0,
        c_rate: 0.0,
    }
}

fn cycle_day(temp_c: f64, efc: f64) -> StressDay {
    StressDay {
        efc,
        dod: 0.8,
        c_rate: 0.5,
        ..rest_day(temp_c)
    }
}

#[test]
fn rest_ages_by_the_calendar_alone_and_cycling_adds_to_it() {
    let model = LfpAging::gw01();
    let cfg = PlantConfig::gw01().rack;
    let mut rested = RackAge::fresh(1.0);
    let mut cycled = RackAge::fresh(1.0);
    for _ in 0..365 {
        model.age_rack(&mut rested, &cfg, &rest_day(25.0));
        model.age_rack(&mut cycled, &cfg, &cycle_day(25.0, 1.0));
    }
    assert!(rested.q_loss_cal > 0.0);
    assert_eq!(rested.q_loss_cyc, 0.0);
    assert_eq!(rested.r_gain_cyc, 0.0);
    assert!((rested.q_loss_cal - cycled.q_loss_cal).abs() < 1.0e-12);
    assert!(cycled.q_loss_cyc > 0.0);
    assert!(cycled.soh() < rested.soh());
}

#[test]
fn an_aged_rack_stores_less_and_heats_more() {
    let model = Ecm1Rc::lfp_314ah_rack();
    let cfg = PlantConfig::gw01().rack;
    let fresh = fresh_rack();
    let mut old = fresh.clone();
    old.age.q_loss_cal = 0.05;
    old.age.q_loss_cyc = 0.10;
    old.age.r_gain_cal = 0.10;
    old.age.r_gain_cyc = 0.05;
    old.soh = old.age.soh();
    assert!((old.soh - 0.85).abs() < 1.0e-12);

    let e_fresh = model.stored_energy_wh(&fresh, &cfg);
    let e_old = model.stored_energy_wh(&old, &cfg);
    assert!(
        (e_old / e_fresh - 0.85).abs() < 1.0e-9,
        "{e_old} vs {e_fresh}"
    );

    // The same discharge current through 15 % more resistance.
    let mut f = fresh.clone();
    let mut o = old.clone();
    let p_w = 150.0e3;
    let rf = model.step_rack(&mut f, &cfg, p_w, 1.0);
    let ro = model.step_rack(&mut o, &cfg, p_w, 1.0);
    assert!(ro.heat_w > rf.heat_w, "{} vs {}", ro.heat_w, rf.heat_w);
    // And a larger SoC move for the same charge.
    assert!(0.5 - o.soc > 0.5 - f.soc);
}

#[test]
fn racks_with_different_rates_diverge() {
    let model = LfpAging::gw01();
    let cfg = PlantConfig::gw01().rack;
    let mut slow = RackAge::fresh(0.9);
    let mut fast = RackAge::fresh(1.1);
    for _ in 0..100 {
        model.age_rack(&mut slow, &cfg, &cycle_day(25.0, 1.0));
        model.age_rack(&mut fast, &cfg, &cycle_day(25.0, 1.0));
    }
    assert!(fast.soh() < slow.soh());
    assert!(fast.resistance_growth() > slow.resistance_growth());
}

fn run_days(aging: Box<dyn AgingModel>, days: u64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let mut models = gw01_models(&cfg);
    models.aging = aging;
    let mut sim = Simulation::new(cfg, models, 7, START_UNIX_S);
    let weather = gw01_weather();
    for _ in 0..(days * DAY_S as u64) {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
    }
    sim
}

/// Two days of the plant: every rack closed two ledgers, its health fell,
/// the spread between racks exists, and the books carry what the fade
/// wrote off.
#[test]
fn the_daily_pass_ages_every_rack_and_books_the_write_off() {
    let sim = run_days(Box::new(LfpAging::gw01()), 2);
    let state = sim.state();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for rack in state.racks() {
        assert!((rack.age.days - 2.0).abs() < 1.0e-9, "{}", rack.age.days);
        assert!(rack.age.last_day.is_some());
        assert!(rack.age.efc > 0.0, "a dispatched rack cycled");
        assert!(rack.soh < 1.0);
        assert!((rack.soh - rack.age.soh()).abs() < 1.0e-15);
        assert!(rack.age.resistance_growth() > 0.0);
        assert!(rack.age.rate_scale > 0.89 && rack.age.rate_scale < 1.11);
        min = min.min(rack.soh);
        max = max.max(rack.soh);
    }
    assert!(max - min > 1.0e-6, "the racks spread: {min} to {max}");
    // Two days of calendar at the design stress is a few hundredths of a
    // percent: the write-off is of that order of the plant's energy.
    let written_off = state.energy.aging_wh;
    assert!(written_off > 0.0);
    assert!(written_off < 0.002 * sim.config().nominal_energy_wh());
}

#[test]
fn a_checkpoint_carries_the_age() {
    let sim = run_days(Box::new(LfpAging::gw01()), 1);
    let bytes = checkpoint::save(sim.state()).expect("a checkpoint");
    let back = checkpoint::load(&bytes).expect("the same tree");
    assert_eq!(&back, sim.state());
    let rack = &back.blocks[0].containers[0].racks[0];
    assert!(rack.age.q_loss_cal > 0.0);
    assert!(rack.age.last_day.is_some());
}

/// The switch-off proof: with `NoAging` no state of health moves, no
/// resistance grows, nothing is written off, and the ledger still counts.
#[test]
fn with_no_aging_the_plant_is_the_v090_plant() {
    let sim = run_days(Box::new(NoAging), 2);
    let state = sim.state();
    for rack in state.racks() {
        assert_eq!(rack.soh, 1.0);
        assert_eq!(rack.age.resistance_growth(), 0.0);
        assert!((rack.age.days - 2.0).abs() < 1.0e-9);
    }
    assert_eq!(state.energy.aging_wh, 0.0);
}
