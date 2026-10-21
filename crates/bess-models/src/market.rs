//! The German balancing markets as the plant sells into them (M4 phase
//! 3): the products' prices, a fixed award plan, and the one place the
//! replayed tender year is attached.

pub mod prices;

use std::sync::OnceLock;

use bess_core::kernel::Awards;
use bess_data::BalancingYear;

/// The replayed balancing year this process uses, if one was attached
/// (`--balancing`); one per process, like the frequency year.
static BALANCING: OnceLock<BalancingYear> = OnceLock::new();

/// Attach a replayed balancing year for every strategy this process
/// builds from here on. Once per process: `None` when one is already
/// attached, and the year handed in is dropped.
pub fn replay_balancing(year: BalancingYear) -> Option<&'static BalancingYear> {
    BALANCING.set(year).ok()?;
    BALANCING.get()
}

/// The balancing year attached to this process, if any.
pub fn replayed_balancing() -> Option<&'static BalancingYear> {
    BALANCING.get()
}

/// A fixed award plan: what the plant holds in every four-hour block of
/// every day, as power. Phase 4's planner chooses per block; this is the
/// phase 3 configuration.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AwardPlan {
    /// FCR held, W.
    pub fcr_w: f64,
    /// aFRR positive held, W.
    pub afrr_pos_w: f64,
    /// aFRR negative held, W.
    pub afrr_neg_w: f64,
}

impl AwardPlan {
    /// Nothing sold: the day-ahead plant.
    pub const NONE: Self = Self {
        fcr_w: 0.0,
        afrr_pos_w: 0.0,
        afrr_neg_w: 0.0,
    };

    /// The awards for local day `day` and block `block`, priced from the
    /// replayed year when one is attached and from the annual figures
    /// otherwise.
    pub fn awards(self, day: usize, block: usize) -> Awards {
        let (fcr, pos, neg) = match replayed_balancing() {
            Some(year) if day < year.days() => (
                year.fcr_eur_mw_block(day, block),
                year.afrr_eur_mw_h(day, block, true),
                year.afrr_eur_mw_h(day, block, false),
            ),
            _ => (
                prices::FCR_EUR_MW_BLOCK_2024[block],
                prices::AFRR_POS_EUR_MW_H_2024[block],
                prices::AFRR_NEG_EUR_MW_H_2024[block],
            ),
        };
        Awards {
            fcr_w: self.fcr_w,
            afrr_pos_w: self.afrr_pos_w,
            afrr_neg_w: self.afrr_neg_w,
            fcr_price_eur_mw: fcr,
            afrr_pos_price_eur_mw: pos * prices::BLOCK_H,
            afrr_neg_price_eur_mw: neg * prices::BLOCK_H,
        }
    }
}
