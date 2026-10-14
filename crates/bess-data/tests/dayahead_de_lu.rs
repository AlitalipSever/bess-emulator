//! Integrity and shape gates for the bundled DE-LU 2024 day-ahead artifact.
//!
//! The loader asserts the pinned hash; these tests pin the local-year shape
//! the planner and the revenue meter rely on, so a regenerated artifact that
//! drifts fails CI even when hash and pin were moved together.

use bess_data::dayahead_de_lu_2024;

/// Local day index of 2024-03-31 (spring forward) and 2024-10-27 (fall back).
const SPRING_DAY: usize = 90;
const AUTUMN_DAY: usize = 300;

#[test]
fn loads_as_the_local_year() {
    let year = dayahead_de_lu_2024();
    assert_eq!(year.year(), 2024);
    assert_eq!(year.len(), 8784);
    assert_eq!(year.days(), 366);
    assert_eq!(
        year.start_offset_h(),
        1,
        "Europe/Berlin is UTC+1 on January 1st"
    );
}

#[test]
fn every_day_is_24_hours_except_the_two_dst_days() {
    let year = dayahead_de_lu_2024();
    for d in 0..year.days() {
        let expected = match d {
            SPRING_DAY => 23,
            AUTUMN_DAY => 25,
            _ => 24,
        };
        assert_eq!(year.day_hours(d).len(), expected, "local day {d}");
    }
}

#[test]
fn day_of_is_the_inverse_of_day_hours() {
    let year = dayahead_de_lu_2024();
    for d in 0..year.days() {
        for idx in year.day_hours(d) {
            assert_eq!(year.day_of(idx), d, "hour {idx}");
        }
    }
}

/// Europe/Berlin is a whole-hour offset from UTC, so every local midnight is
/// a UTC quarter-hour instant and the revenue meter can close its periods on
/// UTC multiples of 900 s without knowing the local clock.
#[test]
fn every_local_midnight_is_a_utc_quarter_hour() {
    let year = dayahead_de_lu_2024();
    let offset_s = i64::from(year.start_offset_h()) * 3600;
    for d in 0..=year.days() {
        let start = if d == year.days() {
            year.len()
        } else {
            year.day_hours(d).start
        };
        let utc_s = start as i64 * 3600 - offset_s;
        assert_eq!(utc_s.rem_euclid(900), 0, "local day {d}");
    }
}

#[test]
fn the_dataset_holds_35_136_quarter_hours() {
    let year = dayahead_de_lu_2024();
    let quarters: usize = (0..year.days()).map(|d| year.day_hours(d).len() * 4).sum();
    assert_eq!(quarters, 35_136);
}

/// Spot checks against the raw SMARD blocks, read by eye: the first hour of
/// local 2024-01-01, the year's peak (2024-12-12 17:00 CET) and trough
/// (2024-05-12 13:00 CEST).
#[test]
fn spot_prices_match_the_raw_blocks() {
    let year = dayahead_de_lu_2024();
    assert!((year.price_eur_mwh(0) - 0.10).abs() < 1e-9);
    assert!((year.price_eur_mwh(8321) - 936.28).abs() < 1e-9);
    assert!((year.price_eur_mwh(3180) + 135.45).abs() < 1e-9);
    let negative = year.prices_eur_mwh().iter().filter(|&&p| p < 0.0).count();
    assert_eq!(negative, 457);
}
