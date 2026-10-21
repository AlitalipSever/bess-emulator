//! The balancing products the plant holds and the reservoir rules it
//! serves them under (M4 phase 3 PR1).
//!
//! An award is a block of a product the plant holds for a four-hour
//! block of the day: FCR, symmetric, served by the droop block at the
//! awarded power; aFRR positive and negative, served by the TSO's request
//! (PR2). The EMS strategy says what the plant holds each tick
//! (`EmsStrategy::awards_at`); a scenario or a test can pin awards by
//! hand, and then the strategy's are ignored until the pin is lifted.
//!
//! FCR from a limited energy reservoir follows the SOGL rules as the
//! Swissgrid prequalification annex states them for Continental Europe:
//! continuous delivery in the normal state, a working range of state of
//! charge that keeps at least 15 minutes of full activation in reserve
//! each way, and an alert state (|Δf| ≥ 50 mHz for 15 min, ≥ 100 mHz for
//! 5 min, or ≥ 200 mHz) in which the range may be left and full power
//! must be held for at least 15 minutes. The German degrees of freedom
//! for charge management are modelled: using the dead band to move toward
//! the range's centre at up to the prequalified power, over-fulfilling the
//! characteristic by up to 20 % when the direction helps, and a scheduled
//! transaction (an energy block at the plan level) when the free degrees
//! are not enough.

use serde::{Deserialize, Serialize};

/// A balancing product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Product {
    /// Frequency containment reserve, symmetric.
    Fcr,
    /// Automatic frequency restoration reserve, positive (deliver).
    AfrrPos,
    /// Automatic frequency restoration reserve, negative (absorb).
    AfrrNeg,
}

impl Product {
    /// The name the surfaces and the scenario files use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fcr => "fcr",
            Self::AfrrPos => "afrr_pos",
            Self::AfrrNeg => "afrr_neg",
        }
    }

    /// The product a name spells.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "fcr" => Self::Fcr,
            "afrr_pos" => Self::AfrrPos,
            "afrr_neg" => Self::AfrrNeg,
            _ => return None,
        })
    }
}

/// What the plant holds this tick, per product: the awarded power and
/// the capacity price it is paid per MW and four-hour block.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Awards {
    /// FCR, W (symmetric).
    pub fcr_w: f64,
    /// aFRR positive, W.
    pub afrr_pos_w: f64,
    /// aFRR negative, W.
    pub afrr_neg_w: f64,
    /// FCR settlement capacity price, EUR per MW and block.
    pub fcr_price_eur_mw: f64,
    /// aFRR positive capacity price, EUR per MW and block.
    pub afrr_pos_price_eur_mw: f64,
    /// aFRR negative capacity price, EUR per MW and block.
    pub afrr_neg_price_eur_mw: f64,
}

impl Awards {
    /// Nothing held.
    pub const NONE: Self = Self {
        fcr_w: 0.0,
        afrr_pos_w: 0.0,
        afrr_neg_w: 0.0,
        fcr_price_eur_mw: 0.0,
        afrr_pos_price_eur_mw: 0.0,
        afrr_neg_price_eur_mw: 0.0,
    };

    /// The power of one product, W.
    pub fn power_w(&self, product: Product) -> f64 {
        match product {
            Product::Fcr => self.fcr_w,
            Product::AfrrPos => self.afrr_pos_w,
            Product::AfrrNeg => self.afrr_neg_w,
        }
    }

    /// Set the power of one product, W.
    pub fn set_power_w(&mut self, product: Product, w: f64) {
        match product {
            Product::Fcr => self.fcr_w = w,
            Product::AfrrPos => self.afrr_pos_w = w,
            Product::AfrrNeg => self.afrr_neg_w = w,
        }
    }

    /// The power reserved for balancing in each direction, W: FCR each
    /// way plus the aFRR of that direction. What the plan leaves as
    /// headroom.
    pub fn headroom_w(&self) -> (f64, f64) {
        (self.fcr_w + self.afrr_pos_w, self.fcr_w + self.afrr_neg_w)
    }
}

/// The rules of the aFRR service: how fast the plant follows the TSO's
/// request.
#[derive(Debug, Clone, PartialEq)]
pub struct AfrrRules {
    /// The gradient the plant follows the request with, as a share of
    /// the awarded power per second: the prequalification's minimum of
    /// 0.5 %/s, which reaches full activation in 200 s, inside ENTSO-E's
    /// five-minute full activation time.
    pub gradient_pu_per_s: f64,
}

impl Default for AfrrRules {
    fn default() -> Self {
        Self {
            gradient_pu_per_s: 0.005,
        }
    }
}

