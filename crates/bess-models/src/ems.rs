//! Day-ahead dispatch plan over an hourly price curve.

use bess_core::state::SiteState;
use bess_core::traits::EmsStrategy;

/// M0 dispatch: rank the 24 hourly prices of one day, charge at full power
/// in the cheapest hours, discharge in the most expensive ones. The plan
/// repeats daily. Balancing-market activation replay and setpoint tracking
/// quality arrive in M4.
#[derive(Debug, Clone, PartialEq)]
pub struct DayAheadEms {
    power_w: f64,
    charge_hour: [bool; 24],
    discharge_hour: [bool; 24],
}

impl DayAheadEms {
    /// Build a plan from 24 hourly prices. Ties break toward the earlier
    /// hour so the plan is deterministic.
    pub fn from_hourly_prices(
        prices: &[f64; 24],
        power_w: f64,
        charge_hours: usize,
        discharge_hours: usize,
    ) -> Self {
        let mut by_price: [usize; 24] = core::array::from_fn(|i| i);
        by_price.sort_unstable_by(|&a, &b| {
            prices[a]
                .partial_cmp(&prices[b])
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        let mut charge_hour = [false; 24];
        let mut discharge_hour = [false; 24];
        for &h in by_price.iter().take(charge_hours.min(24)) {
            charge_hour[h] = true;
        }
        for &h in by_price.iter().rev().take(discharge_hours.min(24)) {
            discharge_hour[h] = true;
        }
        Self {
            power_w,
            charge_hour,
            discharge_hour,
        }
    }

    /// Plan over a placeholder daily price shape (night trough, morning
    /// ramp, midday solar dip, evening peak; EUR/MWh) until bess-data ships
    /// real day-ahead series. Charges three hours, discharges two: a 2 h
    /// plant needs the extra charge hour to cover conversion losses.
    pub fn default_profile(power_w: f64) -> Self {
        const PRICES: [f64; 24] = [
            72.0, 58.0, 45.0, 38.0, 36.0, 41.0, 65.0, 95.0, 110.0, 98.0, 80.0, 62.0, //
            48.0, 42.0, 46.0, 58.0, 74.0, 96.0, 121.0, 135.0, 128.0, 104.0, 88.0, 76.0,
        ];
        Self::from_hourly_prices(&PRICES, power_w, 3, 2)
    }
}

impl EmsStrategy for DayAheadEms {
    fn site_target_w(&self, unix_time_s: i64, _state: &SiteState) -> f64 {
        let hour = (unix_time_s.rem_euclid(86_400) / 3_600) as usize;
        if self.charge_hour[hour] {
            -self.power_w
        } else if self.discharge_hour[hour] {
            self.power_w
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::config::PlantConfig;
    use bess_core::state::SiteState;

    #[test]
    fn plan_picks_cheapest_and_most_expensive_hours() {
        let ems = DayAheadEms::default_profile(100.0e6);
        let cfg = PlantConfig::gw01();
        let state = SiteState::new(&cfg, 1, 0);
        // Hour 4 (36 EUR) charges, hour 19 (135 EUR) discharges.
        assert!(ems.site_target_w(4 * 3600, &state) < 0.0);
        assert!(ems.site_target_w(19 * 3600, &state) > 0.0);
        // Hour 12 sits in the middle of the curve: idle.
        assert!(ems.site_target_w(12 * 3600, &state).abs() < f64::EPSILON);
    }

    #[test]
    fn plan_repeats_daily_and_handles_pre_epoch_times() {
        let ems = DayAheadEms::default_profile(100.0e6);
        let cfg = PlantConfig::gw01();
        let state = SiteState::new(&cfg, 1, 0);
        let a = ems.site_target_w(19 * 3600, &state);
        let b = ems.site_target_w(19 * 3600 + 86_400 * 365, &state);
        assert!((a - b).abs() < f64::EPSILON);
        let c = ems.site_target_w(19 * 3600 - 86_400 * 2, &state);
        assert!((a - c).abs() < f64::EPSILON);
    }
}
