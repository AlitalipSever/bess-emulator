//! One local day's dispatch plan: which hours charge, which discharge.

/// Charge and discharge hours of one local day, as bits over the day's
/// slots (23 to 25 of them, so a `u32` holds any day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DayPlan {
    charge: u32,
    discharge: u32,
}

impl DayPlan {
    /// Rank one day's prices: the `charge_hours` cheapest slots charge, the
    /// `discharge_hours` dearest discharge. Ties break toward the earlier
    /// slot in both rankings, so the plan is deterministic. The counts are
    /// the same on every day, the 23-hour and 25-hour ones included: a DST
    /// day is a day with one hour less or more to choose from, not a day
    /// that needs more or less energy.
    ///
    /// # Panics
    /// Panics when the day has more than 32 slots.
    pub fn rank(prices: &[f64], charge_hours: usize, discharge_hours: usize) -> Self {
        assert!(prices.len() <= 32, "a day of {} slots", prices.len());
        let mut slots: Vec<usize> = (0..prices.len()).collect();
        slots.sort_unstable_by(|a, b| prices[*a].total_cmp(&prices[*b]).then(a.cmp(b)));
        let charge = bits(slots.iter().take(charge_hours));
        slots.sort_unstable_by(|a, b| prices[*b].total_cmp(&prices[*a]).then(a.cmp(b)));
        let discharge = bits(slots.iter().take(discharge_hours));
        Self { charge, discharge }
    }

    /// True when slot `slot` of the day charges.
    pub fn charges(self, slot: usize) -> bool {
        self.charge >> slot & 1 == 1
    }

    /// True when slot `slot` of the day discharges.
    pub fn discharges(self, slot: usize) -> bool {
        self.discharge >> slot & 1 == 1
    }
}

fn bits<'a>(slots: impl Iterator<Item = &'a usize>) -> u32 {
    slots.fold(0, |acc, &s| acc | 1 << s)
}

#[cfg(test)]
mod tests {
    use super::DayPlan;

    fn picked(plan: DayPlan, len: usize, f: fn(DayPlan, usize) -> bool) -> Vec<usize> {
        (0..len).filter(|&s| f(plan, s)).collect()
    }

    #[test]
    fn cheapest_charge_and_dearest_discharge() {
        let prices = [50.0, 10.0, 40.0, 5.0, 90.0, 20.0, 80.0, 60.0];
        let plan = DayPlan::rank(&prices, 3, 2);
        assert_eq!(picked(plan, 8, DayPlan::charges), [1, 3, 5]);
        assert_eq!(picked(plan, 8, DayPlan::discharges), [4, 6]);
    }

    #[test]
    fn ties_break_toward_the_earlier_slot_both_ways() {
        let prices = [7.0; 6];
        let plan = DayPlan::rank(&prices, 2, 2);
        assert_eq!(picked(plan, 6, DayPlan::charges), [0, 1]);
        assert_eq!(picked(plan, 6, DayPlan::discharges), [0, 1]);
    }

    #[test]
    fn negative_prices_rank_below_zero() {
        let prices = [0.0, -12.5, 3.0, -0.01, 8.0];
        let plan = DayPlan::rank(&prices, 2, 1);
        assert_eq!(picked(plan, 5, DayPlan::charges), [1, 3]);
        assert_eq!(picked(plan, 5, DayPlan::discharges), [4]);
    }

    #[test]
    fn a_25_slot_day_can_pick_its_last_slot() {
        let mut prices = [50.0; 25];
        prices[24] = -1.0;
        prices[23] = 999.0;
        let plan = DayPlan::rank(&prices, 1, 1);
        assert!(plan.charges(24));
        assert!(plan.discharges(23));
    }
}
