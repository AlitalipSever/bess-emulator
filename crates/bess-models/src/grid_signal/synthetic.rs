//! A synthetic grid frequency with the Continental European statistics.
//!
//! No open-licence one-second series exists for the reference year
//! (DATA-LICENSES.md, the M4 rows), so the bundled driver is a shape, not
//! a dataset. Two things are known publicly about the Continental
//! European frequency (Rydin Gorjão et al., Nature Communications 11,
//! 6362, 2020, CC BY 4.0): its distribution is close to Gaussian with an
//! approximately exponential autocorrelation decay, and it carries a
//! deterministic pattern at the dispatch boundaries, strongest at the
//! hour, visible at the quarter-hours, where the scheduled generation
//! steps and the load does not. The driver reproduces both:
//!
//! - **The stochastic part** is value noise: uniform knots drawn by
//!   hashing the knot index with the seed, interpolated linearly, on two
//!   octaves (10 s and 120 s knots) so the series has structure at the
//!   seconds a droop controller cares about and at the minutes a
//!   reservoir cares about. Linear interpolation of independent uniform
//!   knots has a variance of 2/9 per unit amplitude, which is how the
//!   amplitude is set from the target standard deviation.
//! - **The deterministic part** is a sawtooth at each hour: the
//!   frequency drifts away in the last five minutes before the boundary
//!   and jumps the other way on it, decaying back over the next five.
//!   Its sign follows the schedule: upward generation ramps in the
//!   morning hours, downward in the evening, nothing in between. The
//!   quarter-hours carry a third of the hour's amplitude.
//!
//! The standard deviation and the boundary amplitudes are labeled
//! estimates in CALIBRATION.md; the paper gives the shape and the
//! pattern, not a figure for 2024.

use bess_data::fnv1a_64;

/// The parameters of the synthetic frequency.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticFrequency {
    /// Nominal frequency, Hz.
    pub nominal_hz: f64,
    /// Standard deviation of the stochastic part, mHz.
    pub std_mhz: f64,
    /// Amplitude of the jump at an hour boundary, mHz.
    pub hour_step_mhz: f64,
    /// Amplitude of the jump at a quarter-hour boundary, mHz.
    pub quarter_step_mhz: f64,
    /// Seconds the boundary deviation takes to decay, and the window
    /// before the boundary over which the opposite drift builds.
    pub boundary_window_s: f64,
    /// Seed of the knot hash; the kernel seed is not reused on purpose,
    /// so a scenario's seed does not move the grid.
    pub seed: u64,
}

/// Knot spacings of the two octaves, s.
const OCTAVES_S: [i64; 2] = [10, 120];

impl SyntheticFrequency {
    /// The Continental European estimate: 20 mHz standard deviation,
    /// 30 mHz hour steps, 10 mHz quarter-hour steps, five-minute
    /// windows. Labeled estimates; see CALIBRATION.md.
    pub fn ce_estimate() -> Self {
        Self {
            nominal_hz: 50.0,
            std_mhz: 20.0,
            hour_step_mhz: 30.0,
            quarter_step_mhz: 10.0,
            boundary_window_s: 300.0,
            seed: 0x4345_2d46_5245_5131, // "CE-FREQ1"
        }
    }

    /// The driver with every deviation set to zero: a flat 50.000 Hz.
    /// The switch-off proof and tests that want a quiet grid.
    pub fn flat() -> Self {
        Self {
            std_mhz: 0.0,
            hour_step_mhz: 0.0,
            quarter_step_mhz: 0.0,
            ..Self::ce_estimate()
        }
    }

    /// Frequency at a UTC timestamp, Hz.
    pub fn frequency_hz(&self, unix_time_s: i64) -> f64 {
        self.nominal_hz + (self.noise_mhz(unix_time_s) + self.boundary_mhz(unix_time_s)) / 1000.0
    }

    /// The stochastic part, mHz: two octaves of value noise scaled to the
    /// target standard deviation (each octave carries half the variance).
    fn noise_mhz(&self, unix_time_s: i64) -> f64 {
        if self.std_mhz <= 0.0 {
            return 0.0;
        }
        // var(octave) = 2/9 * a^2; two octaves, each half the variance.
        let amplitude = self.std_mhz * (9.0_f64 / 4.0).sqrt();
        OCTAVES_S
            .iter()
            .enumerate()
            .map(|(octave, &spacing)| amplitude * self.value_noise(unix_time_s, spacing, octave))
            .sum()
    }

    /// Value noise in [-1, 1): uniform knots every `spacing_s`, linear
    /// between them.
    fn value_noise(&self, unix_time_s: i64, spacing_s: i64, octave: usize) -> f64 {
        let k = unix_time_s.div_euclid(spacing_s);
        let frac = unix_time_s.rem_euclid(spacing_s) as f64 / spacing_s as f64;
        let a = self.knot(k, octave);
        let b = self.knot(k + 1, octave);
        a + (b - a) * frac
    }

