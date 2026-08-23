//! Which week of the year is worth drawing.
//!
//! Its own file because it answers a different question from the collector
//! next door. That one asks what happened; this one asks where to look, and
//! it asks the dataset rather than the run: the window has to be known from
//! the first tick, or the collector would have to buffer the whole year on
//! the chance some part of it turned out to be interesting.

use bess_models::{month_day_from_unix_days, HistoricalWeather};

use crate::run::RunSpec;

/// Seconds in a day.
const DAY_S: u64 = 86_400;

/// Sampling interval of the published trace, seconds. A quarter of an hour
/// is the market's own resolution and plenty for a chart: the thermal
/// quantities it draws move over tens of minutes.
pub const TRACE_INTERVAL_S: u64 = 900;

/// Days either side of the warmest day that the trace covers.
const TRACE_HALF_WIDTH_DAYS: i64 = 3;

/// The window the trace covers: the warmest day of the replayed year with
/// three days either side, in simulated time.
///
/// Found in the dataset before the run rather than discovered during it, so
/// the collector knows from the first tick which week to keep and nothing
/// has to be buffered on the chance it turns out to be interesting.
pub fn hot_week(weather: HistoricalWeather, spec: RunSpec) -> ((i64, i64), String) {
    let year = weather.year();
    let temps = year.temp_c();
    let mut warmest = (0usize, f64::NEG_INFINITY);
    for (day, hours) in temps.chunks(24).enumerate() {
        if hours.len() < 24 {
            break;
        }
        let mean = hours.iter().map(|t| f64::from(*t)).sum::<f64>() / 24.0;
        if mean > warmest.1 {
            warmest = (day, mean);
        }
    }

    // The dataset's day is a calendar date; the run's day is a timestamp.
    // Walk the simulated year and take the day that shares the date.
    let (target_month, target_day) =
        month_day_from_unix_days(YEAR_START_UNIX_S / 86_400 + warmest.0 as i64);
    let mut centre = spec.start_unix_s;
    for day in 0..spec.days as i64 {
        let at = spec.start_unix_s + day * DAY_S as i64;
        let (month, dom) = month_day_from_unix_days(at.div_euclid(86_400));
        if (month, dom) == (target_month, target_day) {
            centre = at;
            break;
        }
    }

    let half = TRACE_HALF_WIDTH_DAYS * DAY_S as i64;
    let start = (centre - half).max(spec.start_unix_s);
    let end =
        (centre + DAY_S as i64 + half).min(spec.start_unix_s + spec.days as i64 * DAY_S as i64);
    (
        (start, end),
        format!(
            "The warmest week of the replayed year, centred on {target_day:02}-{target_month:02}"
        ),
    )
}

/// 2024-01-01 00:00:00 UTC, in days: the reference year the dataset covers.
pub const YEAR_START_UNIX_S: i64 = 1_704_067_200;
