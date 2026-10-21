//! A replayed year of the TSO's aFRR request (M4 phase 3 PR2): one
//! sample per second, the request as a share of the demand, in per
//! mille, positive to deliver.
//!
//! The source is the control area's per-second aFRR setpoint
//! (Netztransparenz, "Sekündlicher SRL-Sollwert"), the TSOs' copyright
//! behind a registered API: nothing of it is bundled. `compile-afrr`
//! turns a `timestamp;mw` CSV the user obtained into this artifact,
//! normalized by the demand the user states, and `--afrr` replays it.
//! A plant holding an award of `P` then follows `request × P`, the
//! pro-rata share of the control area's request, labeled in
//! CALIBRATION.md.

pub mod codec;

use std::path::Path;

use crate::fnv1a_64;

/// One year of one-second request samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfrrRequestYear {
    year: u16,
    start_unix_s: i64,
    demand_mw: u32,
    permille: Vec<i16>,
    hash: u64,
}

impl AfrrRequestYear {
    /// Builds a year from its parts.
    ///
    /// # Errors
    /// Returns an error when the series is empty.
    #[doc(hidden)]
    pub fn from_parts(
        year: u16,
        start_unix_s: i64,
        demand_mw: u32,
        permille: Vec<i16>,
    ) -> Result<Self, String> {
        if permille.is_empty() {
            return Err("an aFRR request year with no samples".into());
        }
        let hash = fnv1a_64(&codec::encode_samples(&permille));
        Ok(Self {
            year,
            start_unix_s,
            demand_mw,
            permille,
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
        Self::from_parts(data.year, data.start_unix_s, data.demand_mw, data.permille)
    }

    /// Calendar year the samples cover.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Unix seconds UTC of the first sample.
    pub fn start_unix_s(&self) -> i64 {
        self.start_unix_s
    }

    /// The demand the samples are normalized by, MW.
    pub fn demand_mw(&self) -> u32 {
        self.demand_mw
    }

    /// FNV-1a 64 over the samples.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Number of samples.
    pub fn len(&self) -> usize {
        self.permille.len()
    }

    /// True when the year holds no samples (never for a loaded artifact).
    pub fn is_empty(&self) -> bool {
        self.permille.is_empty()
    }

    /// The request `offset_s` seconds into the year, −1 to 1, wrapping.
    pub fn request_pu_at(&self, offset_s: i64) -> f64 {
        let idx = offset_s.rem_euclid(self.permille.len() as i64) as usize;
        (f64::from(self.permille[idx]) / 1000.0).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::AfrrRequestYear;

    #[test]
    fn samples_read_back_clamped_and_wrapping() {
        let y = AfrrRequestYear::from_parts(2024, 0, 2000, vec![-500, 0, 1200]).expect("year");
        assert_eq!(y.request_pu_at(0), -0.5);
        assert_eq!(y.request_pu_at(2), 1.0);
        assert_eq!(y.request_pu_at(3), -0.5);
        assert_eq!(y.demand_mw(), 2000);
        assert!(AfrrRequestYear::from_parts(2024, 0, 1, vec![]).is_err());
    }
}
