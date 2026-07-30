//! Day-night model driven by the simulated clock. Pure functions.
//!
//! The sun window uses approximate German sunrise/sunset hours by month, so
//! a December replay is short and dark while May runs long and bright. This
//! is scenery, not science: the kernel's weather inputs stay authoritative
//! for anything physical.

use std::f32::consts::PI;

/// Lighting for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunLight {
    /// Normalized direction the light travels (FROM the light).
    pub dir: [f32; 3],
    /// Light color premultiplied by intensity.
    pub color: [f32; 3],
    /// 0 at night, 1 at full day.
    pub daylight: f32,
}

/// Approximate German sunrise/sunset hours by month (1..=12).
fn sun_window(month: u32) -> (f64, f64) {
    const SUNRISE: [f64; 12] = [8.2, 7.6, 6.7, 6.3, 5.5, 5.1, 5.3, 6.0, 6.8, 7.5, 7.6, 8.2];
    const SUNSET: [f64; 12] = [
        16.8, 17.6, 18.4, 20.2, 21.0, 21.5, 21.4, 20.6, 19.4, 18.3, 16.6, 16.3,
    ];
    let i = (month.clamp(1, 12) - 1) as usize;
    (SUNRISE[i], SUNSET[i])
}

/// Civil date and time (UTC) of a Unix timestamp:
/// (year, month, day, hour, minute, second). Calendar conversion after
/// Howard Hinnant's algorithm.
pub fn civil_from_unix(unix_time_s: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = unix_time_s.div_euclid(86_400);
    let secs = unix_time_s.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (if m <= 2 { y + 1 } else { y }) as i32;
    (
        year,
        m as u32,
        d as u32,
        (secs / 3600) as u32,
        (secs / 60 % 60) as u32,
        (secs % 60) as u32,
    )
}

/// `YYYY-MM-DD HH:MM:SS UTC` for panel headers.
pub fn format_utc(unix_time_s: i64) -> String {
    let (y, mo, d, h, mi, s) = civil_from_unix(unix_time_s);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// Month (1..=12) of a Unix timestamp, UTC.
pub fn month_of_unix(unix_time_s: i64) -> u32 {
    civil_from_unix(unix_time_s).1
}

/// Sun state for a Unix timestamp: direction, color, and daylight fraction.
pub fn sun_at(unix_time_s: i64) -> SunLight {
    let month = month_of_unix(unix_time_s);
    let hours = unix_time_s.rem_euclid(86_400) as f64 / 3600.0;
    let (rise, set) = sun_window(month);
    let t = ((hours - rise) / (set - rise)) as f32;
    let daylight = if (0.0..=1.0).contains(&t) {
        (t * PI).sin()
    } else {
        0.0
    };
    let az = PI * t.clamp(0.0, 1.0);
    let el = 0.15 + daylight * 0.95;
    let sun_v = [az.cos() * el.cos(), el.sin(), 0.35 * el.cos()];
    let norm = (sun_v[0] * sun_v[0] + sun_v[1] * sun_v[1] + sun_v[2] * sun_v[2]).sqrt();
    let day_dir = [-sun_v[0] / norm, -sun_v[1] / norm, -sun_v[2] / norm];
    let moon_dir = [-0.301_09, -0.822_98, -0.481_74]; // unit length
    let warm = daylight.powf(0.5);
    let day_col = [
        1.0 * daylight,
        (0.6 + 0.37 * warm) * daylight,
        (0.35 + 0.55 * warm) * daylight,
    ];
    let night = 1.0 - daylight;
    let color = [
        day_col[0] + 0.12 * night,
        day_col[1] + 0.15 * night,
        day_col[2] + 0.22 * night,
    ];
    let dir = if daylight > 0.05 { day_dir } else { moon_dir };
    SunLight {
        dir,
        color,
        daylight,
    }
}

#[cfg(test)]
mod tests {
    use super::{month_of_unix, sun_at};

    #[test]
    fn month_extraction_matches_known_dates() {
        assert_eq!(month_of_unix(0), 1); // 1970-01-01
        assert_eq!(month_of_unix(1_767_225_600), 1); // 2026-01-01
        assert_eq!(month_of_unix(1_767_225_600 + 181 * 86_400), 7); // 2026-07-01
        assert_eq!(month_of_unix(-86_400), 12); // 1969-12-31
    }

    #[test]
    fn civil_conversion_and_formatting_match_known_timestamps() {
        assert_eq!(super::civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(super::format_utc(1_767_225_600), "2026-01-01 00:00:00 UTC");
        // 2026-07-14 10:30:00 UTC
        let t = 1_767_225_600 + 194 * 86_400 + 10 * 3600 + 30 * 60;
        assert_eq!(super::format_utc(t), "2026-07-14 10:30:00 UTC");
    }

    #[test]
    fn january_noon_is_day_and_midnight_is_night() {
        let jan1 = 1_767_225_600;
        assert!(sun_at(jan1 + 12 * 3600).daylight > 0.5);
        assert!(sun_at(jan1).daylight < 0.01);
    }

    #[test]
    fn july_evening_is_still_bright_but_january_evening_is_dark() {
        let jan1 = 1_767_225_600;
        let jul1 = jan1 + 181 * 86_400;
        let at_1930 = |day0: i64| sun_at(day0 + 19 * 3600 + 1800).daylight;
        assert!(at_1930(jul1) > 0.3);
        assert!(at_1930(jan1) < 0.01);
    }

    #[test]
    fn light_direction_is_normalized() {
        for h in 0..24 {
            let s = sun_at(1_767_225_600 + h * 3600);
            let len = (s.dir[0] * s.dir[0] + s.dir[1] * s.dir[1] + s.dir[2] * s.dir[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-3, "hour {h}: |dir| = {len}");
        }
    }
}
