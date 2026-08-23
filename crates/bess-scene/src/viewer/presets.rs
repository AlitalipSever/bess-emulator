//! One-click stops, computed from the dataset rather than chosen by hand.
//!
//! "The warmest day of the year" is a fact about the compiled series, so it
//! is derived from the series. Hard-coding 14 July would be a number that
//! silently stops being true the first time the reference year changes, and
//! the whole point of these is that a demo lands on a day worth watching
//! without anyone having gone looking for it first.
//!
//! A stop is a calendar date rather than a timestamp, because that is how the
//! replay maps: `HistoricalWeather` looks up the reference year by month and
//! day, so the same stop works whatever year the simulated clock says.

use bess_models::{HourSample, PrecipForm, WeatherYear};

use crate::clock::civil_from_unix;
use crate::panels::Stop;

/// 2024-01-01 00:00:00 UTC, the first hour of the reference year. Used only
/// to turn a day index into a calendar date.
const REFERENCE_YEAR_START: i64 = 1_704_067_200;

/// Hours the plant opens at when a stop is picked: mid-morning, so the day
/// it is showing is ahead rather than behind.
const OPENING_HOUR: u32 = 9;

/// The stops for a compiled year, in the order they are offered.
pub fn stops(year: &WeatherYear) -> Vec<Stop> {
    let mut out = Vec::with_capacity(5);
    if let Some(day) = extreme_day(year.temp_c(), Extreme::Highest) {
        out.push(from_day("Warmest day", day));
    }
    if let Some(day) = extreme_day(year.temp_c(), Extreme::Lowest) {
        out.push(from_day("Coldest day", day));
    }
    if let Some(day) = extreme_day(year.ghi_wm2(), Extreme::Highest) {
        out.push(from_day("Sunniest day", day));
    }
    // Rain is the exception, and finding out why cost a screenshot. The
    // wettest day of the reference year holds 34.7 mm, of which 32.9 falls
    // in the single hour at 20:00; a stop that picked the day and landed at
    // nine in the morning arrived in the dry. Anything this spiky has to be
    // found by the hour it happened in, not by the day it belongs to.
    if let Some(hour) = wettest_hour(year, |_| true) {
        out.push(from_hour("Heaviest rain", hour));
    }
    if let Some(hour) = wettest_hour(year, |s| s.precip_form == PrecipForm::Snow) {
        out.push(from_hour("Snowfall", hour));
    }
    out
}

/// The hour with the most precipitation among those the filter accepts.
fn wettest_hour(year: &WeatherYear, accept: impl Fn(&HourSample) -> bool) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for hour in 0..year.len() {
        let sample = year.hour(hour);
        if sample.precip_mm <= 0.05 || !accept(&sample) {
            continue;
        }
        if best.is_none_or(|(_, mm)| sample.precip_mm > mm) {
            best = Some((hour, sample.precip_mm));
        }
    }
    best.map(|(hour, _)| hour)
}

/// Which end of the range a stop is looking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Extreme {
    Highest,
    Lowest,
}

/// The day whose total is the most extreme, as a day index into the year.
///
/// Daily totals rather than single hours, deliberately. The hottest single
/// hour of the year can belong to an otherwise ordinary day, and someone who
/// clicks "warmest day" wants the day that was warm, not the day that held
/// one warm hour.
fn extreme_day(hourly: &[f32], want: Extreme) -> Option<usize> {
    if hourly.len() < 24 {
        return None;
    }
    let mut best = None;
    for (day, hours) in hourly.chunks(24).enumerate() {
        if hours.len() < 24 {
            break;
        }
        let total: f32 = hours.iter().map(|v| f64::from(*v) as f32).sum();
        let better = match (best, want) {
            (None, _) => true,
            (Some((_, b)), Extreme::Highest) => total > b,
            (Some((_, b)), Extreme::Lowest) => total < b,
        };
        if better {
            best = Some((day, total));
        }
    }
    best.map(|(day, _)| day)
}

/// A day of the reference year, opened mid-morning.
fn from_day(label: &'static str, day_index: usize) -> Stop {
    let (_, month, day, ..) = civil_from_unix(REFERENCE_YEAR_START + day_index as i64 * 86_400);
    Stop {
        label,
        month,
        day,
        hour: OPENING_HOUR,
    }
}

/// A specific hour of the reference year, opened just before it so the
/// weather arrives rather than being already there.
fn from_hour(label: &'static str, hour_index: usize) -> Stop {
    let start = hour_index.saturating_sub(1);
    let (_, month, day, hour, ..) = civil_from_unix(REFERENCE_YEAR_START + start as i64 * 3600);
    Stop {
        label,
        month,
        day,
        hour,
    }
}

