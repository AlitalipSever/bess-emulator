//! Where a simulated instant lands in a replayed reference year.
//!
//! The weather driver, the day-ahead planner, the shell's price gauge and
//! the bench all ask the same question, "which reference hour is now?", and
//! this module is the one answer. A simulated timestamp keeps its month, day
//! and time of day and borrows the reference year's calendar, so the
//! emulator's July 14th replays July 14th in any simulated year. A
//! simulated December 31st 23:00 and the following January 1st 00:00 are
//! consecutive reference hours: the year wraps without a seam.
//!
//! February 29th of a simulated leap year lands on the reference leap day;
//! in a common simulated year the reference leap day is never visited.

/// Days elapsed before the first of each month in a common year.
const DAYS_BEFORE_MONTH: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// Position of a UTC timestamp inside reference year `year`, in fractional
/// hours since its first hour (00:00 UTC on January 1st).
pub fn reference_position_h(unix_time_s: i64, year: u16) -> f64 {
    let (day_of_year, second_of_day) = reference_day_and_second(unix_time_s, year);
    f64::from(day_of_year * 24) + second_of_day as f64 / 3600.0
}

/// The second of reference year `year` a timestamp maps onto, counted
/// from 00:00 UTC on January 1st, computed without a float (M4: what a
/// one-second replay indexes by).
pub fn reference_second(unix_time_s: i64, year: u16) -> i64 {
    let (day_of_year, second_of_day) = reference_day_and_second(unix_time_s, year);
    i64::from(day_of_year) * 86_400 + second_of_day
}

/// The whole UTC hour of reference year `year` a timestamp falls in: the
/// floor of [`reference_position_h`], computed without a float.
pub fn reference_hour(unix_time_s: i64, year: u16) -> usize {
    let (day_of_year, second_of_day) = reference_day_and_second(unix_time_s, year);
    day_of_year as usize * 24 + (second_of_day / 3600) as usize
}

fn reference_day_and_second(unix_time_s: i64, year: u16) -> (u32, i64) {
    let (month, day) = month_day_from_unix_days(unix_time_s.div_euclid(86_400));
    let mut day_of_year = DAYS_BEFORE_MONTH[month - 1] + day - 1;
    if month > 2 && is_leap_year(year) {
        day_of_year += 1;
    }
    (day_of_year, unix_time_s.rem_euclid(86_400))
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// Calendar month (1-12) and day of month of a count of days since the
/// Unix epoch, UTC. Howard Hinnant's `civil_from_days`, with the year
/// dropped: the reference year supplies the year, the timestamp supplies
/// the position within it.
///
/// Public because the calendar is not any one driver's private business:
/// `bess-bench` needs the same answer to break a year into months, and a
/// second copy of the algorithm in this workspace would be one copy too
/// many.
pub fn month_day_from_unix_days(days: i64) -> (usize, u32) {
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

#[cfg(test)]
mod tests {
    use super::{reference_hour, reference_position_h};

    /// 2026-01-01 00:00 UTC.
    const START_2026_S: i64 = 1_767_225_600;

    #[test]
    fn the_hour_is_the_floor_of_the_position() {
        for offset_s in [0, 1, 3599, 3600, 86_399, 200 * 86_400 + 5_000] {
            let t = START_2026_S + offset_s;
            assert_eq!(
                reference_hour(t, 2024),
                reference_position_h(t, 2024).floor() as usize
            );
        }
    }

    #[test]
    fn march_first_skips_the_leap_day_of_a_leap_reference() {
        // 2026-03-01 00:00 UTC is day 59 of a common year, day 60 of 2024.
        let t = START_2026_S + 59 * 86_400;
        assert_eq!(reference_hour(t, 2024), 60 * 24);
        assert_eq!(reference_hour(t, 2023), 59 * 24);
    }

    #[test]
    fn the_last_hour_of_any_year_is_the_last_reference_hour() {
        // 2026-12-31 23:00 UTC against the 8784-hour reference year.
        let t = START_2026_S + 364 * 86_400 + 23 * 3600;
        assert_eq!(reference_hour(t, 2024), 8783);
    }
}
