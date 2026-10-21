//! The market participant's day (M4 phase 4): what to sell each block,
//! and a day-ahead plan whose charges feed its discharges.
//!
//! Each local day the planner reads the day's prices (the day-ahead
//! hours, the FCR price per block, the aFRR capacity price per block and
//! direction) and, per four-hour block, compares the value of three uses
//! of the plant's power: keeping it for arbitrage (the block's share of
//! the day's best spreads), offering it to FCR, or offering it to aFRR
//! in both directions. The best use takes the block; a block FCR takes
//! keeps the reservoir's working range, which costs the plan the
//! reserve's energy. Perfect foresight inside the day, like the public
//! index; none across days. A ranking, not an optimizer: the index's own
//! method, simple enough to be calculable by hand.
//!
//! The plan itself keeps the day-ahead rule (the cheapest hours charge,
//! the dearest discharge) and adds the ordering the M2 finding asked for:
//! a discharge hour counts only when a charge hour precedes it in the
//! day or the plant starts the day with the energy for it, so the plant
//! no longer plans to discharge what it has not got.

use bess_core::kernel::Awards;
use bess_core::state::SiteState;
use bess_core::traits::{EmsStrategy, MarketPrices};
use bess_data::PriceYear;

use crate::calendar::reference_hour;
use crate::ems::plan::DayPlan;
use crate::market::{prices, AwardPlan};

/// How a block's power is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockUse {
    /// Kept for the day-ahead plan.
    Arbitrage,
    /// Offered to FCR (a share) with the rest arbitraging.
    Fcr,
    /// Offered to aFRR, both directions.
    Afrr,
}

impl BlockUse {
    /// The register value: 0 arbitrage, 1 fcr, 2 afrr.
    pub fn code(self) -> u16 {
        match self {
            Self::Arbitrage => 0,
            Self::Fcr => 1,
            Self::Afrr => 2,
        }
    }

    /// The name the surfaces publish.
    pub fn name(self) -> &'static str {
        match self {
            Self::Arbitrage => "arbitrage",
            Self::Fcr => "fcr",
            Self::Afrr => "afrr",
        }
    }
}

/// The planner's knobs.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketRules {
    /// The share of the rating FCR may take in a block (the index offers
    /// 80 %; the reservoir's range leaves the rest).
    pub fcr_share: f64,
    /// The share of the rating aFRR may take in each direction.
    pub afrr_share: f64,
    /// The round-trip efficiency the arbitrage value is discounted by.
    pub round_trip: f64,
    /// Whether the ordering rule (charges before discharges) is on.
    pub ordered: bool,
}

impl Default for MarketRules {
    fn default() -> Self {
        Self {
            fcr_share: 0.8,
            afrr_share: 0.5,
            round_trip: 0.83,
            ordered: true,
        }
    }
}

/// One day's decision: the use per block and the plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayDecision {
    /// Per four-hour block.
    pub uses: [BlockUse; 6],
    /// The day-ahead plan for the hours arbitrage keeps.
    pub plan: DayPlan,
}

/// The market participant's strategy.
#[derive(Clone)]
pub struct MarketEms {
    power_w: f64,
    prices: &'static PriceYear,
    rules: MarketRules,
    decisions: Vec<DayDecision>,
}

impl std::fmt::Debug for MarketEms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketEms")
            .field("power_w", &self.power_w)
            .field("price_year", &self.prices.year())
            .field("days", &self.decisions.len())
            .finish_non_exhaustive()
    }
}

impl MarketEms {
    /// Decide every local day of `prices` for a plant of `power_w` and
    /// `energy_wh` usable energy.
    pub fn from_price_year(
        prices: &'static PriceYear,
        power_w: f64,
        energy_wh: f64,
        rules: MarketRules,
    ) -> Self {
        let decisions = (0..prices.days())
            .map(|d| decide(prices, d, power_w, energy_wh, &rules))
            .collect();
        Self {
            power_w,
            prices,
            rules,
            decisions,
        }
    }

    /// The decision of local day `d`.
    pub fn decision(&self, d: usize) -> DayDecision {
        self.decisions[d]
    }

    /// The local day and the hour of that day a timestamp falls in.
    fn local(&self, unix_time_s: i64) -> (usize, usize) {
        let idx = self
            .prices
            .index_at_utc_hour(reference_hour(unix_time_s, self.prices.year()));
        let day = self.prices.day_of(idx);
        (day, idx - self.prices.day_hours(day).start)
    }

