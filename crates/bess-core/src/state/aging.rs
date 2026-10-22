//! What a rack has been through and what it did to the rack: the stress
//! ledger a tick fills, the day it closes into, and the aging states a
//! model advances from that day (M5 phase 1).
//!
//! The ledger is sums and extrema only. The published models the kernel
//! runs are identified on check-ups days apart and take window averages,
//! so a day's stress is eight numbers, not a trace, and the aging states
//! move once per closed day per rack.

use serde::{Deserialize, Serialize};

/// Seconds of ledger time a day closes at: the run's day boundary from
/// the rack's last close, not local midnight, so a day's stress is one
/// row whatever the start time (design D3).
pub const DAY_S: f64 = 86_400.0;

/// The state of health the kernel never lets a rack fall below: a rack
/// with no capacity at all would divide the cell model by zero. A plant
/// this old has long stopped being a plant; the floor is a guard, not a
/// physical claim.
pub const SOH_FLOOR: f64 = 0.05;

/// The open day of one rack: sums and extrema since the last close.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct StressLedger {
    /// Seconds observed since the last close.
    pub seconds: f64,
    /// Sum of the state of charge over the observed seconds.
    pub soc_sum: f64,
    /// Sum of the cell temperature in kelvin over the observed seconds.
    pub temp_k_sum: f64,
    /// Charge moved through the terminals either way, Ah.
    pub throughput_ah: f64,
    /// Sum of the current's magnitude over the seconds it flowed, A s.
    pub abs_current_sum_a: f64,
    /// Seconds in which current flowed.
    pub active_s: f64,
    /// Lowest state of charge seen.
    pub soc_min: f64,
    /// Highest state of charge seen.
    pub soc_max: f64,
}

/// Current below which a tick counts as rest for the rate average: the
/// BLAST-Lite extraction drops rates under 0.01C before averaging.
const REST_C_RATE: f64 = 0.01;

impl StressLedger {
    /// An empty ledger whose extrema start at the rack's state of charge.
    pub fn open(soc: f64) -> Self {
        Self {
            soc_min: soc,
            soc_max: soc,
            ..Self::default()
        }
    }

    /// Add one tick of the rack's state.
    pub fn observe(
        &mut self,
        soc: f64,
        cell_temp_c: f64,
        current_a: f64,
        nominal_ah: f64,
        dt_s: f64,
    ) {
        if self.seconds <= 0.0 {
            self.soc_min = soc;
            self.soc_max = soc;
        }
        self.seconds += dt_s;
        self.soc_sum += soc * dt_s;
        self.temp_k_sum += (cell_temp_c + 273.15) * dt_s;
        let abs_a = current_a.abs();
        self.throughput_ah += abs_a * dt_s / 3600.0;
        if abs_a > REST_C_RATE * nominal_ah {
            self.abs_current_sum_a += abs_a * dt_s;
            self.active_s += dt_s;
        }
        self.soc_min = self.soc_min.min(soc);
        self.soc_max = self.soc_max.max(soc);
    }

    /// Whether a day of ledger time has accumulated.
    pub fn is_full(&self) -> bool {
        self.seconds + 1.0e-9 >= DAY_S
    }

    /// Close the ledger into a day's stress against the rack's nominal
    /// capacity. An empty ledger closes into a day of rest at 50 % and
    /// 25 °C, which no caller asks for; the guard keeps the arithmetic
    /// finite.
    pub fn close(&self, nominal_ah: f64) -> StressDay {
        if self.seconds <= 0.0 {
            return StressDay {
                days: 0.0,
                efc: 0.0,
                soc_mean: 0.5,
                temp_k_mean: 298.15,
                dod: 0.0,
                c_rate: 0.0,
            };
        }
        let c_rate = if self.active_s > 0.0 {
            self.abs_current_sum_a / self.active_s / nominal_ah
        } else {
            0.0
        };
        StressDay {
            days: self.seconds / DAY_S,
            efc: self.throughput_ah / (2.0 * nominal_ah),
            soc_mean: self.soc_sum / self.seconds,
            temp_k_mean: self.temp_k_sum / self.seconds,
            dod: (self.soc_max - self.soc_min).max(0.0),
            c_rate,
        }
    }
}

