//! Exogenous input drivers: replayed observations and a synthetic fallback.
//!
//! [`HistoricalWeather`] is the driver the reference site runs on from M1:
//! it replays a compiled observation year from bess-data, so the emulator's
//! July 14th is the actual July 14th. [`SyntheticWeather`] stays available
//! for tests and offline use where a smooth, dataset-free input is easier to
//! reason about than a real day.
//!
//! Grid frequency is synthetic in both drivers. Frequency replay is an M4
//! deliverable tied to grid-code behavior; until then a driver that pretended
//! to replay it would be inventing a signal.

use std::f64::consts::{PI, TAU};
use std::fmt;

use bess_core::kernel::{Inputs, Weather};
use bess_data::{HourSample, WeatherYear};

/// Grid frequency at a UTC timestamp: 50 Hz plus a +/- 10 mHz wander on a
/// 10 minute period. Deterministic and shared by both drivers.
pub fn synthetic_grid_frequency_hz(unix_time_s: i64) -> f64 {
    let second_of_day = unix_time_s.rem_euclid(86_400) as f64;
    50.0 + 0.01 * (TAU * second_of_day / 600.0).sin()
}

/// Synthetic diurnal weather and grid-frequency driver.
///
/// A daily temperature sinusoid and a daylight irradiance arc, purely a
/// function of the timestamp. Every day is the same day, which is exactly
/// what a unit test wants and exactly what a calibration run does not.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticWeather {
    /// Daily mean temperature, degrees Celsius.
    pub mean_c: f64,
    /// Half of the daily temperature swing, K.
    pub amplitude_c: f64,
}

impl Default for SyntheticWeather {
    fn default() -> Self {
        Self {
            mean_c: 14.0,
            amplitude_c: 8.0,
        }
    }
}

impl SyntheticWeather {
    /// Inputs for a given UTC timestamp. Temperature peaks at 15:00,
    /// irradiance follows a sine arc between 06:00 and 18:00, frequency
    /// wanders as in [`synthetic_grid_frequency_hz`].
    pub fn inputs_at(&self, unix_time_s: i64) -> Inputs {
        let second_of_day = unix_time_s.rem_euclid(86_400) as f64;
        let hour = second_of_day / 3600.0;
        let ambient_c = self.mean_c + self.amplitude_c * (TAU * (hour - 9.0) / 24.0).sin();
        let irradiance_wm2 = if (6.0..18.0).contains(&hour) {
            900.0 * (PI * (hour - 6.0) / 12.0).sin()
        } else {
            0.0
        };
        Inputs {
            weather: Weather {
                ambient_c,
                irradiance_wm2,
            },
            grid_frequency_hz: synthetic_grid_frequency_hz(unix_time_s),
        }
    }
}

/// Replay driver over a compiled observation year.
///
/// The dataset is a fixed, hash-pinned release artifact, so replay keeps the
/// determinism contract: same seed + scenario + dataset = byte-identical
/// output.
///
/// **Calendar mapping.** A simulation may start in any year; the driver maps
/// its UTC calendar date onto the same date of the reference year, so a run
/// in July replays July weather no matter which year the clock says. The
/// mapping is by month and day rather than by elapsed time, which is what
/// keeps the seasons aligned across the leap-year offset. February 29th of a
/// simulated leap year lands on the reference year's February 29th when that
/// year is a leap year too (2024 is), and continues into March 1st when it
/// is not.
///
/// **Interpolation.** Hourly source values feed the 1 s tick linearly. The
/// two series carry different time semantics and are placed accordingly:
/// temperature is an instantaneous observation stamped at its hour, so it
/// interpolates between stamps; irradiance is an hourly mean, so it is
/// placed at the bucket midpoint and interpolated between midpoints. Either
/// way sub-hour transients are lost, which matters least for exactly the two
/// quantities driving a container with an hours-long thermal time constant.
/// The last hour of the year interpolates into the first: a replayed year is
/// a loop.
#[derive(Clone, Copy)]
pub struct HistoricalWeather {
    year: &'static WeatherYear,
}

impl fmt::Debug for HistoricalWeather {
    /// Prints what identifies the dataset, not its 8784 hours.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistoricalWeather")
            .field("year", &self.year.year())
            .field("station_id", &self.year.station_id())
            .finish()
    }
}

impl HistoricalWeather {
    /// Replay the DWD Lindenberg (Mark) 2024 year bundled with the release.
    pub fn lindenberg_2024() -> Self {
        Self::new(bess_data::lindenberg_2024())
    }

    /// Replay a caller-supplied compiled year.
    pub fn new(year: &'static WeatherYear) -> Self {
        Self { year }
    }