    /// The block use at a timestamp.
    pub fn block_use_at(&self, unix_time_s: i64) -> BlockUse {
        let (day, hour) = self.local(unix_time_s);
        self.decisions[day].uses[(hour / 4).min(5)]
    }
}

/// The value of the three uses of one block of a day, EUR per MW of
/// rating, and the plan for the hours arbitrage keeps.
fn decide(
    prices: &PriceYear,
    d: usize,
    power_w: f64,
    energy_wh: f64,
    rules: &MarketRules,
) -> DayDecision {
    let day = &prices.prices_eur_mwh()[prices.day_hours(d)];
    let hours = day.len();
    let charge_hours = 3usize;
    let discharge_hours = 2usize;
    let base = DayPlan::rank(day, charge_hours, discharge_hours);
    // The day's arbitrage value per MW of plan power: what the discharge
    // hours sell, discounted by the round trip, less what the charge hours
    // buy, one MWh per MW and hour; shared over the blocks the plan uses
    // in proportion to their plan hours, since a trough block and a peak
    // block earn the spread together. A plant shorter than its plan hours
    // scales down with its hours of energy per MW.
    let hours_per_mw = (energy_wh / power_w).min(discharge_hours as f64);
    let day_value: f64 = (0..hours)
        .map(|h| {
            if base.discharges(h) {
                day[h] * rules.round_trip
            } else if base.charges(h) {
                -day[h]
            } else {
                0.0
            }
        })
        .sum::<f64>()
        .max(0.0)
        * hours_per_mw
        / discharge_hours as f64;
    let plan_hours = (0..hours)
        .filter(|&h| base.charges(h) || base.discharges(h))
        .count()
        .max(1);
    let mut uses = [BlockUse::Arbitrage; 6];
    for (b, u) in uses.iter_mut().enumerate() {
        let block_plan_hours = (0..hours)
            .filter(|&h| (h / 4).min(5) == b && (base.charges(h) || base.discharges(h)))
            .count();
        let arbitrage = day_value * block_plan_hours as f64 / plan_hours as f64;
        let fcr = prices::FCR_EUR_MW_BLOCK_2024[b] * rules.fcr_share;
        let afrr = (prices::AFRR_POS_EUR_MW_H_2024[b] + prices::AFRR_NEG_EUR_MW_H_2024[b])
            * prices::BLOCK_H
            * rules.afrr_share;
        *u = if fcr >= arbitrage && fcr >= afrr {
            BlockUse::Fcr
        } else if afrr > arbitrage {
            BlockUse::Afrr
        } else {
            BlockUse::Arbitrage
        };
    }
    let plan = if rules.ordered {
        ordered(day, base, charge_hours, discharge_hours)
    } else {
        base
    };
    DayDecision { uses, plan }
}

/// The ordering rule: a discharge slot is kept only when a charge slot
/// precedes it; a dropped discharge is replaced by the next-dearest slot
/// that has a charge before it, if any.
fn ordered(day: &[f64], base: DayPlan, charge_hours: usize, discharge_hours: usize) -> DayPlan {
    let first_charge = (0..day.len()).find(|&s| base.charges(s));
    let Some(first_charge) = first_charge else {
        return base;
    };
    // Re-rank the discharges among the slots after the first charge.
    let mut candidates: Vec<usize> = (first_charge + 1..day.len())
        .filter(|&s| !base.charges(s))
        .collect();
    candidates.sort_by(|a, b| day[*b].total_cmp(&day[*a]).then(a.cmp(b)));
    let picked: Vec<usize> = candidates.into_iter().take(discharge_hours).collect();
    DayPlan::with_slots(
        (0..day.len())
            .filter(|&s| base.charges(s))
            .take(charge_hours),
        picked.into_iter(),
    )
}

impl EmsStrategy for MarketEms {
    fn site_target_w(&self, unix_time_s: i64, _state: &SiteState) -> f64 {
        let (day, slot) = self.local(unix_time_s);
        let plan = self.decisions[day].plan;
        if plan.charges(slot) {
            -self.power_w
        } else if plan.discharges(slot) {
            self.power_w
        } else {
            0.0
        }
    }

