//! Deterministic synthetic input driver.
//!
//! Placeholder until bess-data ships compiled real series (weather,
//! frequency): a daily temperature sinusoid, a daylight irradiance arc, and
//! a gently wandering grid frequency. Purely a function of the timestamp,
//! so it preserves the determinism contract.

use std::f64::consts::{PI, TAU};

use bess_core::kernel::Inputs;

/// Synthetic diurnal weather and grid-frequency driver.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticWeather {
    /// Daily mean temperature, degrees Celsius.
    pub mean_c: f64,
    /// Half of the daily temperature swing, K.
    pub amplitude_c: f64,
}

impl Default for SyntheticWeather {
    fn default() -> Self {
        Self {
            mean_c: 14.0,
            amplitude_c: 8.0,
        }
    }
}

impl SyntheticWeather {
    /// Inputs for a given UTC timestamp. Temperature peaks at 15:00,
    /// irradiance follows a sine arc between 06:00 and 18:00, frequency
    /// wanders +/- 10 mHz around 50 Hz on a 10 minute period.
    pub fn inputs_at(&self, unix_time_s: i64) -> Inputs {
        let second_of_day = unix_time_s.rem_euclid(86_400) as f64;
        let hour = second_of_day / 3600.0;
        let ambient_c = self.mean_c + self.amplitude_c * (TAU * (hour - 9.0) / 24.0).sin();
        let irradiance_wm2 = if (6.0..18.0).contains(&hour) {
            900.0 * (PI * (hour - 6.0) / 12.0).sin()
        } else {
            0.0
        };
        let grid_frequency_hz = 50.0 + 0.01 * (TAU * second_of_day / 600.0).sin();
        Inputs {
            ambient_c,
            irradiance_wm2,
            grid_frequency_hz,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_peaks_mid_afternoon() {
        let w = SyntheticWeather::default();
        let at_15 = w.inputs_at(15 * 3600).ambient_c;
        let at_03 = w.inputs_at(3 * 3600).ambient_c;
        assert!(at_15 > at_03);
        assert!((at_15 - (w.mean_c + w.amplitude_c)).abs() < 1.0e-9);
    }

    #[test]
    fn irradiance_is_zero_at_night() {
        let w = SyntheticWeather::default();
        assert!(w.inputs_at(2 * 3600).irradiance_wm2.abs() < f64::EPSILON);
        assert!(w.inputs_at(12 * 3600).irradiance_wm2 > 800.0);
    }

    #[test]
    fn frequency_stays_near_nominal() {
        let w = SyntheticWeather::default();
        for s in (0..86_400).step_by(97) {
            let f = w.inputs_at(s).grid_frequency_hz;
            assert!((49.98..50.02).contains(&f));
        }
    }
}