    /// A uniform draw in [-1, 1) for one knot, from the seed, the octave
    /// and the knot index.
    fn knot(&self, index: i64, octave: usize) -> f64 {
        let mut bytes = [0u8; 17];
        bytes[..8].copy_from_slice(&self.seed.to_le_bytes());
        bytes[8..16].copy_from_slice(&index.to_le_bytes());
        bytes[16] = octave as u8;
        let h = fnv1a_64(&bytes);
        // 53 bits of mantissa, exact in f64.
        (h >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }

    /// The deterministic part, mHz: the sawtooth at the hour and the
    /// quarter-hours.
    fn boundary_mhz(&self, unix_time_s: i64) -> f64 {
        let second_of_day = unix_time_s.rem_euclid(86_400);
        let hour = second_of_day / 3600;
        let sign = schedule_sign(hour);
        if sign == 0.0 {
            return 0.0;
        }
        let w = self.boundary_window_s;
        // Distance to the nearest quarter-hour boundary, signed: negative
        // before it, positive after.
        let since_quarter = (second_of_day % 900) as f64;
        let (tau, at_hour) = if since_quarter <= 450.0 {
            (since_quarter, second_of_day % 3600 < 900)
        } else {
            (since_quarter - 900.0, (second_of_day + 900) % 3600 < 900)
        };
        let amplitude = if at_hour {
            self.hour_step_mhz
        } else {
            self.quarter_step_mhz
        };
        if tau >= 0.0 && tau < w {
            sign * amplitude * (1.0 - tau / w)
        } else if tau < 0.0 && -tau < w {
            -0.5 * sign * amplitude * (1.0 + tau / w)
        } else {
            0.0
        }
    }
}

/// The schedule's direction at an hour of the day (UTC): generation
/// ramps up through the morning (a step up at the boundary, the
/// frequency jumps high), down through the evening, and sits flat
/// otherwise. A stylized pattern, labeled.
fn schedule_sign(hour: i64) -> f64 {
    match hour {
        5..=11 => 1.0,
        16..=22 => -1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-07-14 00:00 UTC.
    const DAY_S: i64 = 1_767_225_600 + 194 * 86_400;

    fn stats(f: &SyntheticFrequency, from: i64, ticks: i64) -> (f64, f64) {
        let n = ticks as f64;
        let mean = (0..ticks)
            .map(|t| f.frequency_hz(from + t) - 50.0)
            .sum::<f64>()
            / n;
        let var = (0..ticks)
            .map(|t| {
                let d = f.frequency_hz(from + t) - 50.0 - mean;
                d * d
            })
            .sum::<f64>()
            / n;
        (mean * 1000.0, var.sqrt() * 1000.0)
    }

    /// Over a day the mean sits at 50.000 Hz within a few millihertz
    /// (720 slow knots average to that), over a week within one, and the
    /// standard deviation within a quarter of the target (the boundary
    /// pattern adds a little).
    #[test]
    fn a_day_has_the_stated_mean_and_spread() {
        let f = SyntheticFrequency::ce_estimate();
        let (mean_mhz, std_mhz) = stats(&f, DAY_S, 86_400);
        assert!(mean_mhz.abs() < 3.0, "mean {mean_mhz} mHz");
        assert!((15.0..=25.0).contains(&std_mhz), "std {std_mhz} mHz");
        let (week_mean_mhz, _) = stats(&f, DAY_S, 7 * 86_400);
        assert!(week_mean_mhz.abs() < 1.5, "week mean {week_mean_mhz} mHz");
        let flat = SyntheticFrequency::flat();
        assert_eq!(flat.frequency_hz(DAY_S + 12_345), 50.0);
    }

    /// The hour boundary is visible: the mean absolute deviation in the
    /// first minute of a morning hour exceeds the last minute's, and the
    /// first second after the boundary carries the step.
    #[test]
    fn the_hour_boundary_carries_a_step() {
        let f = SyntheticFrequency::ce_estimate();
        let hour = DAY_S + 8 * 3600;
        let first: f64 = (0..60)
            .map(|t| (f.frequency_hz(hour + t) - 50.0).abs())
            .sum::<f64>()
            / 60.0;
        let last: f64 = (1800..1860)
            .map(|t| (f.frequency_hz(hour + t) - 50.0).abs())
            .sum::<f64>()
            / 60.0;
        assert!(first > last, "first minute {first}, mid hour {last}");
        assert!(f.boundary_mhz(hour) > 29.9 && f.boundary_mhz(hour) <= 30.0);
        assert!(f.boundary_mhz(hour - 1) < -14.0);
        assert!(f.boundary_mhz(hour + 900).abs() < 10.1);
        assert_eq!(f.boundary_mhz(DAY_S + 13 * 3600), 0.0);
    }

    /// The same instant reads the same value whatever was asked before,
    /// and a different seed reads a different value.
    #[test]
    fn pure_and_seeded() {
        let f = SyntheticFrequency::ce_estimate();
        let a = f.frequency_hz(DAY_S + 777);
        let _ = f.frequency_hz(DAY_S + 99_999);
        assert_eq!(a, f.frequency_hz(DAY_S + 777));
        let g = SyntheticFrequency {
            seed: 1,
            ..SyntheticFrequency::ce_estimate()
        };
        assert_ne!(a, g.frequency_hz(DAY_S + 777));
        assert_eq!(synthetic_grid_frequency_hz(DAY_S + 777), a);
    }

    use super::super::synthetic_grid_frequency_hz;
}
