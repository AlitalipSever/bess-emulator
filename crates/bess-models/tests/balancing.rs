//! The plant selling balancing products (M4 phase 3 PR1): the awards the
//! strategy holds and the headroom they take from the plan, the FCR
//! reservoir's working range and its charge management, the alert state
//! and its release, the transaction outside the range, and the whole of
//! it through a checkpoint.

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::site as s;
use bess_core::checkpoint;
use bess_core::kernel::{Awards, Inputs, Product};
use bess_core::{PlantConfig, Simulation};
use bess_models::market::prices;
use bess_models::{gw01_models, gw01_weather, AwardPlan, DayAheadEms, SyntheticFrequency};

/// 2026-07-14 00:00 UTC: the warmest day.
const JULY_MIDNIGHT_S: i64 = 1_767_225_600 + 194 * 86_400;
const RATED_W: f64 = 100.0e6;

fn plant_with(plan: AwardPlan, start_unix_s: i64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let mut models = gw01_models(&cfg);
    models.ems = Box::new(
        DayAheadEms::from_price_year(
            bess_data::dayahead_de_lu_2024(),
            cfg.grid.site_rated_w,
            3,
            2,
        )
        .with_awards(plan),
    );
    Simulation::new(cfg, models, 7, start_unix_s)
}

fn at(sim: &Simulation, frequency_hz: f64) -> Inputs {
    Inputs {
        grid_frequency_hz: frequency_hz,
        ..gw01_weather().inputs_at(sim.unix_time_s())
    }
}

fn run_at(sim: &mut Simulation, frequency_hz: f64, ticks: u64) {
    for _ in 0..ticks {
        let inputs = at(sim, frequency_hz);
        sim.step(&inputs);
    }
}

/// The strategy's awards reach the controller with the block's annual
/// price, the droop is armed with the FCR award, and the plan's target
/// is kept inside the rating less the headroom.
#[test]
fn awards_arm_the_droop_and_take_headroom_from_the_plan() {
    let plan = AwardPlan {
        fcr_w: 20.0e6,
        afrr_pos_w: 10.0e6,
        afrr_neg_w: 0.0,
    };
    // 18:00 on the warmest day: the plan discharges at the rating.
    let mut sim = plant_with(plan, JULY_MIDNIGHT_S + 18 * 3600);
    run_at(&mut sim, 50.0, 2);
    let ems = &sim.state().ems;
    assert_eq!(ems.awards.fcr_w, 20.0e6);
    assert_eq!(ems.fcr_prequalified_w, 20.0e6);
    // 18:00 UTC is 20:00 local: block 20-24, the sixth.
    assert_eq!(
        ems.awards.fcr_price_eur_mw,
        prices::FCR_EUR_MW_BLOCK_2024[5]
    );
    assert!(
        (ems.awards.afrr_pos_price_eur_mw - prices::AFRR_POS_EUR_MW_H_2024[5] * 4.0).abs() < 1e-9
    );
    assert!(
        (ems.site_setpoint_w - 70.0e6).abs() < 1.0,
        "the plan's 100 MW held to 70 by 30 MW of headroom: {}",
        ems.site_setpoint_w
    );
    let (lo, hi) = (ems.reservoir.range_min, ems.reservoir.range_max);
    // 20 MW on 180 MWh usable: 5 MWh each way, 2.5 % of the 200 MWh.
    assert!(
        (lo - 0.075).abs() < 1e-3 && (hi - 0.925).abs() < 1e-3,
        "{lo} {hi}"
    );
    // A plant that sells nothing holds nothing and keeps its rating.
    let mut idle = plant_with(AwardPlan::NONE, JULY_MIDNIGHT_S + 18 * 3600);
    run_at(&mut idle, 50.0, 2);
    assert_eq!(idle.state().ems.awards, Awards::NONE);
    assert!((idle.state().ems.site_setpoint_w - RATED_W).abs() < 1.0);
}

/// A day of FCR on the synthetic frequency with the plan idle: the droop
/// delivers, the reservoir stays inside its working range, the dead-band
/// recentering is visible, and nothing alerts.
#[test]
fn a_day_of_fcr_keeps_the_reservoir_in_its_range() {
    let mut sim = plant_with(AwardPlan::NONE, JULY_MIDNIGHT_S);
    sim.set_external_setpoint_w(Some(0.0));
    sim.set_fcr_prequalified_w(20.0e6);
    let weather = gw01_weather();
    let f = SyntheticFrequency::ce_estimate();
    let (mut delivering, mut recentering) = (0u64, 0u64);
    for _ in 0..86_400 {
        let inputs = Inputs {
            grid_frequency_hz: f.frequency_hz(sim.unix_time_s()),
            ..weather.inputs_at(sim.unix_time_s())
        };
        sim.step(&inputs);
        let ems = &sim.state().ems;
        delivering += u64::from(ems.fcr_output_w.abs() > 1.0e6);
        recentering += u64::from(ems.reservoir.recenter_w.abs() > 1.0e5);
        let soc = sim.state().average_soc();
        assert!(
            soc >= ems.reservoir.range_min - 0.01 && soc <= ems.reservoir.range_max + 0.01,
            "left the range: {soc}"
        );
        assert!(!ems.reservoir.alert);
    }
    assert!(delivering > 20_000, "delivered {delivering} s");
    assert!(recentering > 1_000, "recentered {recentering} s");
    assert!(!has_bit(sim.state().alarm_bits, s::FCR_ALERT_STATE));
}

