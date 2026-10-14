//! The day-ahead planner over a price year: one plan per local day, the
//! DST days planned over their own 23 and 25 hours, and the year wrapping
//! with the replay.

use bess_core::config::PlantConfig;
use bess_core::state::SiteState;
use bess_core::traits::EmsStrategy;
use bess_models::{gw01_day_ahead_price_eur_mwh, DayAheadEms, PriceYear};

/// 2024-01-01 00:00 UTC, the reference year's first UTC hour.
const REFERENCE_START_S: i64 = 1_704_067_200;
/// 2026-01-01 00:00 UTC.
const START_2026_S: i64 = 1_767_225_600;

fn state() -> SiteState {
    SiteState::new(&PlantConfig::gw01(), 1, 0)
}

/// The UTC instant (mid-hour) that reads series index `idx` of `year`.
fn instant_of(year: &PriceYear, idx: usize) -> i64 {
    let offset = i64::from(year.start_offset_h());
    let utc_hour = (idx as i64 - offset).rem_euclid(year.len() as i64);
    REFERENCE_START_S + utc_hour * 3600 + 1800
}

/// Three local days of 24, 23 and 25 hours, offset like the real year.
/// Every price is distinct, so the expected picks are unambiguous.
fn three_days() -> &'static PriceYear {
    let prices = (0..72).map(|i| f64::from((i * 37) % 72) - 20.0).collect();
    let year = PriceYear::from_parts(2024, 1, prices, vec![0, 24, 47, 72]).expect("valid");
    Box::leak(Box::new(year))
}

#[test]
fn each_local_day_charges_its_three_cheapest_and_discharges_its_two_dearest() {
    let year = three_days();
    let ems = DayAheadEms::from_price_year(year, 1.0e6, 3, 2);
    let state = state();
    for d in 0..year.days() {
        let hours = year.day_hours(d);
        let mut by_price: Vec<usize> = hours.clone().collect();
        by_price.sort_by(|a, b| year.price_eur_mwh(*a).total_cmp(&year.price_eur_mwh(*b)));
        for idx in hours {
            let target = ems.site_target_w(instant_of(year, idx), &state);
            let rank = by_price.iter().position(|&i| i == idx).expect("in day");
            let expected = if rank < 3 {
                -1.0e6
            } else if rank >= by_price.len() - 2 {
                1.0e6
            } else {
                0.0
            };
            assert!((target - expected).abs() < 1e-9, "day {d}, hour {idx}");
        }
    }
}

/// Over the bundled year, every local day gets exactly three charge and two
/// discharge hours, the 23-hour and 25-hour days included, and no charge
/// hour is dearer than a hour the plan left out.
#[test]
fn the_bundled_year_plans_every_local_day_including_the_dst_days() {
    let cfg = PlantConfig::gw01();
    let ems = bess_models::gw01_models(&cfg).ems;
    let year = bess_data::dayahead_de_lu_2024();
    let state = state();
    for d in 0..year.days() {
        let (mut charge, mut discharge) = (Vec::new(), Vec::new());
        for idx in year.day_hours(d) {
            let target = ems.site_target_w(instant_of(year, idx), &state);
            if target < 0.0 {
                charge.push(year.price_eur_mwh(idx));
            } else if target > 0.0 {
                discharge.push(year.price_eur_mwh(idx));
            }
        }
        assert_eq!((charge.len(), discharge.len()), (3, 2), "local day {d}");
        let dearest_charge = charge.iter().copied().fold(f64::MIN, f64::max);
        let cheapest_discharge = discharge.iter().copied().fold(f64::MAX, f64::min);
        for idx in year.day_hours(d) {
            let p = year.price_eur_mwh(idx);
            if !charge.contains(&p) && !discharge.contains(&p) {
                assert!(p >= dearest_charge && p <= cheapest_discharge, "day {d}");
            }
        }
    }
    assert_eq!(year.day_hours(90).len(), 23, "2024-03-31");
    assert_eq!(year.day_hours(300).len(), 25, "2024-10-27");
}

#[test]
fn the_year_wraps_with_the_replay() {
    let year = bess_data::dayahead_de_lu_2024();
    // 2026-12-31 23:00 UTC is 2027-01-01 00:00 in Berlin: the first local hour.
    let last_utc_hour = START_2026_S + 364 * 86_400 + 23 * 3600;
    assert_eq!(
        gw01_day_ahead_price_eur_mwh(last_utc_hour),
        year.price_eur_mwh(0)
    );
    // And the hour before it is the local year's last.
    let before = last_utc_hour - 3600;
    let last = year.len() - 1;
    assert_eq!(
        gw01_day_ahead_price_eur_mwh(before),
        year.price_eur_mwh(last)
    );
}

#[test]
fn any_simulated_year_reads_the_same_local_hour() {
    // 2026-07-14 18:00 UTC against 2024-07-14 18:00 UTC.
    let sim = START_2026_S + 194 * 86_400 + 18 * 3600;
    let reference = REFERENCE_START_S + 195 * 86_400 + 18 * 3600;
    assert_eq!(
        gw01_day_ahead_price_eur_mwh(sim).to_bits(),
        gw01_day_ahead_price_eur_mwh(reference).to_bits()
    );
}
