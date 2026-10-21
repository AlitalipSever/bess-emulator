//! A replayed grid-frequency year (M4 phase 1).
//!
//! One-second samples of the Continental European frequency as a
//! millihertz deviation from 50 Hz, compiled by `compile-frequency` from
//! the Energy-Charts API responses `scripts/fetch-frequency.sh` downloads.
//! Not bundled: the source's terms (free for non-commercial and
//! journalistic use with attribution to "Energy-Charts / Fraunhofer ISE")
//! are not compatible with this repository's licences, so the artifact is
//! the user's, built under their own acceptance and loaded by path
//! (`--frequency`). A run that replays one names the file's FNV-1a hash in
//! its record, so whoever holds the same file reproduces the run.
//!
//! Gaps in the source are filled linearly up to a bound the compiler
//! states and refuses beyond; the count of filled seconds rides in the
//! header so a loaded year can say how much of it is interpolation.

pub mod codec;

use std::path::Path;

use crate::fnv1a_64;

/// One year of one-second frequency samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyYear {
    year: u16,
    start_unix_s: i64,
    filled_s: u32,
    mhz: Vec<i16>,
    hash: u64,
}

impl FrequencyYear {
    /// Builds a year from its parts (the loader and the tests).
    ///
    /// # Errors
    /// Returns an error when the series is empty.
    #[doc(hidden)]
    pub fn from_parts(
        year: u16,
        start_unix_s: i64,
        filled_s: u32,
        mhz: Vec<i16>,
    ) -> Result<Self, String> {
        if mhz.is_empty() {
            return Err("a frequency year with no samples".into());
        }
        let hash = fnv1a_64(&codec::encode_samples(&mhz));
        Ok(Self {
            year,
            start_unix_s,
            filled_s,
            mhz,
            hash,
        })
    }

    /// Loads a compiled artifact from disk.
    ///
    /// # Errors
    /// Returns an error when the file cannot be read or does not decode.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let data = codec::decode(&bytes)?;
        Self::from_parts(data.year, data.start_unix_s, data.filled_s, data.mhz)
    }

    /// Calendar year the samples cover.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Unix seconds UTC of the first sample.
    pub fn start_unix_s(&self) -> i64 {
        self.start_unix_s
    }

    /// Number of one-second samples.
    pub fn len(&self) -> usize {
        self.mhz.len()
    }

    /// True when the year holds no samples (never for a loaded artifact).
    pub fn is_empty(&self) -> bool {
        self.mhz.is_empty()
    }

    /// Seconds the compiler filled by interpolation.
    pub fn filled_s(&self) -> u32 {
        self.filled_s
    }

    /// FNV-1a 64 over the samples, what a record names.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Frequency `offset_s` seconds into the year, Hz, wrapping at the
    /// end so a replayed year is a loop like the weather's.
    pub fn frequency_hz_at(&self, offset_s: i64) -> f64 {
        let idx = offset_s.rem_euclid(self.mhz.len() as i64) as usize;
        50.0 + f64::from(self.mhz[idx]) / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::FrequencyYear;

    #[test]
    fn samples_read_back_and_wrap() {
        let y = FrequencyYear::from_parts(2024, 1_704_067_200, 3, vec![-15, 0, 22]).expect("year");
        assert_eq!(y.frequency_hz_at(0), 49.985);
        assert_eq!(y.frequency_hz_at(2), 50.022);
        assert_eq!(y.frequency_hz_at(3), 49.985);
        assert_eq!(y.frequency_hz_at(-1), 50.022);
        assert_eq!(y.len(), 3);
        assert_eq!(y.filled_s(), 3);
        assert!(FrequencyYear::from_parts(2024, 0, 0, vec![]).is_err());
    }
}