/// The rules of FCR from a limited energy reservoir.
#[derive(Debug, Clone, PartialEq)]
pub struct ReservoirRules {
    /// Hours of full activation the working range keeps in reserve each
    /// way (the 15-minute criterion: 0.25).
    pub reserve_h: f64,
    /// The share of the prequalified power the plant may use inside the
    /// dead band to move toward the range's centre (the German rule
    /// allows the full power; 1.0).
    pub deadband_use: f64,
    /// Over-fulfilment the plant may apply when the characteristic's
    /// direction helps the reservoir (1.2 is 120 %).
    pub overfulfil: f64,
    /// Alert: |Δf| at or beyond this for `alert_long_s` enters the alert
    /// state, Hz.
    pub alert_low_hz: f64,
    /// Seconds for `alert_low_hz`.
    pub alert_long_s: f64,
    /// Alert: |Δf| at or beyond this for `alert_short_s` enters it, Hz.
    pub alert_mid_hz: f64,
    /// Seconds for `alert_mid_hz`.
    pub alert_short_s: f64,
    /// Alert: |Δf| at or beyond this enters it at once, Hz.
    pub alert_high_hz: f64,
    /// Seconds the frequency has to sit inside `alert_low_hz` for the
    /// alert to end, once the minimum activation has been delivered.
    pub alert_release_s: f64,
    /// Minimum seconds of full activation in the alert state before it
    /// may end (15 minutes: 900).
    pub alert_min_s: f64,
}

impl Default for ReservoirRules {
    /// The SOGL figures as the Swissgrid annex states them, the German
    /// degrees of freedom at their public values; the release time is a
    /// labeled estimate.
    fn default() -> Self {
        Self {
            reserve_h: 0.25,
            deadband_use: 1.0,
            overfulfil: 1.2,
            alert_low_hz: 0.050,
            alert_long_s: 900.0,
            alert_mid_hz: 0.100,
            alert_short_s: 300.0,
            alert_high_hz: 0.200,
            alert_release_s: 60.0,
            alert_min_s: 900.0,
        }
    }
}

/// The reservoir manager's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Reservoir {
    /// In the alert state.
    pub alert: bool,
    /// Seconds |Δf| has sat at or beyond the low alert threshold.
    pub beyond_low_s: f64,
    /// Seconds |Δf| has sat at or beyond the mid alert threshold.
    pub beyond_mid_s: f64,
    /// Seconds in the alert state so far.
    pub alert_elapsed_s: f64,
    /// Seconds |Δf| has sat inside the low threshold while in alert.
    pub calm_s: f64,
    /// The recentering power the manager asked for this tick, W,
    /// positive to discharge (the reservoir is high).
    pub recenter_w: f64,
    /// The scheduled transaction in force, W at the plan level, positive
    /// to discharge; zero when none.
    pub transaction_w: f64,
    /// The working range this tick, as state of charge.
    pub range_min: f64,
    /// See `range_min`.
    pub range_max: f64,
}

impl ReservoirRules {
    /// The working range for a prequalified `pq_w` on a plant whose
    /// usable window is `window_min` to `window_max` of `energy_wh`
    /// nominal: the reserve each way, inside the window.
    pub fn range(&self, pq_w: f64, window_min: f64, window_max: f64, energy_wh: f64) -> (f64, f64) {
        if energy_wh <= 0.0 || pq_w <= 0.0 {
            return (window_min, window_max);
        }
        let reserve = self.reserve_h * pq_w / energy_wh;
        let lo = (window_min + reserve).min(window_max);
        let hi = (window_max - reserve).max(window_min);
        if lo <= hi {
            (lo, hi)
        } else {
            let mid = f64::midpoint(window_min, window_max);
            (mid, mid)
        }
    }

    /// Advance the alert clocks one tick on the frequency deviation.
    pub fn advance_alert(&self, r: &mut Reservoir, df_hz: f64, dt_s: f64) {
        let a = df_hz.abs();
        r.beyond_low_s = if a >= self.alert_low_hz {
            r.beyond_low_s + dt_s
        } else {
            0.0
        };
        r.beyond_mid_s = if a >= self.alert_mid_hz {
            r.beyond_mid_s + dt_s
        } else {
            0.0
        };
        if !r.alert {
            let enter = a >= self.alert_high_hz
                || r.beyond_mid_s >= self.alert_short_s
                || r.beyond_low_s >= self.alert_long_s;
            if enter {
                r.alert = true;
                r.alert_elapsed_s = 0.0;
                r.calm_s = 0.0;
            }
            return;
        }
        r.alert_elapsed_s += dt_s;
        r.calm_s = if a < self.alert_low_hz {
            r.calm_s + dt_s
        } else {
            0.0
        };
        if r.alert_elapsed_s >= self.alert_min_s && r.calm_s >= self.alert_release_s {
            r.alert = false;
            r.alert_elapsed_s = 0.0;
            r.calm_s = 0.0;
        }
    }

