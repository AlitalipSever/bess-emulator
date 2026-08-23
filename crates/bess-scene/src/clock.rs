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

#[cfg(test)]
mod tests {
    use super::{civil_from_unix, format_utc};

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
}
