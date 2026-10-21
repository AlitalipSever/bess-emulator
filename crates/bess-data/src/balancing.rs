//! A replayed year of German balancing tender results (M4 phase 3).
//!
//! Per local day, the FCR settlement capacity price of each of the six
//! four-hour blocks (EUR per MW and block) and the volume-weighted
//! average aFRR capacity price of each block and direction (EUR per MW
//! and hour, the German auction's pay-as-bid average); per quarter-hour,
//! the aFRR energy merit order's minimum and average price in each
//! direction (EUR/MWh). Compiled by `compile-balancing` from the CSVs
//! `scripts/xlsx-to-csv.py` writes beside the files
//! `scripts/fetch-balancing.sh` downloads from regelleistung.net.
//!
//! Not bundled: the platform publishes the results without a licence
//! statement, so the artifact is the user's, loaded by path
//! (`--balancing`); the emulator bundles only the annual figures
//! CALIBRATION.md cites (`bess-models`'s `market::prices`). A run that
//! replays one names the file's hash in its record.

pub mod codec;

use std::ops::Range;
use std::path::Path;

use crate::fnv1a_64;

/// Four-hour blocks in a day.
pub const BLOCKS: usize = 6;

/// One replayed balancing year.
#[derive(Debug, Clone, PartialEq)]
pub struct BalancingYear {
    year: u16,
    start_offset_h: i16,
    fcr_eur_mw_block: Vec<f64>,
    afrr_pos_eur_mw_h: Vec<f64>,
    afrr_neg_eur_mw_h: Vec<f64>,
    energy_pos_min: Vec<f64>,
    energy_pos_avg: Vec<f64>,
    energy_neg_min: Vec<f64>,
    energy_neg_avg: Vec<f64>,
    quarter_starts: Vec<u32>,
    hash: u64,
}