#[cfg(test)]
mod tests {
    use super::{civil_from_unix, extreme_day, stops, Extreme, REFERENCE_YEAR_START};

    #[test]
    fn the_stops_are_distinct_moments_of_the_reference_year() {
        let year = bess_data::lindenberg_2024();
        let found = stops(year);
        assert_eq!(found.len(), 5, "{found:?}");
        for stop in &found {
            assert!((1..=12).contains(&stop.month), "{stop:?}");
            assert!((1..=31).contains(&stop.day), "{stop:?}");
            assert!(stop.hour < 24, "{stop:?}");
        }
        let mut moments: Vec<_> = found.iter().map(|s| (s.month, s.day, s.hour)).collect();
        moments.sort_unstable();
        moments.dedup();
        assert_eq!(
            moments.len(),
            found.len(),
            "two stops land on the same hour"
        );
    }

    #[test]
    fn a_rain_stop_lands_where_it_is_actually_raining() {
        // The finding this test exists for: the wettest *day* of the year
        // puts 32.9 of its 34.7 mm into the single hour at 20:00, so a stop
        // that picked the day and opened at nine in the morning arrived in
        // the dry and looked like a broken feature.
        let year = bess_data::lindenberg_2024();
        let found = stops(year);
        let rain = found
            .iter()
            .find(|s| s.label == "Heaviest rain")
            .expect("a rain stop");

        let mut wet_within_reach = false;
        for hour in 0..year.len() {
            let (_, month, day, h, ..) = civil_from_unix(REFERENCE_YEAR_START + hour as i64 * 3600);
            if (month, day) != (rain.month, rain.day) {
                continue;
            }
            if h >= rain.hour && h <= rain.hour + 2 && year.hour(hour).precip_mm > 1.0 {
                wet_within_reach = true;
            }
        }
        assert!(
            wet_within_reach,
            "the rain stop opens at {:02}:00 on {}-{} with no rain within two hours",
            rain.hour, rain.month, rain.day
        );
    }

    #[test]
    fn the_snow_stop_falls_in_a_month_that_can_hold_snow() {
        let year = bess_data::lindenberg_2024();
        let found = stops(year);
        let snow = found
            .iter()
            .find(|s| s.label == "Snowfall")
            .expect("a snow stop");
        assert!(
            snow.month <= 4 || snow.month >= 10,
            "snow landed in month {}",
            snow.month
        );
    }

    #[test]
    fn the_warm_stop_is_in_summer_and_the_cold_one_is_not() {
        // A weak claim on purpose: it holds for any plausible year at this
        // latitude without pinning the answer to one dataset, so refreshing
        // the compiled year does not rewrite the test.
        let year = bess_data::lindenberg_2024();
        let found = stops(year);
        let warmest = found[0];
        let coldest = found[1];
        assert!(
            (5..=9).contains(&warmest.month),
            "warmest day landed in month {}",
            warmest.month
        );
        assert!(
            coldest.month <= 3 || coldest.month >= 11,
            "coldest day landed in month {}",
            coldest.month
        );
    }

    #[test]
    fn a_day_is_picked_by_its_whole_total_not_by_one_hour() {
        // Day 1 holds a single enormous hour; day 0 is uniformly warmer over
        // the whole day. The stop belongs to day 0.
        let mut hourly = vec![10.0f32; 48];
        for h in hourly.iter_mut().take(24) {
            *h = 20.0;
        }
        hourly[30] = 100.0;
        assert_eq!(extreme_day(&hourly, Extreme::Highest), Some(0));
        assert_eq!(extreme_day(&hourly, Extreme::Lowest), Some(1));
    }

    #[test]
    fn a_short_series_offers_nothing_rather_than_a_wrong_day() {
        assert_eq!(extreme_day(&[1.0; 12], Extreme::Highest), None);
        assert_eq!(extreme_day(&[], Extreme::Lowest), None);
        // A trailing partial day is ignored, not counted as a whole one.
        let mut hourly = vec![1.0f32; 24];
        hourly.extend([99.0f32; 5]);
        assert_eq!(extreme_day(&hourly, Extreme::Highest), Some(0));
    }

    #[test]
    fn the_all_day_stops_open_mid_morning() {
        // Temperature and sunshine are day-long facts, so those stops still
        // open at nine with the day ahead of them.
        let year = bess_data::lindenberg_2024();
        let found = stops(year);
        for label in ["Warmest day", "Coldest day", "Sunniest day"] {
            let stop = found.iter().find(|s| s.label == label).expect(label);
            assert_eq!(stop.hour, 9, "{label} opened at {}", stop.hour);
        }
    }
}