/// One closed day of a rack's stress: what the aging model reads.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StressDay {
    /// Length of the window in days (1.0 for a closed ledger; a
    /// fast-forward may hand the model longer windows).
    pub days: f64,
    /// Equivalent full cycles in the window: throughput over twice the
    /// nominal capacity.
    pub efc: f64,
    /// Mean state of charge, 0..1.
    pub soc_mean: f64,
    /// Mean cell temperature, kelvin.
    pub temp_k_mean: f64,
    /// Depth of discharge: the window's state-of-charge range, 0..1.
    pub dod: f64,
    /// Mean rate while current flowed, in C (1 C is the nominal capacity
    /// in one hour); zero for a day at rest.
    pub c_rate: f64,
}

/// A rack's aging: two capacity-loss states and two resistance-growth
/// states, its own rate, its open ledger and its last closed day.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RackAge {
    /// Simulated days since commissioning, ticked or skipped.
    pub days: f64,
    /// Equivalent full cycles since commissioning.
    pub efc: f64,
    /// Capacity lost to calendar aging, fraction of nominal.
    pub q_loss_cal: f64,
    /// Capacity lost to cycle aging, fraction of nominal.
    pub q_loss_cyc: f64,
    /// Resistance gained by calendar aging, fraction of the rack's
    /// commissioning resistance.
    pub r_gain_cal: f64,
    /// Resistance gained by cycle aging, fraction.
    pub r_gain_cyc: f64,
    /// This rack's aging speed relative to the model, drawn once at
    /// commissioning (design D5); every state's increment is scaled by it.
    pub rate_scale: f64,
    /// The open day.
    pub ledger: StressLedger,
    /// The last closed day, kept for an observer that collects a stress
    /// year (phase 2); `None` before the first close.
    pub last_day: Option<StressDay>,
}

impl RackAge {
    /// A rack at commissioning.
    pub fn fresh(rate_scale: f64) -> Self {
        Self {
            days: 0.0,
            efc: 0.0,
            q_loss_cal: 0.0,
            q_loss_cyc: 0.0,
            r_gain_cal: 0.0,
            r_gain_cyc: 0.0,
            rate_scale,
            ledger: StressLedger::default(),
            last_day: None,
        }
    }

    /// State of health the states imply, floored (see [`SOH_FLOOR`]).
    pub fn soh(&self) -> f64 {
        (1.0 - self.q_loss_cal - self.q_loss_cyc).max(SOH_FLOOR)
    }

    /// Total resistance growth, fraction of the commissioning resistance.
    pub fn resistance_growth(&self) -> f64 {
        self.r_gain_cal + self.r_gain_cyc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ledger_closes_into_the_days_means_and_extrema() {
        let mut ledger = StressLedger::open(0.5);
        // Half a day discharging at 157 A (0.5C of 314 Ah), half at rest.
        for _ in 0..43_200 {
            ledger.observe(0.6, 25.0, 157.0, 314.0, 1.0);
        }
        for _ in 0..43_200 {
            ledger.observe(0.2, 35.0, 0.0, 314.0, 1.0);
        }
        assert!(ledger.is_full());
        let day = ledger.close(314.0);
        assert!((day.days - 1.0).abs() < 1.0e-12);
        // 157 A for 12 h is 1884 Ah: three equivalent full cycles.
        assert!((day.efc - 3.0).abs() < 1.0e-9, "{}", day.efc);
        assert!((day.soc_mean - 0.4).abs() < 1.0e-9);
        assert!((day.temp_k_mean - 303.15).abs() < 1.0e-9);
        assert!((day.dod - 0.4).abs() < 1.0e-12);
        assert!((day.c_rate - 0.5).abs() < 1.0e-9);
    }

    #[test]
    fn rest_is_not_counted_in_the_rate() {
        let mut ledger = StressLedger::open(0.5);
        ledger.observe(0.5, 20.0, 1.0, 314.0, 1.0);
        let day = ledger.close(314.0);
        assert_eq!(day.c_rate, 0.0);
        assert!(day.efc > 0.0);
    }

    #[test]
    fn a_fresh_rack_is_whole() {
        let age = RackAge::fresh(1.05);
        assert_eq!(age.soh(), 1.0);
        assert_eq!(age.resistance_growth(), 0.0);
        let worn = RackAge {
            q_loss_cal: 0.6,
            q_loss_cyc: 0.6,
            ..age
        };
        assert_eq!(worn.soh(), SOH_FLOOR);
    }
}
