//! A year of stress, rack by rack: what a ticked year leaves behind and
//! what a fast-forward replays (M5 phase 2).
//!
//! One closed `StressDay` per rack per day, collected from the racks'
//! `last_day` as the days close. It lives beside the run (the bench, the
//! shell), never in the checkpoint: a projected plant's age is in its
//! racks, the stress that produced it is the run's record.

use crate::state::aging::StressDay;
use crate::state::SiteState;

/// Closed days per rack, in the order they closed.
#[derive(Debug, Clone, PartialEq)]
pub struct StressYear {
    rows: Vec<Vec<StressDay>>,
}

impl StressYear {
    /// An empty year for `racks` racks.
    pub fn new(racks: usize) -> Self {
        Self {
            rows: vec![Vec::new(); racks],
        }
    }

    /// The same day for every rack: the preset's year (design D3).
    pub fn repeated(racks: usize, day: StressDay) -> Self {
        Self {
            rows: vec![vec![day]; racks],
        }
    }

    /// Collect the days that closed since the last call. A rack's closed
    /// count is its `age.days` while it ticks; a day is taken once.
    pub fn observe(&mut self, state: &SiteState) {
        for (rows, rack) in self.rows.iter_mut().zip(state.racks()) {
            let closed = rack.age.days.round();
            if closed > rows.len() as f64 + 0.5 {
                if let Some(day) = rack.age.last_day {
                    rows.push(day);
                }
            }
        }
    }

    /// The closed days of rack `idx`, empty for a rack that never closed
    /// one.
    pub fn rows(&self, idx: usize) -> &[StressDay] {
        self.rows.get(idx).map_or(&[], Vec::as_slice)
    }

    /// How many racks the year covers.
    pub fn racks(&self) -> usize {
        self.rows.len()
    }

    /// The shortest row: how many days every rack has.
    pub fn days(&self) -> usize {
        self.rows.iter().map(Vec::len).min().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::aging::{RackAge, StressDay};
    use crate::PlantConfig;

    #[test]
    fn a_closed_day_is_taken_once() {
        let cfg = PlantConfig::gw01();
        let mut state = SiteState::new(&cfg, 7, 1_767_225_600);
        let mut year = StressYear::new(cfg.total_racks());
        year.observe(&state);
        assert_eq!(year.days(), 0);
        let day = StressDay::gw01_typical();
        for rack in state
            .blocks
            .iter_mut()
            .flat_map(|b| b.containers.iter_mut())
            .flat_map(|c| c.racks.iter_mut())
        {
            rack.age = RackAge {
                days: 1.0,
                last_day: Some(day),
                ..rack.age
            };
        }
        year.observe(&state);
        year.observe(&state);
        assert_eq!(year.days(), 1);
        assert_eq!(year.rows(3), &[day]);
        assert_eq!(year.racks(), cfg.total_racks());
        assert_eq!(year.rows(10_000), &[]);
    }
}