    /// The dataset being replayed.
    pub fn year(&self) -> &'static WeatherYear {
        self.year
    }

    /// Inputs for a given UTC timestamp.
    pub fn inputs_at(&self, unix_time_s: i64) -> Inputs {
        let position_h = self.position_h(unix_time_s);
        Inputs {
            weather: Weather {
                ambient_c: interpolate(self.year.temp_c(), position_h),
                irradiance_wm2: interpolate(self.year.ghi_wm2(), position_h - 0.5),
            },
            grid_frequency_hz: synthetic_grid_frequency_hz(unix_time_s),
        }
    }

    /// The whole observation bucket a timestamp falls in.
    ///
    /// `inputs_at` returns only what the physics consumes, interpolated.
    /// This returns the hour as observed, every series of it, for the view
    /// layer's scenery. Not interpolated: cloud cover is an integer count of
    /// eighths and precipitation form is a category, and averaging either
    /// across an hour boundary would invent a reading nobody took.
    pub fn hour_at(&self, unix_time_s: i64) -> HourSample {
        let hours = self.position_h(unix_time_s) as usize;
        self.year.hour(hours.min(self.year.len() - 1))
    }

    /// Position of a timestamp inside the reference year, in fractional
    /// hours since its first hour.
    fn position_h(self, unix_time_s: i64) -> f64 {
        let (month, day) = month_day_from_unix_days(unix_time_s.div_euclid(86_400));
        let second_of_day = unix_time_s.rem_euclid(86_400) as f64;
        let mut day_of_year = DAYS_BEFORE_MONTH[month - 1] + day - 1;
        if month > 2 && is_leap_year(self.year.year()) {
            day_of_year += 1;
        }
        f64::from(day_of_year * 24) + second_of_day / 3600.0
    }
}

/// Days elapsed before the first of each month in a common year.
const DAYS_BEFORE_MONTH: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// Civil month (1-12) and day of month from days since the Unix epoch.
/// Howard Hinnant's `civil_from_days`, with the year dropped: the reference
/// year supplies the year, the timestamp supplies the position within it.
fn month_day_from_unix_days(days: i64) -> (usize, u32) {
    // Shift the epoch to 0000-03-01 so leap days land at the end of the era.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shifted = (5 * day_of_year + 2) / 153; // [0, 11], 0 = March
    let day = (day_of_year - (153 * month_shifted + 2) / 5 + 1) as u32;
    let month = if month_shifted < 10 {
        month_shifted + 3
    } else {
        month_shifted - 9
    };
    (month as usize, day)
}