impl BalancingYear {
    /// Builds a year from its decoded parts (the loader and the tests).
    ///
    /// # Errors
    /// Returns an error when the day table does not describe the series.
    #[doc(hidden)]
    pub fn from_data(data: codec::ArtifactData, hash: u64) -> Result<Self, String> {
        let days = data.quarter_starts.len().saturating_sub(1);
        if days == 0 {
            return Err("a balancing year with no days".into());
        }
        let per_day = |name: &str, v: &Vec<i32>, n: usize| {
            if v.len() == days * n {
                Ok(())
            } else {
                Err(format!("{name}: {} values for {days} days", v.len()))
            }
        };
        per_day("fcr", &data.fcr_ct, BLOCKS)?;
        per_day("afrr_pos", &data.afrr_pos_ct, BLOCKS)?;
        per_day("afrr_neg", &data.afrr_neg_ct, BLOCKS)?;
        let quarters = *data.quarter_starts.last().expect("a table") as usize;
        for (name, v) in [
            ("energy_pos_min", &data.energy_pos_min_ct),
            ("energy_pos_avg", &data.energy_pos_avg_ct),
            ("energy_neg_min", &data.energy_neg_min_ct),
            ("energy_neg_avg", &data.energy_neg_avg_ct),
        ] {
            if v.len() != quarters {
                return Err(format!(
                    "{name}: {} values for {quarters} quarters",
                    v.len()
                ));
            }
        }
        let eur = |v: &[i32]| v.iter().map(|&c| f64::from(c) / 100.0).collect();
        Ok(Self {
            year: data.year,
            start_offset_h: data.start_offset_h,
            fcr_eur_mw_block: eur(&data.fcr_ct),
            afrr_pos_eur_mw_h: eur(&data.afrr_pos_ct),
            afrr_neg_eur_mw_h: eur(&data.afrr_neg_ct),
            energy_pos_min: eur(&data.energy_pos_min_ct),
            energy_pos_avg: eur(&data.energy_pos_avg_ct),
            energy_neg_min: eur(&data.energy_neg_min_ct),
            energy_neg_avg: eur(&data.energy_neg_avg_ct),
            quarter_starts: data.quarter_starts,
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
        Self::from_data(data, fnv1a_64(&bytes))
    }

    /// Calendar year of the local days.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Hours the local year starts after 00:00 UTC on January 1st.
    pub fn start_offset_h(&self) -> i16 {
        self.start_offset_h
    }

    /// FNV-1a 64 of the artifact's bytes, what a record names.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Number of local days.
    pub fn days(&self) -> usize {
        self.quarter_starts.len() - 1
    }

    /// The quarter-hour indices of local day `d`.
    pub fn day_quarters(&self, d: usize) -> Range<usize> {
        self.quarter_starts[d] as usize..self.quarter_starts[d + 1] as usize
    }

    /// The quarter-hour index UTC reference quarter `utc_quarter` reads
    /// (quarter 0 is 00:00 UTC on January 1st), wrapping at the year's
    /// end like the price year.
    pub fn index_at_utc_quarter(&self, utc_quarter: usize) -> usize {
        let len = i64::from(*self.quarter_starts.last().expect("a table"));
        (utc_quarter as i64 + i64::from(self.start_offset_h) * 4).rem_euclid(len) as usize
    }

    /// The local day quarter `idx` belongs to.
    pub fn day_of(&self, idx: usize) -> usize {
        self.quarter_starts.partition_point(|&s| s as usize <= idx) - 1
    }

    /// FCR settlement capacity price of block `block` of local day `d`,
    /// EUR per MW and block.
    pub fn fcr_eur_mw_block(&self, d: usize, block: usize) -> f64 {
        self.fcr_eur_mw_block[d * BLOCKS + block]
    }

    /// aFRR average capacity price of block `block` of day `d`, EUR per
    /// MW and hour, positive (`pos`) or negative direction.
    pub fn afrr_eur_mw_h(&self, d: usize, block: usize, pos: bool) -> f64 {
        if pos {
            self.afrr_pos_eur_mw_h[d * BLOCKS + block]
        } else {
            self.afrr_neg_eur_mw_h[d * BLOCKS + block]
        }
    }

    /// The aFRR energy merit order's minimum and average price at
    /// quarter `idx`, EUR/MWh, positive or negative direction.
    pub fn energy_eur_mwh(&self, idx: usize, pos: bool) -> (f64, f64) {
        if pos {
            (self.energy_pos_min[idx], self.energy_pos_avg[idx])
        } else {
            (self.energy_neg_min[idx], self.energy_neg_avg[idx])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::codec::ArtifactData;
    use super::*;

    fn two_days() -> BalancingYear {
        let data = ArtifactData {
            year: 2024,
            start_offset_h: 1,
            fcr_ct: (0..12).map(|i| i * 100).collect(),
            afrr_pos_ct: vec![100; 12],
            afrr_neg_ct: vec![200; 12],
            energy_pos_min_ct: vec![1000; 192],
            energy_pos_avg_ct: vec![2000; 192],
            energy_neg_min_ct: vec![-1000; 192],
            energy_neg_avg_ct: vec![-2000; 192],
            quarter_starts: vec![0, 96, 192],
        };
        BalancingYear::from_data(data, 7).expect("year")
    }

    #[test]
    fn the_tables_read_back_by_day_block_and_quarter() {
        let y = two_days();
        assert_eq!(y.days(), 2);
        assert_eq!(y.fcr_eur_mw_block(1, 2), 8.0);
        assert_eq!(y.afrr_eur_mw_h(0, 5, true), 1.0);
        assert_eq!(y.afrr_eur_mw_h(0, 5, false), 2.0);
        assert_eq!(y.energy_eur_mwh(100, false), (-10.0, -20.0));
        assert_eq!(y.day_of(100), 1);
        assert_eq!(y.day_quarters(1), 96..192);
        // One hour after the UTC year starts is local quarter 8 of day 0.
        assert_eq!(y.index_at_utc_quarter(4), 8);
        assert_eq!(y.index_at_utc_quarter(190), 2);
    }

    #[test]
    fn a_mismatched_table_is_refused() {
        let mut data = ArtifactData {
            year: 2024,
            start_offset_h: 1,
            fcr_ct: vec![0; 12],
            afrr_pos_ct: vec![0; 12],
            afrr_neg_ct: vec![0; 12],
            energy_pos_min_ct: vec![0; 192],
            energy_pos_avg_ct: vec![0; 192],
            energy_neg_min_ct: vec![0; 192],
            energy_neg_avg_ct: vec![0; 192],
            quarter_starts: vec![0, 96, 192],
        };
        data.fcr_ct.pop();
        assert!(BalancingYear::from_data(data, 0).is_err());
    }
}