/// A −300 mHz hold enters the alert state at once (≥ 200 mHz), the bit
/// raises, the droop holds full power, and the release waits for the
/// minimum activation and a calm minute.
#[test]
fn an_excursion_enters_the_alert_state_and_the_calm_releases_it() {
    let mut sim = plant_with(AwardPlan::NONE, JULY_MIDNIGHT_S + 3 * 3600);
    sim.set_external_setpoint_w(Some(0.0));
    sim.set_fcr_prequalified_w(20.0e6);
    run_at(&mut sim, 50.0, 60);
    run_at(&mut sim, 49.7, 2);
    assert!(sim.state().ems.reservoir.alert);
    assert!(has_bit(sim.state().alarm_bits, s::FCR_ALERT_STATE));
    run_at(&mut sim, 49.7, 60);
    assert_eq!(sim.state().ems.fcr_output_w, 20.0e6);
    assert_eq!(
        sim.state().ems.reservoir.recenter_w,
        0.0,
        "no management in alert"
    );
    run_at(&mut sim, 50.0, 500);
    assert!(
        sim.state().ems.reservoir.alert,
        "the minimum activation first"
    );
    run_at(&mut sim, 50.0, 500);
    assert!(!sim.state().ems.reservoir.alert);
    assert!(!has_bit(sim.state().alarm_bits, s::FCR_ALERT_STATE));
    let bytes = checkpoint::save(sim.state()).expect("save");
    assert_eq!(checkpoint::load(&bytes).expect("load"), *sim.state());
}

/// A reservoir outside its working range in the normal state books a
/// transaction at the prequalified power toward the range, on the plan;
/// pinned awards by product and the clear.
#[test]
fn a_reservoir_outside_the_range_books_a_transaction() {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut state = bess_core::SiteState::new(&cfg, 7, JULY_MIDNIGHT_S + 3 * 3600);
    for rack in state
        .blocks
        .iter_mut()
        .flat_map(|b| b.containers.iter_mut())
        .flat_map(|c| c.racks.iter_mut())
    {
        rack.soc = 0.06;
        rack.soc_reported = 0.06;
    }
    let mut sim = Simulation::from_state(cfg, models, state);
    sim.set_external_setpoint_w(Some(0.0));
    let mut awards = Awards::NONE;
    awards.set_power_w(Product::Fcr, 20.0e6);
    sim.pin_awards(awards);
    run_at(&mut sim, 50.0, 2);
    let ems = &sim.state().ems;
    assert!(ems.awards_pinned);
    assert!(
        (ems.reservoir.transaction_w + 20.0e6).abs() < 1.0,
        "{}",
        ems.reservoir.transaction_w
    );
    assert!(
        (ems.site_setpoint_w + 20.0e6).abs() < 1.0,
        "the plan charges: {}",
        ems.site_setpoint_w
    );
    sim.unpin_awards();
    run_at(&mut sim, 50.0, 2);
    assert_eq!(sim.state().ems.awards, Awards::NONE);
    assert_eq!(sim.state().ems.reservoir.transaction_w, 0.0);
}

/// The aFRR service: a step request is followed at the gradient (half
/// a percent of the award a second, full in 200 s), never beyond the
/// award, in the award's own direction only; the energy is metered each
/// way; a plant without an award serves nothing.
#[test]
fn the_afrr_request_is_followed_at_the_gradient_inside_the_award() {
    let mut sim = plant_with(AwardPlan::NONE, JULY_MIDNIGHT_S + 3 * 3600);
    sim.set_external_setpoint_w(Some(0.0));
    let mut awards = Awards::NONE;
    awards.set_power_w(Product::AfrrPos, 20.0e6);
    sim.pin_awards(awards);
    let weather = gw01_weather();
    let step = |sim: &mut Simulation, request: f64| {
        let inputs = Inputs {
            grid_frequency_hz: 50.0,
            afrr_request_pu: request,
            ..weather.inputs_at(sim.unix_time_s())
        };
        sim.step(&inputs);
    };
    for _ in 0..30 {
        step(&mut sim, 0.0);
    }
    let mut last = 0.0;
    for tick in 1..=200 {
        step(&mut sim, 1.0);
        let w = sim.state().ems.afrr_followed_w;
        assert!(w - last <= 0.005 * 20.0e6 + 1.0, "gradient at {tick}");
        last = w;
    }
    assert!((last - 20.0e6).abs() < 1.0, "full in 200 s: {last}");
    assert_eq!(sim.state().ems.afrr_request_pu, 1.0);
    let delivered = sim.state().ems.afrr_energy_pos_wh;
    assert!(delivered > 20.0e6 * 100.0 / 3600.0 * 0.9, "{delivered}");
    // Over-requested: clamped at the award.
    step(&mut sim, 3.0);
    assert!((sim.state().ems.afrr_followed_w - 20.0e6).abs() < 1.0);
    // A negative request with no negative award: nothing to absorb.
    for _ in 0..300 {
        step(&mut sim, -1.0);
    }
    assert_eq!(sim.state().ems.afrr_followed_w, 0.0);
    assert_eq!(sim.state().ems.afrr_energy_neg_wh, 0.0);
    let bytes = checkpoint::save(sim.state()).expect("save");
    assert_eq!(checkpoint::load(&bytes).expect("load"), *sim.state());
    sim.unpin_awards();
    for _ in 0..3 {
        step(&mut sim, 1.0);
    }
    assert_eq!(sim.state().ems.afrr_followed_w, 0.0);
}