    fn awards_at(&self, unix_time_s: i64, _state: &SiteState) -> Awards {
        let (day, hour) = self.local(unix_time_s);
        let block = (hour / 4).min(5);
        let plan = match self.decisions[day].uses[block] {
            BlockUse::Arbitrage => AwardPlan::NONE,
            BlockUse::Fcr => AwardPlan {
                fcr_w: self.power_w * self.rules.fcr_share,
                afrr_pos_w: 0.0,
                afrr_neg_w: 0.0,
            },
            BlockUse::Afrr => AwardPlan {
                fcr_w: 0.0,
                afrr_pos_w: self.power_w * self.rules.afrr_share,
                afrr_neg_w: self.power_w * self.rules.afrr_share,
            },
        };
        plan.awards(day, block)
    }

    fn prices_at(&self, unix_time_s: i64, state: &SiteState) -> MarketPrices {
        crate::ems::day_ahead_prices_at(self.prices, unix_time_s, state)
    }

    fn block_use_at(&self, unix_time_s: i64, _state: &SiteState) -> u16 {
        self.block_use_at(unix_time_s).code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_data::PriceYear;

    fn year(days: &[Vec<f64>]) -> &'static PriceYear {
        let mut starts = vec![0u16];
        let mut eur = Vec::new();
        for d in days {
            eur.extend(d.iter().copied());
            starts.push(eur.len() as u16);
        }
        Box::leak(Box::new(
            PriceYear::from_parts(2024, 1, eur, starts).expect("a hand-made year"),
        ))
    }

    /// The M2 counter-example: a morning peak before the midday trough.
    /// Unordered, the plan discharges at 07:00 and 08:00 into a plant
    /// that charges at noon; ordered, the discharges move after the
    /// charges, to the dearest later hours.
    #[test]
    fn the_ordering_rule_puts_charges_before_discharges() {
        let mut day = vec![60.0; 24];
        day[7] = 150.0;
        day[8] = 140.0;
        day[12] = 5.0;
        day[13] = 8.0;
        day[14] = 10.0;
        day[19] = 120.0;
        day[20] = 110.0;
        let base = DayPlan::rank(&day, 3, 2);
        assert!(base.discharges(7) && base.discharges(8));
        let plan = ordered(&day, base, 3, 2);
        assert!(!plan.discharges(7) && !plan.discharges(8));
        assert!(plan.discharges(19) && plan.discharges(20));
        assert!(plan.charges(12) && plan.charges(13) && plan.charges(14));
        // A day with no spread worth a discharge after the charges keeps
        // the charges and discharges nothing.
        let mut flat = vec![50.0; 24];
        flat[22] = 1.0;
        flat[23] = 2.0;
        flat[21] = 3.0;
        let plan = ordered(&flat, DayPlan::rank(&flat, 3, 2), 3, 2);
        assert!((0..24).all(|s| !plan.discharges(s)));
    }

    /// The block decision: a block whose FCR price beats the spread sells
    /// FCR, a block with a wide spread arbitrages, and the awards follow
    /// the decision at the rules' shares.
    #[test]
    fn blocks_go_to_the_best_paid_use() {
        let mut day = vec![80.0; 24];
        // Block 12-16 holds the trough, block 16-20 the peak: a 100 EUR
        // spread per MWh, worth more than the FCR block price there.
        day[13] = 10.0;
        day[14] = 12.0;
        day[15] = 14.0;
        day[18] = 160.0;
        day[19] = 150.0;
        let prices = year(&[day]);
        let ems = MarketEms::from_price_year(prices, 100.0e6, 200.0e6, MarketRules::default());
        let d = ems.decision(0);
        assert_eq!(d.uses[3], BlockUse::Arbitrage, "the trough block charges");
        assert_eq!(d.uses[4], BlockUse::Arbitrage, "the peak block discharges");
        assert_ne!(d.uses[0], BlockUse::Arbitrage, "a flat night sells reserve");
        // 2024-01-01 00:30 UTC is 01:30 local, block 0.
        let t = 1_704_067_200 + 1800;
        let awards = ems.awards_at(
            t,
            &bess_core::SiteState::new(&bess_core::PlantConfig::gw01(), 1, t),
        );
        match d.uses[0] {
            BlockUse::Fcr => assert_eq!(awards.fcr_w, 80.0e6),
            BlockUse::Afrr => assert_eq!(awards.afrr_pos_w, 50.0e6),
            BlockUse::Arbitrage => unreachable!(),
        }
        assert_eq!(ems.block_use_at(t), d.uses[0]);
    }
}
