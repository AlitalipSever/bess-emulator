//! The TSO's aFRR request as the plant sees it (M4 phase 3 PR2): a
//! fraction of the awarded power, positive to deliver, negative to
//! absorb.
//!
//! The real signal is the control area's per-second setpoint, which
//! Netztransparenz publishes under the TSOs' copyright behind a
//! registered API, so nothing of it is bundled: `AfrrRequestYear`
//! replays a file the user compiled (`compile-afrr`), and the bundled
//! driver is synthetic. The synthetic request has two parts, both pure
//! functions of the timestamp: a fast part that follows the frequency
//! the way the frequency restoration controller's proportional term
//! does (the frequency averaged over the last minute, scaled so a
//! 100 mHz low average asks for the full positive award), and a slow
//! part of value noise on 15-minute knots standing in for the control
//! area's imbalance the frequency does not show. Labeled estimates; see
//! CALIBRATION.md.

use bess_data::fnv1a_64;

use super::SyntheticFrequency;

/// The synthetic request's parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticAfrr {
    /// The frequency the fast part reads.
    pub frequency: SyntheticFrequency,
    /// Seconds of frequency averaged for the fast part.
    pub window_s: i64,
    /// The average deviation, Hz, at which the fast part asks for the
    /// full award.
    pub full_hz: f64,
    /// Amplitude of the slow part, as a share of the award.
    pub slow_amplitude: f64,
    /// Knot spacing of the slow part, s.
    pub slow_knot_s: i64,
    /// Seed of the slow part's knots.
    pub seed: u64,
}

impl SyntheticAfrr {
    /// The estimate: a one-minute window, full activation at a 100 mHz
    /// average, a slow part of ±40 % on 15-minute knots.
    pub fn estimate() -> Self {
        Self {
            frequency: SyntheticFrequency::ce_estimate(),
            window_s: 60,
            full_hz: 0.100,
            slow_amplitude: 0.4,
            slow_knot_s: 900,
            seed: 0x6146_5252_2d52_4551, // "aFRR-REQ"
        }
    }

    /// The request at a UTC timestamp, −1 to 1.
    pub fn request_pu(&self, unix_time_s: i64) -> f64 {
        let samples = (self.window_s / 10).max(1);
        let mean_df: f64 = (0..samples)
            .map(|k| self.frequency.nominal_hz - self.frequency.frequency_hz(unix_time_s - 10 * k))
            .sum::<f64>()
            / samples as f64;
        let fast = (mean_df / self.full_hz).clamp(-1.0, 1.0);
        let slow = self.slow_amplitude * self.slow_noise(unix_time_s);
        (fast + slow).clamp(-1.0, 1.0)
    }

    /// Value noise in [-1, 1) on the slow knots.
    fn slow_noise(&self, unix_time_s: i64) -> f64 {
        let k = unix_time_s.div_euclid(self.slow_knot_s);
        let frac = unix_time_s.rem_euclid(self.slow_knot_s) as f64 / self.slow_knot_s as f64;
        let knot = |index: i64| {
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&self.seed.to_le_bytes());
            bytes[8..].copy_from_slice(&index.to_le_bytes());
            (fnv1a_64(&bytes) >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        };
        let a = knot(k);
        a + (knot(k + 1) - a) * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_S: i64 = 1_767_225_600 + 194 * 86_400;

    /// Bounded, pure, a mean near zero over a day, and both directions
    /// asked for a fair share of the time.
    #[test]
    fn the_request_is_bounded_and_balanced() {
        let r = SyntheticAfrr::estimate();
        let (mut pos, mut neg, mut sum) = (0u64, 0u64, 0.0);
        for t in 0..86_400 {
            let v = r.request_pu(DAY_S + t);
            assert!((-1.0..=1.0).contains(&v));
            pos += u64::from(v > 0.1);
            neg += u64::from(v < -0.1);
            sum += v;
        }
        assert!(pos > 10_000 && neg > 10_000, "pos {pos} neg {neg}");
        assert!((sum / 86_400.0).abs() < 0.15, "mean {}", sum / 86_400.0);
        assert_eq!(r.request_pu(DAY_S + 777), r.request_pu(DAY_S + 777));
    }
}
