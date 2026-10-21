//! Day-ahead dispatch plan over a replayed price year.
//!
//! Since M2 phase 3 the planner ranks real prices: the 2024 German
//! day-ahead series from bess-data, one plan per local day. The rule is
//! the M0 rule unchanged (charge the cheapest hours at full power,
//! discharge the dearest); what changed is what a day is. A plan day is a
//! Europe/Berlin day, so the two DST days are planned over 23 and 25
//! hours. Balancing-market activation replay and setpoint tracking quality
//! arrive in M4; market behaviour does not get smarter here.

pub mod plan;

use std::fmt;

use bess_core::kernel::Awards;
use bess_core::state::SiteState;
use bess_core::traits::EmsStrategy;
use bess_data::PriceYear;

use crate::calendar::reference_hour;
use crate::market::AwardPlan;
pub use plan::DayPlan;

/// Day-ahead strategy: one [`DayPlan`] per local day of a price year,
/// computed at construction, so a tick costs one table lookup and no
/// allocation.
#[derive(Clone, PartialEq)]
pub struct DayAheadEms {
    power_w: f64,
    prices: &'static PriceYear,
    plans: Vec<DayPlan>,
    /// What the plant sells into the balancing markets in every block
    /// (M4 phase 3); nothing by default.
    awards: AwardPlan,
}

impl fmt::Debug for DayAheadEms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DayAheadEms")
            .field("power_w", &self.power_w)
            .field("price_year", &self.prices.year())
            .field("days", &self.plans.len())
            .finish_non_exhaustive()
    }
}

impl DayAheadEms {
    /// Plan every local day of `prices`: `charge_hours` cheapest hours at
    /// full power, `discharge_hours` dearest. The reference site charges
    /// three and discharges two: a 2 h plant needs the extra charge hour to
    /// cover conversion losses.
    pub fn from_price_year(
        prices: &'static PriceYear,
        power_w: f64,
        charge_hours: usize,
        discharge_hours: usize,
    ) -> Self {
        let plans = (0..prices.days())
            .map(|d| {
                let day = &prices.prices_eur_mwh()[prices.day_hours(d)];
                DayPlan::rank(day, charge_hours, discharge_hours)
            })
            .collect();
        Self {
            power_w,
            prices,
            plans,
            awards: AwardPlan::NONE,
        }
    }

    /// The same plan, selling `awards` in every block of every day (M4
    /// phase 3). The kernel keeps the plan's target inside the rating
    /// less the awards' headroom.
    #[must_use]
    pub fn with_awards(self, awards: AwardPlan) -> Self {
        Self { awards, ..self }
    }

    /// The local day and the hour of that day a timestamp falls in.
    fn local(&self, unix_time_s: i64) -> (usize, usize) {
        let idx = self
            .prices
            .index_at_utc_hour(reference_hour(unix_time_s, self.prices.year()));
        let day = self.prices.day_of(idx);
        (day, idx - self.prices.day_hours(day).start)
    }
}

impl EmsStrategy for DayAheadEms {
    fn awards_at(&self, unix_time_s: i64, _state: &SiteState) -> Awards {
        if self.awards == AwardPlan::NONE {
            return Awards::NONE;
        }
        let (day, hour) = self.local(unix_time_s);
        // The four-hour blocks are local; a 25-hour day's last hour sits
        // in the last block.
        self.awards.awards(day, (hour / 4).min(5))
    }

    fn site_target_w(&self, unix_time_s: i64, _state: &SiteState) -> f64 {
        let (day, slot) = self.local(unix_time_s);
        let plan = self.plans[day];
        if plan.charges(slot) {
            -self.power_w
        } else if plan.discharges(slot) {
            self.power_w
        } else {
            0.0
        }
    }
}
