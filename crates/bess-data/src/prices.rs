//! The German day-ahead price year the planner replays.
//!
//! SMARD's wholesale day-ahead price for the DE-LU bidding zone, 2024,
//! hourly, stored as the Europe/Berlin year: 2023-12-31 23:00 UTC to
//! 2024-12-31 22:00 UTC, 8784 hours. A plan day is a local day, and the
//! artifact carries the local-day table that says where each one starts,
//! so the two DST days are 23 and 25 hours long with no timezone code
//! anywhere in the simulation. The weather artifact holds the UTC year;
//! the header's start offset (+1 h) is what maps one onto the other, so the
//! mapping is data rather than a constant in code.
//!
//! Prices have no physical continuity that would justify interpolation, so
//! the series is strict: the compiler refuses a missing or repeated hour.

pub mod codec;

use std::ops::Range;
use std::sync::OnceLock;

use crate::fnv1a_64;

/// Pinned FNV-1a 64 hash of the bundled DE-LU 2024 day-ahead artifact.
///
/// [`dayahead_de_lu_2024`] refuses to serve data whose bytes do not hash to
/// this value. When the artifact is legitimately regenerated,
/// `compile-prices` prints the new hash and this constant moves with it in
/// the same commit.
pub const DAYAHEAD_DE_LU_2024_FNV1A: u64 = 0x5674_7c44_c7d6_7904;

/// One replayed price year: hourly prices plus its local-day table.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceYear {
    year: u16,
    start_offset_h: i16,
    eur_mwh: Vec<f64>,
    day_starts: Vec<u16>,
}

impl PriceYear {
    /// Builds a year from its parts. The loader goes through here, and so do
    /// tests that need a small hand-made year instead of the bundled one.
    ///
    /// # Errors
    /// Returns an error when `day_starts` does not describe `eur_mwh` (it
    /// must run from 0 to the series length in days of 23 to 25 hours).
    #[doc(hidden)]
    pub fn from_parts(
        year: u16,
        start_offset_h: i16,
        eur_mwh: Vec<f64>,
        day_starts: Vec<u16>,
    ) -> Result<Self, String> {
        codec::check_day_starts(&day_starts, eur_mwh.len())?;
        Ok(Self {
            year,
            start_offset_h,
            eur_mwh,
            day_starts,
        })
    }

    /// Calendar year of the local days.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Hours the series starts after 00:00 UTC on January 1st: UTC
    /// reference hour `h` reads price index `(h + offset) mod len`.
    pub fn start_offset_h(&self) -> i16 {
        self.start_offset_h
    }

    /// Number of hourly prices.
    pub fn len(&self) -> usize {
        self.eur_mwh.len()
    }

    /// True when the year holds no hours (never for a loaded artifact).
    pub fn is_empty(&self) -> bool {
        self.eur_mwh.is_empty()
    }

    /// Number of local days.
    pub fn days(&self) -> usize {
        self.day_starts.len() - 1
    }

    /// The series index that UTC reference hour `utc_hour` reads (hour 0 is
    /// 00:00 UTC on January 1st): `(utc_hour + offset) mod len`. For the
    /// local year the last UTC hour wraps to the first local hour, which is
    /// where the replay's own wrap takes it next anyway.
    pub fn index_at_utc_hour(&self, utc_hour: usize) -> usize {
        let len = self.len() as i64;
        (utc_hour as i64 + i64::from(self.start_offset_h)).rem_euclid(len) as usize
    }

    /// Price of hour `idx`, EUR/MWh.
    pub fn price_eur_mwh(&self, idx: usize) -> f64 {
        self.eur_mwh[idx]
    }

    /// All prices, EUR/MWh, in series order.
    pub fn prices_eur_mwh(&self) -> &[f64] {
        &self.eur_mwh
    }

    /// The local day hour `idx` belongs to.
    pub fn day_of(&self, idx: usize) -> usize {
        debug_assert!(idx < self.len(), "hour {idx} outside the year");
        self.day_starts.partition_point(|&s| usize::from(s) <= idx) - 1
    }

    /// The hour indices of local day `d`: 24 entries on most days, 23 and
    /// 25 on the DST days.
    pub fn day_hours(&self, d: usize) -> Range<usize> {
        usize::from(self.day_starts[d])..usize::from(self.day_starts[d + 1])
    }
}

/// The DE-LU 2024 day-ahead price year bundled with this release.
///
/// # Panics
/// Panics when the bundled bytes fail to decode or do not hash to
/// [`DAYAHEAD_DE_LU_2024_FNV1A`]; either means artifact and pin were not
/// moved together and the build is not trustworthy.
pub fn dayahead_de_lu_2024() -> &'static PriceYear {
    static YEAR: OnceLock<PriceYear> = OnceLock::new();
    YEAR.get_or_init(|| {
        static RAW: &[u8] = include_bytes!("../data/dayahead-de-lu-2024.bin");
        assert_eq!(
            fnv1a_64(RAW),
            DAYAHEAD_DE_LU_2024_FNV1A,
            "bundled price artifact does not match the pinned hash; \
             regenerate with compile-prices and move DAYAHEAD_DE_LU_2024_FNV1A \
             in the same commit"
        );
        let data = codec::decode(RAW).expect("bundled price artifact must decode");
        let eur_mwh = data
            .price_ct_mwh
            .iter()
            .map(|&ct| f64::from(ct) / 100.0)
            .collect();
        PriceYear::from_parts(data.year, data.start_offset_h, eur_mwh, data.day_starts)
            .expect("bundled day table must describe the series")
    })
}

#[cfg(test)]
mod tests {
    use super::PriceYear;

    /// Two local days, the first a 23-hour spring day.
    fn spring_pair() -> PriceYear {
        let prices = (0..47).map(f64::from).collect();
        PriceYear::from_parts(2024, 1, prices, vec![0, 23, 47]).expect("valid year")
    }

    #[test]
    fn day_of_is_the_inverse_of_day_hours() {
        let year = spring_pair();
        for d in 0..year.days() {
            for idx in year.day_hours(d) {
                assert_eq!(year.day_of(idx), d, "hour {idx}");
            }
        }
        assert_eq!(year.day_hours(0).len(), 23);
        assert_eq!(year.day_hours(1).len(), 24);
    }

    #[test]
    fn a_table_that_misses_the_series_is_refused() {
        assert!(PriceYear::from_parts(2024, 1, vec![0.0; 47], vec![0, 23, 48]).is_err());
        assert!(PriceYear::from_parts(2024, 1, vec![0.0; 47], vec![]).is_err());
    }
}
