//! The fiscal meter's EUR rows (M4 phase 3 PR3): the day-ahead value of
//! the energy exchanged at the hour's price, the capacity an award
//! accrues over its blocks, the aFRR energy at the merit order's floor,
//! the imbalance settlement of a period's deviation, the totals, and the
//! rows through a checkpoint.

use bess_core::checkpoint;
use bess_core::kernel::{Awards, Inputs, Product};
use bess_core::{PlantConfig, Simulation};
use bess_models::market::prices;
use bess_models::{gw01_day_ahead_price_eur_mwh, gw01_models, gw01_weather};

/// 2026-07-14 00:00 UTC: the warmest day.
const JULY_MIDNIGHT_S: i64 = 1_767_225_600 + 194 * 86_400;

fn plant(start_unix_s: i64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    Simulation::new(cfg, models, 7, start_unix_s)
}

/// A day on the plan: the day-ahead row equals the exchange at the POI
/// priced hour by hour, the capacity and aFRR rows stay zero without an
/// award, and the imbalance row is the sum of the periods' deviations at
/// the price; the totals are the periods' sums.
#[test]
fn a_day_on_the_plan_books_its_exchange_at_the_hours_prices() {
    let mut sim = plant(JULY_MIDNIGHT_S);
    let weather = gw01_weather();
    let mut expected_eur = 0.0;
    let mut periods_eur = 0.0;
    let mut last_closed = 0u32;
    for _ in 0..86_400 {
        let price = gw01_day_ahead_price_eur_mwh(sim.unix_time_s());
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        let sub = &sim.state().substation;
        expected_eur += sub.poi_active_power_w / 3600.0 / 1.0e6 * price;
        if sub.revenue.periods_closed != last_closed {
            last_closed = sub.revenue.periods_closed;
            periods_eur += sub.revenue.last_eur.total();
        }
    }
    let total = &sim.state().substation.revenue.total_eur;
    assert!(
        (total.day_ahead - expected_eur).abs() < 1.0,
        "day-ahead {} vs {expected_eur}",
        total.day_ahead
    );
    assert!(total.day_ahead.abs() > 1000.0, "a cycling day moves money");
    assert_eq!(total.capacity, 0.0);
    assert_eq!(total.afrr_energy, 0.0);
    assert!(
        total.imbalance != 0.0,
        "the ramps deviate from the schedule"
    );
    assert!((total.total() - periods_eur).abs() < 1.0e-6);
}

/// An FCR award over a block accrues its price per MW; an aFRR award
/// its hourly price over the block's four hours; aFRR energy delivered
/// is paid the merit order's floor.
#[test]
fn awards_accrue_their_capacity_prices_and_afrr_energy_its_floor() {
    // 20:00 UTC on 2026-07-14 is 22:00 local: inside block 20-24.
    let mut sim = plant(JULY_MIDNIGHT_S + 20 * 3600);
    sim.set_external_setpoint_w(Some(0.0));
    let mut awards = Awards::NONE;
    awards.set_power_w(Product::Fcr, 10.0e6);
    awards.set_power_w(Product::AfrrPos, 20.0e6);
    awards.fcr_price_eur_mw = prices::FCR_EUR_MW_BLOCK_2024[5];
    awards.afrr_pos_price_eur_mw = prices::AFRR_POS_EUR_MW_H_2024[5] * 4.0;
    sim.pin_awards(awards);
    let weather = gw01_weather();
    for _ in 0..3600 {
        let inputs = Inputs {
            grid_frequency_hz: 50.0,
            afrr_request_pu: 1.0,
            ..weather.inputs_at(sim.unix_time_s())
        };
        sim.step(&inputs);
    }
    let total = &sim.state().substation.revenue.total_eur;
    // One hour of a four-hour block: a quarter of the FCR block price per
    // MW, and one hour of the aFRR hourly price.
    let expected_capacity =
        10.0 * prices::FCR_EUR_MW_BLOCK_2024[5] / 4.0 + 20.0 * prices::AFRR_POS_EUR_MW_H_2024[5];
    assert!(
        (total.capacity - expected_capacity).abs() < 0.01,
        "capacity {} vs {expected_capacity}",
        total.capacity
    );
    // The request ramps to 20 MW in 200 s and holds: about 18.9 MWh.
    let delivered_mwh = sim.state().ems.afrr_energy_pos_wh / 1.0e6;
    assert!(
        delivered_mwh > 18.5 && delivered_mwh < 20.0,
        "{delivered_mwh}"
    );
    assert!(
        (total.afrr_energy - delivered_mwh * prices::AFRR_ENERGY_MIN_POS_EUR_MWH_2024).abs() < 0.5,
        "afrr energy {}",
        total.afrr_energy
    );
    let bytes = checkpoint::save(sim.state()).expect("save");
    assert_eq!(checkpoint::load(&bytes).expect("load"), *sim.state());
}
