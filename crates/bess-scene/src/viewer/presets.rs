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

use bess_models::WeatherYear;

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
    let mut out = Vec::with_capacity(4);
    if let Some(day) = extreme_day(year.temp_c(), Extreme::Highest) {
        out.push(stop("Warmest day", day));
    }
    if let Some(day) = extreme_day(year.temp_c(), Extreme::Lowest) {
        out.push(stop("Coldest day", day));
    }
    if let Some(day) = extreme_day(year.ghi_wm2(), Extreme::Highest) {
        out.push(stop("Sunniest day", day));
    }
    if let Some(day) = extreme_day(year.precip_mm(), Extreme::Highest) {
        out.push(stop("Wettest day", day));
    }
    out
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

/// A day index of the reference year, as a calendar date.
fn stop(label: &'static str, day_index: usize) -> Stop {
    let (_, month, day, ..) = civil_from_unix(REFERENCE_YEAR_START + day_index as i64 * 86_400);
    Stop {
        label,
        month,
        day,
        hour: OPENING_HOUR,
    }
}

#[cfg(test)]
mod tests {
    use super::{extreme_day, stops, Extreme};

    #[test]
    fn the_stops_are_four_distinct_days_of_the_reference_year() {
        let year = bess_data::lindenberg_2024();
        let found = stops(year);
        assert_eq!(found.len(), 4);
        for stop in &found {
            assert!((1..=12).contains(&stop.month), "{stop:?}");
            assert!((1..=31).contains(&stop.day), "{stop:?}");
        }
        // Warm and cold cannot be the same day, and neither can wet and sunny.
        assert_ne!(
            (found[0].month, found[0].day),
            (found[1].month, found[1].day)
        );
        assert_ne!(
            (found[2].month, found[2].day),
            (found[3].month, found[3].day)
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
    fn a_stop_opens_mid_morning() {
        let year = bess_data::lindenberg_2024();
        assert!(stops(year).iter().all(|s| s.hour == 9));
    }
}
