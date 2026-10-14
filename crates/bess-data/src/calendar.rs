//! Proleptic Gregorian day arithmetic shared by the artifact compilers.
//!
//! Both compilers turn calendar stamps into offsets from a year start, and
//! the price compiler also turns day indices back into dates for its report.
//! One copy of the algorithm, so the two artifacts cannot disagree about
//! which day a stamp falls on.

/// Days since 1970-01-01 of a civil date, after Howard Hinnant's algorithm
/// (the same calendar arithmetic the view layer uses).
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date `(year, month, day)` of a day count since 1970-01-01; the
/// inverse of [`days_from_civil`].
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, days_from_civil};

    #[test]
    fn known_dates_map_to_known_day_counts() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 1, 1), 19_723);
        assert_eq!(
            days_from_civil(2024, 3, 1) - days_from_civil(2024, 2, 28),
            2
        );
    }

    #[test]
    fn the_two_directions_are_inverses() {
        for days in days_from_civil(2023, 12, 1)..days_from_civil(2025, 2, 1) {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }
}
