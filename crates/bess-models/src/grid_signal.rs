//! The grid as an input with a shape (M4 phase 1).
//!
//! Frequency and voltage at the point of interconnection are exogenous:
//! the plant answers them, it does not move them. Until M4 both were
//! sinusoids. This module holds the bundled frequency driver, a synthetic
//! series calibrated to the published statistics of the Continental
//! European grid, and the voltage driver M3 introduced. A measured year
//! replays through `bess_data::FrequencyYear` instead; see
//! `weather::HistoricalWeather::with_frequency`.
//!
//! Everything here is a pure function of the timestamp and a seed: no
//! state, so a driver can be asked for any instant in any order, and
//! rational arithmetic only (a hash, linear interpolation, a square root
//! at construction), so the kernel's inputs are byte-identical across
//! platforms.

pub mod afrr;
pub mod synthetic;

pub use afrr::SyntheticAfrr;
pub use synthetic::SyntheticFrequency;

/// Grid frequency at a UTC timestamp, Hz, from the default synthetic
/// driver ([`SyntheticFrequency::ce_estimate`]). Shared by both weather
/// drivers; kept under its M0 name so callers do not notice the move.
pub fn synthetic_grid_frequency_hz(unix_time_s: i64) -> f64 {
    SyntheticFrequency::ce_estimate().frequency_hz(unix_time_s)
}

/// The TSO's aFRR request at a UTC timestamp, −1 to 1, from the default
/// synthetic driver ([`SyntheticAfrr::estimate`]).
pub fn synthetic_afrr_request_pu(unix_time_s: i64) -> f64 {
    SyntheticAfrr::estimate().request_pu(unix_time_s)
}

/// Grid voltage at a UTC timestamp, per unit: a +/- 2 % daily swing,
/// highest at 04:00 and lowest at 16:00, as a triangle so the kernel
/// stays rational (M3 phase 4). Synthetic until a measured series is
/// replayed.
pub fn synthetic_grid_voltage_pu(unix_time_s: i64) -> f64 {
    let since_peak_s = (unix_time_s - 4 * 3600).rem_euclid(86_400) as f64;
    let phase = since_peak_s / 86_400.0;
    1.0 + 0.02 * (4.0 * (phase - 0.5).abs() - 1.0)
}