/// Linear interpolation over an hourly series, wrapping at the year end.
fn interpolate(series: &[f32], position_h: f64) -> f64 {
    let len = series.len();
    let position = position_h.rem_euclid(len as f64);
    // rem_euclid can return the modulus itself for tiny negative inputs.
    let idx = (position as usize).min(len - 1);
    let frac = (position - idx as f64).clamp(0.0, 1.0);
    let a = f64::from(series[idx]);
    let b = f64::from(series[(idx + 1) % len]);
    a + (b - a) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2024-01-01 00:00:00 UTC, the first hour of the reference year.
    const REFERENCE_START_S: i64 = 1_704_067_200;
    /// 2026-01-01 00:00:00 UTC.
    const START_2026_S: i64 = 1_767_225_600;

    #[test]
    fn temperature_peaks_mid_afternoon() {
        let w = SyntheticWeather::default();
        let at_15 = w.inputs_at(15 * 3600).weather.ambient_c;
        let at_03 = w.inputs_at(3 * 3600).weather.ambient_c;
        assert!(at_15 > at_03);
        assert!((at_15 - (w.mean_c + w.amplitude_c)).abs() < 1.0e-9);
    }

    #[test]
    fn irradiance_is_zero_at_night() {
        let w = SyntheticWeather::default();
        assert!(w.inputs_at(2 * 3600).weather.irradiance_wm2.abs() < f64::EPSILON);
        assert!(w.inputs_at(12 * 3600).weather.irradiance_wm2 > 800.0);
    }

    #[test]
    fn frequency_stays_near_nominal() {
        for s in (0..86_400).step_by(97) {
            let f = synthetic_grid_frequency_hz(s);
            assert!((49.98..50.02).contains(&f));
        }
    }

    #[test]
    fn replay_reproduces_the_stamped_observation() {
        let driver = HistoricalWeather::lindenberg_2024();
        let observed = driver.year().hour(0).temp_c;
        let replayed = driver.inputs_at(REFERENCE_START_S).weather.ambient_c;
        assert!((replayed - f64::from(observed)).abs() < 1.0e-12);
    }

    #[test]
    fn half_past_the_hour_is_the_midpoint_of_two_stamps() {
        let driver = HistoricalWeather::lindenberg_2024();
        let temps = driver.year().temp_c();
        let expected = f64::midpoint(f64::from(temps[0]), f64::from(temps[1]));
        let replayed = driver.inputs_at(REFERENCE_START_S + 1800).weather.ambient_c;
        assert!((replayed - expected).abs() < 1.0e-12);
    }

    /// Hourly irradiance means are placed at their bucket midpoint, so the
    /// half hour reads the bucket exactly and the stamp reads the average of
    /// the two buckets it separates.
    #[test]
    fn irradiance_means_sit_at_their_bucket_midpoint() {
        let driver = HistoricalWeather::lindenberg_2024();
        let ghi = driver.year().ghi_wm2();
        // 2024-06-21, the buckets around noon UTC.
        let noon = (173 - 1) * 24 + 12;
        let at_bucket_start = REFERENCE_START_S + noon as i64 * 3600;

        let midpoint = driver
            .inputs_at(at_bucket_start + 1800)
            .weather
            .irradiance_wm2;
        assert!((midpoint - f64::from(ghi[noon])).abs() < 1.0e-12);
        assert!(midpoint > 100.0, "midsummer noon must be bright");

        let expected_at_stamp = f64::midpoint(f64::from(ghi[noon - 1]), f64::from(ghi[noon]));
        let at_stamp = driver.inputs_at(at_bucket_start).weather.irradiance_wm2;
        assert!((at_stamp - expected_at_stamp).abs() < 1.0e-12);
    }

    #[test]
    fn any_simulated_year_replays_the_same_calendar_day() {
        let driver = HistoricalWeather::lindenberg_2024();
        // 2026-07-14 11:00 UTC (the viewer's start) against 2024-07-14 11:00.
        let sim = START_2026_S + 194 * 86_400 + 11 * 3600;
        let reference = REFERENCE_START_S + 195 * 86_400 + 11 * 3600; // leap year
                                                                      // Bit-exact: the same date must resolve to the same dataset position.
        assert_eq!(
            driver.inputs_at(sim).weather.ambient_c.to_bits(),
            driver.inputs_at(reference).weather.ambient_c.to_bits()
        );
        assert_eq!(
            driver.inputs_at(sim).weather.irradiance_wm2.to_bits(),
            driver.inputs_at(reference).weather.irradiance_wm2.to_bits()
        );
    }

    #[test]
    fn a_simulated_leap_day_lands_on_the_reference_leap_day() {
        let driver = HistoricalWeather::lindenberg_2024();
        // 2028-02-29 12:00 UTC; February 29th is day-of-year index 59.
        let sim = 1_830_297_600 + 59 * 86_400 + 12 * 3600;
        let expected = f64::from(driver.year().temp_c()[59 * 24 + 12]);
        assert!((driver.inputs_at(sim).weather.ambient_c - expected).abs() < 1.0e-12);
    }

    #[test]
    fn the_year_wraps_without_a_seam() {
        let driver = HistoricalWeather::lindenberg_2024();
        let temps = driver.year().temp_c();
        let expected = f64::midpoint(f64::from(temps[temps.len() - 1]), f64::from(temps[0]));
        // 2024-12-31 23:30 UTC.
        let last_half_hour = REFERENCE_START_S + (365 * 24 + 23) * 3600 + 1800;
        assert!((driver.inputs_at(last_half_hour).weather.ambient_c - expected).abs() < 1.0e-12);
    }

    #[test]
    fn frequency_stays_synthetic_under_replay() {
        let driver = HistoricalWeather::lindenberg_2024();
        let t = START_2026_S + 12_345;
        assert_eq!(
            driver.inputs_at(t).grid_frequency_hz.to_bits(),
            synthetic_grid_frequency_hz(t).to_bits()
        );
    }

    /// Every tick of a replayed year stays inside the bounds the artifact
    /// tests pin, interpolation included.
    #[test]
    fn replayed_inputs_stay_inside_physical_bounds() {
        let driver = HistoricalWeather::lindenberg_2024();
        let mut t = START_2026_S;
        let end = START_2026_S + 365 * 86_400;
        while t < end {
            let weather = driver.inputs_at(t).weather;
            assert!(
                (-35.0..50.0).contains(&weather.ambient_c),
                "ambient {} at {t}",
                weather.ambient_c
            );
            assert!(
                (0.0..1200.0).contains(&weather.irradiance_wm2),
                "irradiance {} at {t}",
                weather.irradiance_wm2
            );
            t += 97;
        }
    }
}