    /// The charge management for this tick, given the plant's state of
    /// charge, the droop's characteristic target (W, positive to
    /// deliver) and whether the frequency is inside the dead band.
    /// Returns the term to add to the droop's target, W, and sets
    /// `recenter_w`. Nothing in the alert state.
    pub fn manage(
        &self,
        r: &mut Reservoir,
        soc: f64,
        pq_w: f64,
        droop_target_w: f64,
        in_deadband: bool,
    ) -> f64 {
        r.recenter_w = 0.0;
        if r.alert || pq_w <= 0.0 {
            return 0.0;
        }
        let centre = f64::midpoint(r.range_min, r.range_max);
        let half = 0.5 * (r.range_max - r.range_min);
        if half <= 0.0 {
            return 0.0;
        }
        // How far the reservoir sits from the centre, -1 (empty side) to
        // 1 (full side): the manager pushes proportionally.
        let lean = ((soc - centre) / half).clamp(-1.0, 1.0);
        let term = if in_deadband {
            lean * self.deadband_use * pq_w
        } else if droop_target_w * lean > 0.0 {
            // The characteristic helps: over-fulfil toward the centre.
            (self.overfulfil - 1.0) * droop_target_w.abs() * lean.abs() * droop_target_w.signum()
        } else {
            0.0
        };
        r.recenter_w = term;
        term
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_keeps_fifteen_minutes_each_way() {
        let rules = ReservoirRules::default();
        // 20 MW on 180 MWh usable (0.05 to 0.95 of 200 MWh): 5 MWh each
        // way, 2.78 % of the nominal.
        let (lo, hi) = rules.range(20.0e6, 0.05, 0.95, 200.0e6);
        assert!((lo - 0.075).abs() < 1e-9, "{lo}");
        assert!((hi - 0.925).abs() < 1e-9, "{hi}");
        let (lo, hi) = rules.range(0.0, 0.05, 0.95, 200.0e6);
        assert_eq!((lo, hi), (0.05, 0.95));
        let (lo, hi) = rules.range(1.0e9, 0.05, 0.95, 200.0e6);
        assert_eq!(lo, hi);
    }

    #[test]
    fn the_alert_state_enters_on_the_three_criteria_and_releases_after_the_minimum() {
        let rules = ReservoirRules::default();
        let mut r = Reservoir::default();
        rules.advance_alert(&mut r, 0.2, 1.0);
        assert!(r.alert, "200 mHz enters at once");
        for _ in 0..899 {
            rules.advance_alert(&mut r, 0.0, 1.0);
        }
        assert!(r.alert, "fifteen minutes of activation first");
        for _ in 0..61 {
            rules.advance_alert(&mut r, 0.0, 1.0);
        }
        assert!(!r.alert, "released after a calm minute past the minimum");
        let mut r = Reservoir::default();
        for _ in 0..299 {
            rules.advance_alert(&mut r, 0.11, 1.0);
        }
        assert!(!r.alert);
        rules.advance_alert(&mut r, 0.11, 1.0);
        assert!(r.alert, "100 mHz for five minutes");
        let mut r = Reservoir::default();
        for _ in 0..900 {
            rules.advance_alert(&mut r, 0.06, 1.0);
        }
        assert!(r.alert, "50 mHz for fifteen minutes");
    }

    #[test]
    fn charge_management_uses_the_dead_band_and_overfulfils_when_it_helps() {
        let rules = ReservoirRules::default();
        let mut r = Reservoir {
            range_min: 0.2,
            range_max: 0.8,
            ..Reservoir::default()
        };
        // A full reservoir inside the dead band discharges at the full
        // prequalified power; an empty one charges.
        assert!((rules.manage(&mut r, 0.8, 20.0e6, 0.0, true) - 20.0e6).abs() < 1.0);
        assert!((rules.manage(&mut r, 0.2, 20.0e6, 0.0, true) + 20.0e6).abs() < 1.0);
        assert_eq!(rules.manage(&mut r, 0.5, 20.0e6, 0.0, true), 0.0);
        // Outside the dead band: the droop delivering with a full
        // reservoir over-fulfils by 20 %; against it, nothing.
        let term = rules.manage(&mut r, 0.8, 20.0e6, 10.0e6, false);
        assert!((term - 2.0e6).abs() < 1.0, "{term}");
        assert_eq!(rules.manage(&mut r, 0.8, 20.0e6, -10.0e6, false), 0.0);
        r.alert = true;
        assert_eq!(rules.manage(&mut r, 0.8, 20.0e6, 10.0e6, false), 0.0);
    }
}
