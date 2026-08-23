//! Moving the plant to another moment in time.
//!
//! Two ways, and the difference between them is what happens to the state.
//! A restart throws it away and builds a fresh plant on the chosen date. A
//! fast-forward keeps it and computes every tick in between, so the plant
//! that arrives has lived through the journey: the same state a run left
//! going would have reached.
//!
//! There is no rewind. The kernel has no inverse, and pretending otherwise
//! would mean either storing every state or lying about continuity. Going
//! back means restarting there, and the panel says so.
//!
//! The work is split across frames rather than blocked in one. The browser
//! build is single-threaded, so a blocking catch-up would freeze the canvas
//! and show nothing at all for the length of the jump. Advancing a budget at
//! a time keeps the scene drawing, which also happens to be the better demo:
//! a month of weather goes past in a few seconds of watching.

use bess_core::Simulation;
use bess_models::HistoricalWeather;

/// A jump in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastForward {
    /// Where the plant was when the jump started.
    from_unix_s: i64,
    /// Where it is going.
    target_unix_s: i64,
}

impl FastForward {
    /// Start a jump, or refuse one that does not go forwards.
    pub fn toward(sim: &Simulation, target_unix_s: i64) -> Option<Self> {
        let from_unix_s = sim.unix_time_s();
        (target_unix_s > from_unix_s).then_some(Self {
            from_unix_s,
            target_unix_s,
        })
    }

    /// Where the jump is going.
    pub fn target_unix_s(self) -> i64 {
        self.target_unix_s
    }

    /// How far along, 0 to 1.
    pub fn progress(self, sim: &Simulation) -> f32 {
        let total = (self.target_unix_s - self.from_unix_s).max(1);
        let done = (sim.unix_time_s() - self.from_unix_s).clamp(0, total);
        done as f32 / total as f32
    }

    /// Advance up to `budget_ticks` toward the target. Returns true when the
    /// plant has arrived and the jump is over.
    ///
    /// The budget is in ticks rather than wall time because the browser has
    /// no monotonic clock without a shim, and a tick count is the same
    /// quantity on both targets anyway.
    pub fn advance(
        self,
        sim: &mut Simulation,
        weather: &HistoricalWeather,
        budget_ticks: u64,
    ) -> bool {
        for _ in 0..budget_ticks {
            if sim.unix_time_s() >= self.target_unix_s {
                return true;
            }
            let inputs = weather.inputs_at(sim.unix_time_s());
            sim.step(&inputs);
        }
        sim.unix_time_s() >= self.target_unix_s
    }
}

#[cfg(test)]
mod tests {
    use super::FastForward;
    use bess_core::{PlantConfig, Simulation};
    use bess_models::{gw01_models, gw01_weather};

    /// 2026-01-01 00:00:00 UTC.
    const NEW_YEAR: i64 = 1_767_225_600;

    fn plant() -> Simulation {
        let cfg = PlantConfig::gw01();
        let models = gw01_models(&cfg);
        Simulation::new(cfg, models, 7, NEW_YEAR)
    }

    #[test]
    fn a_jump_arrives_where_a_plain_run_would_have() {
        // The whole claim of fast-forward: it is the same run, not a
        // shortcut through it. Six hours is long enough for the thermal
        // state to have a history and short enough to run twice in a test.
        let target = NEW_YEAR + 6 * 3600;
        let weather = gw01_weather();

        let mut plain = plant();
        while plain.unix_time_s() < target {
            let inputs = weather.inputs_at(plain.unix_time_s());
            plain.step(&inputs);
        }

        let mut jumped = plant();
        let jump = FastForward::toward(&jumped, target).expect("a forward jump");
        // Deliberately awkward budgets, so the split across frames cannot be
        // what makes it come out right.
        let mut frames = 0;
        while !jump.advance(&mut jumped, &weather, 997) {
            frames += 1;
            assert!(frames < 1_000, "the jump never arrived");
        }

        assert_eq!(jumped.unix_time_s(), plain.unix_time_s());
        assert_eq!(jumped.state(), plain.state());
    }

    #[test]
    fn a_jump_reports_where_it_has_got_to() {
        let target = NEW_YEAR + 4 * 3600;
        let weather = gw01_weather();
        let mut sim = plant();
        let jump = FastForward::toward(&sim, target).expect("a forward jump");
        assert!(jump.progress(&sim).abs() < 1e-6);

        jump.advance(&mut sim, &weather, 7_200);
        let halfway = jump.progress(&sim);
        assert!(
            (halfway - 0.5).abs() < 0.01,
            "half a jump reported {halfway:.3}"
        );

        while !jump.advance(&mut sim, &weather, 10_000) {}
        assert!((jump.progress(&sim) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_jump_that_does_not_go_forwards_is_refused() {
        let sim = plant();
        assert!(FastForward::toward(&sim, NEW_YEAR).is_none());
        assert!(FastForward::toward(&sim, NEW_YEAR - 86_400).is_none());
        assert!(FastForward::toward(&sim, NEW_YEAR + 1).is_some());
    }

    #[test]
    fn a_jump_never_overshoots_its_target() {
        // A budget far larger than the distance must stop on arrival rather
        // than spend itself, or a short hop would run the plant for hours.
        let target = NEW_YEAR + 120;
        let weather = gw01_weather();
        let mut sim = plant();
        let jump = FastForward::toward(&sim, target).expect("a forward jump");
        assert!(jump.advance(&mut sim, &weather, 500_000));
        assert_eq!(sim.unix_time_s(), target);
    }
}
