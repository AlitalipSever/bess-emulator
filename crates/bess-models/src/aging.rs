//! Aging: how a day of stress moves a rack's capacity and resistance.
//!
//! The capacity model is NREL's for a large-format prismatic LFP cell
//! (`nrel`), the resistance laws Naumann's (`naumann`), both advanced as
//! the published trajectories at their current slope (`power_law`); the
//! kernel calls `age_rack` once per rack per closed day and reads the
//! states back as `soh` and a resistance multiplier. `NoAging` is the
//! switch-off proof's model.

pub mod naumann;
pub mod nrel;
pub mod power_law;

use bess_core::config::RackConfig;
use bess_core::state::aging::{RackAge, StressDay};
use bess_core::traits::AgingModel;

pub use naumann::NaumannResistance;
pub use nrel::NrelLfp250;

/// The LFP aging model the GW-01 plant runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LfpAging {
    /// Capacity fade.
    pub capacity: NrelLfp250,
    /// Resistance growth.
    pub resistance: NaumannResistance,
}

impl LfpAging {
    /// The published constants for both.
    pub const fn gw01() -> Self {
        Self {
            capacity: NrelLfp250::blast_lite_2023(),
            resistance: NaumannResistance::simses(),
        }
    }
}

impl AgingModel for LfpAging {
    fn age_rack(&self, age: &mut RackAge, _cfg: &RackConfig, day: &StressDay) {
        let scale = age.rate_scale;
        let cap = &self.capacity;
        let k_cal = cap.k_cal(day.temp_k_mean, day.soc_mean);
        age.q_loss_cal += scale * power_law::advance(age.q_loss_cal, day.days, k_cal, cap.pcal);
        let k_cyc = cap.k_cyc(day.temp_k_mean, day.dod, day.c_rate);
        age.q_loss_cyc += scale * power_law::advance(age.q_loss_cyc, day.efc, k_cyc, cap.pcyc);
        let res = &self.resistance;
        age.r_gain_cal +=
            scale * res.calendar_gain(day.temp_k_mean, day.soc_mean, day.days * 86_400.0);
        age.r_gain_cyc += scale * res.cycle_gain(day.c_rate, day.dod, day.efc);
    }
}

/// A rack that never ages: the states stay where they are, the ledger
/// and the counters still move. The switch-off proof's model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoAging;

impl AgingModel for NoAging {
    fn age_rack(&self, _age: &mut RackAge, _cfg: &RackConfig, _day: &StressDay) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::PlantConfig;

    fn day(temp_c: f64, efc: f64) -> StressDay {
        StressDay {
            days: 1.0,
            efc,
            soc_mean: 0.5,
            temp_k_mean: temp_c + 273.15,
            dod: 0.8,
            c_rate: 0.5,
        }
    }

    /// A year of the design's stress, day by day, lands where the
    /// closed forms put it (D1, D2), within the daily discretization.
    #[test]
    fn a_year_of_days_lands_near_the_closed_forms() {
        let model = LfpAging::gw01();
        let cfg = PlantConfig::gw01().rack;
        let mut age = RackAge::fresh(1.0);
        for _ in 0..365 {
            model.age_rack(&mut age, &cfg, &day(25.0, 280.0 / 365.0));
        }
        assert!(
            (age.q_loss_cal - 0.0110).abs() < 0.0005,
            "{}",
            age.q_loss_cal
        );
        assert!(
            (age.q_loss_cyc - 0.0231).abs() < 0.0010,
            "{}",
            age.q_loss_cyc
        );
        assert!(
            (age.r_gain_cal - 0.0168).abs() < 0.0003,
            "{}",
            age.r_gain_cal
        );
        assert!(
            (age.r_gain_cyc - 0.0034).abs() < 0.0002,
            "{}",
            age.r_gain_cyc
        );
        assert!((age.soh() - 0.966).abs() < 0.002, "{}", age.soh());
    }

    #[test]
    fn the_rate_scale_multiplies_every_state() {
        let model = LfpAging::gw01();
        let cfg = PlantConfig::gw01().rack;
        let mut slow = RackAge::fresh(0.9);
        let mut fast = RackAge::fresh(1.1);
        model.age_rack(&mut slow, &cfg, &day(25.0, 1.0));
        model.age_rack(&mut fast, &cfg, &day(25.0, 1.0));
        for (s, f) in [
            (slow.q_loss_cal, fast.q_loss_cal),
            (slow.q_loss_cyc, fast.q_loss_cyc),
            (slow.r_gain_cal, fast.r_gain_cal),
            (slow.r_gain_cyc, fast.r_gain_cyc),
        ] {
            assert!((f / s - 1.1 / 0.9).abs() < 1.0e-9);
        }
    }

    #[test]
    fn no_aging_leaves_the_states() {
        let cfg = PlantConfig::gw01().rack;
        let mut age = RackAge::fresh(1.0);
        NoAging.age_rack(&mut age, &cfg, &day(45.0, 3.0));
        assert_eq!(age, RackAge::fresh(1.0));
    }
}
