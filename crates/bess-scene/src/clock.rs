//! Calendar arithmetic for the simulated clock.
//!
//! Split out of `sun` because it never belonged there: converting a Unix
//! timestamp to a civil date has nothing to do with the sun, and the panels
//! that format the clock have nothing to do with lighting. It lived there
//! only because the sun module needed the month for a lookup table that no
//! longer exists.

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

/// Unix timestamp of a civil UTC date and time. The inverse of
/// [`civil_from_unix`], after the same algorithm.
///
/// Out-of-range components are not rejected: month 13 rolls into the next
/// year and day 32 into the next month, which is what the arithmetic does
/// naturally and what a date picker wants when someone drags a field past
/// its end.
pub fn unix_from_civil(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
}

#[cfg(test)]
mod tests {
    use super::{civil_from_unix, format_utc, unix_from_civil};

    /// 2026-01-01 00:00:00 UTC.
    const NEW_YEAR: i64 = 1_767_225_600;

    #[test]
    fn civil_conversion_and_formatting_match_known_timestamps() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil_from_unix(-86_400).1, 12);
        assert_eq!(format_utc(NEW_YEAR), "2026-01-01 00:00:00 UTC");
        let t = NEW_YEAR + 194 * 86_400 + 10 * 3600 + 30 * 60;
        assert_eq!(format_utc(t), "2026-07-14 10:30:00 UTC");
    }

    #[test]
    fn the_two_conversions_are_inverses() {
        // Every day across four years, including two leap days and the
        // century-rule edge that catches naive implementations.
        let mut t = unix_from_civil(2023, 1, 1, 0, 0, 0);
        for step in 0..(4 * 366) {
            let stamp = t + step * 86_400 + 13 * 3600 + 47 * 60 + 11;
            let (y, mo, d, h, mi, s) = civil_from_unix(stamp);
            assert_eq!(
                unix_from_civil(y, mo, d, h, mi, s),
                stamp,
                "round trip broke at {}",
                format_utc(stamp)
            );
        }
        t = unix_from_civil(1900, 3, 1, 0, 0, 0);
        assert_eq!(civil_from_unix(t), (1900, 3, 1, 0, 0, 0));
        assert_eq!(unix_from_civil(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(unix_from_civil(2024, 2, 29, 12, 0, 0) % 86_400, 43_200);
    }

    #[test]
    fn a_day_dragged_past_its_end_rolls_over() {
        // What a date field does when someone holds the up arrow.
        assert_eq!(
            civil_from_unix(unix_from_civil(2026, 1, 32, 0, 0, 0)),
            (2026, 2, 1, 0, 0, 0)
        );
        assert_eq!(
            civil_from_unix(unix_from_civil(2026, 2, 29, 0, 0, 0)),
            (2026, 3, 1, 0, 0, 0)
        );
    }
}
