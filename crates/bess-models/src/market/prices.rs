//! The 2024 German balancing prices as annual figures (M4 phase 3).
//!
//! Derived values, cited in CALIBRATION.md: the means over the 366 daily
//! result files regelleistung.net's data centre served on 2026-10-21
//! (fetched by `scripts/fetch-balancing.sh`, read by
//! `compile-balancing`), one figure per four-hour block and product. The
//! files themselves are not bundled (DATA-LICENSES.md); these numbers are
//! what the emulator prices with when no replayed year is attached, and
//! the annual record is made with them.

/// Hours in a four-hour block.
pub const BLOCK_H: f64 = 4.0;

/// FCR settlement capacity price per block, EUR per MW and block
/// (blocks 00-04, 04-08, 08-12, 12-16, 16-20, 20-24), means over 2024.
/// Holding every block of every day paid 142,909 EUR/MW over the year.
pub const FCR_EUR_MW_BLOCK_2024: [f64; 6] = [40.22, 50.97, 78.04, 103.54, 71.71, 45.93];

/// aFRR positive capacity, the volume-weighted average accepted price per
/// block, EUR per MW and hour, means over 2024.
pub const AFRR_POS_EUR_MW_H_2024: [f64; 6] = [4.87, 9.64, 16.98, 10.14, 21.29, 15.92];

/// aFRR negative capacity, likewise.
pub const AFRR_NEG_EUR_MW_H_2024: [f64; 6] = [7.39, 6.33, 13.52, 26.60, 8.71, 2.52];

/// The aFRR energy merit order's cheapest bid, EUR/MWh, mean over the
/// 35,136 quarter-hours of 2024: positive direction (the plant delivers
/// and is paid), negative direction (the plant absorbs; a positive price
/// is what the plant is paid to take energy).
pub const AFRR_ENERGY_MIN_POS_EUR_MWH_2024: f64 = 102.49;
/// See `AFRR_ENERGY_MIN_POS_EUR_MWH_2024`.
pub const AFRR_ENERGY_MIN_NEG_EUR_MWH_2024: f64 = 66.15;

/// The merit order's volume-weighted average bid, EUR/MWh, mean over
/// 2024: 1,328 positive and −1,142 negative (a negative price in the
/// negative direction is the plant paying to absorb). Far from the
/// cheapest bid: the merit order's tail is priced at the cap, and the
/// average over the whole list is not what an activated unit sees.
pub const AFRR_ENERGY_AVG_POS_EUR_MWH_2024: f64 = 1328.0;
/// See `AFRR_ENERGY_AVG_POS_EUR_MWH_2024`.
pub const AFRR_ENERGY_AVG_NEG_EUR_MWH_2024: f64 = -1142.46;

/// Capacity offered to the aFRR energy merit order, MW, mean over 2024,
/// each direction: the German demand of about 2 GW the TSOs' 2026
/// requirements name.
pub const AFRR_OFFERED_MW_2024: (f64, f64) = (2165.3, 2133.2);

#[cfg(test)]
mod tests {
    use super::*;

    /// The annual sums the figures imply, held so a retyped number shows.
    #[test]
    fn the_annual_sums_are_what_the_files_said() {
        let fcr_year: f64 = FCR_EUR_MW_BLOCK_2024.iter().sum::<f64>() * 366.0;
        assert!((fcr_year - 142_909.0).abs() < 60.0, "{fcr_year}");
        let pos: f64 = AFRR_POS_EUR_MW_H_2024.iter().sum::<f64>() * BLOCK_H * 366.0;
        let neg: f64 = AFRR_NEG_EUR_MW_H_2024.iter().sum::<f64>() * BLOCK_H * 366.0;
        assert!((115_000.0..=116_000.0).contains(&pos), "{pos}");
        assert!((95_000.0..=96_000.0).contains(&neg), "{neg}");
    }
}
