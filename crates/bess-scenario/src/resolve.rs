//! From a scenario and run parameters to a fixed schedule (design D12),
//! and from a schedule to a simulation that carries its identity (D8).
//!
//! Every range is drawn here, once, from a stream of its own seeded by the
//! run's seed and the event's index, so the schedule is fully determined
//! before the first tick and a resume can rebuild it without drawing
//! anything twice. Which seed, start and speed apply is the shell's rule
//! (D5), because it is about flags; this module takes the answer.

use bess_core::kernel::FaultAction;
use bess_core::rng::Rng;
use bess_core::state::ScenarioId;
use bess_core::{Models, PlantConfig, Simulation, SiteState, TICK_SECONDS};

use crate::scenario::{DataFault, Fault, PendingAction, Scenario};
use crate::schema::Rate;

/// Salt that separates scenario draws from the kernel's streams.
const SCENARIO_STREAM: u64 = 0x5ce7_a210_d4a3_0b1e;

/// Days in the month a `%/month` rate is quoted over.
const MONTH_DAYS: f64 = 30.0;

/// The parameters a run uses, once the shell has applied its precedence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunParams {
    /// PRNG seed.
    pub seed: u64,
    /// Unix seconds UTC of tick 0.
    pub start_unix_s: i64,
    /// Simulated seconds per wall second.
    pub speed: f64,
}

/// A scenario fixed for one run: identity, parameters and the schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// Name and content hash.
    pub id: ScenarioId,
    /// What the run uses.
    pub params: RunParams,
    /// Every event, sorted by (tick, file index).
    pub schedule: Vec<Scheduled>,
}

/// One event at its tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheduled {
    /// Position in the file's `events`.
    pub index: usize,
    /// Offset from the start, seconds, as the file gave it.
    pub at_s: u64,
    /// The tick it fires before.
    pub tick: u64,
    /// What fires.
    pub due: Due,
}

/// What a scheduled event does when its tick comes.
#[derive(Debug, Clone, PartialEq)]
pub enum Due {
    /// Hand to `Simulation::apply`.
    Physical(FaultAction),
    /// Scheduled only, until phase 5's tap.
    Data(DataFault),
}

/// Why a checkpoint cannot resume under this scenario.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResumeError {
    /// The tree was started without a scenario.
    #[error("the checkpoint was taken without a scenario; it cannot resume under {0:?}")]
    NoScenario(String),
    /// The tree was started under another file.
    #[error("the checkpoint was taken under {found_name:?} ({found:#018x}); this is {name:?} ({expected:#018x})")]
    OtherScenario {
        /// This scenario's name.
        name: String,
        /// This scenario's hash.
        expected: u64,
        /// The tree's scenario name.
        found_name: String,
        /// The tree's scenario hash.
        found: u64,
    },
    /// The tree's seed is not the seed this schedule was drawn with.
    #[error("the checkpoint's seed is {found}; this schedule was drawn with seed {expected}")]
    OtherSeed {
        /// The seed of this resolution.
        expected: u64,
        /// The tree's seed.
        found: u64,
    },
}

impl Scenario {
    /// Draw every range and fix the schedule for one run.
    pub fn resolve(self, params: RunParams) -> Resolved {
        let mut schedule: Vec<Scheduled> = self
            .events
            .into_iter()
            .map(|e| {
                let due = match e.fault {
                    Fault::Physical { action, .. } => {
                        Due::Physical(draw(action, params.seed, e.index))
                    }
                    Fault::Data(d) => Due::Data(d),
                };
                Scheduled {
                    index: e.index,
                    at_s: e.at_s,
                    tick: e.at_s / TICK_SECONDS,
                    due,
                }
            })
            .collect();
        schedule.sort_by_key(|s| (s.tick, s.index));
        Resolved {
            id: self.id,
            params,
            schedule,
        }
    }
}

fn draw(action: PendingAction, seed: u64, index: usize) -> FaultAction {
    match action {
        PendingAction::Ready(a) => a,
        PendingAction::SelfDischarge {
            block,
            container,
            rack,
            rate_pct_per_month,
        } => {
            let pct = match rate_pct_per_month {
                Rate::Fixed(v) => v,
                Rate::Range(lo, hi) => {
                    Rng::from_seed(seed ^ SCENARIO_STREAM ^ index as u64).uniform(lo, hi)
                }
            };
            FaultAction::RackSelfDischarge {
                block,
                container,
                rack,
                excess_ppm_per_day: ppm_per_day(pct),
            }
        }
    }
}

/// `%/month` to ppm of SoC per day, the unit the fault carries: 1 %/month
/// is 333.
pub fn ppm_per_day(pct_per_month: f64) -> u32 {
    (pct_per_month * 1.0e4 / MONTH_DAYS).round() as u32
}

impl Resolved {
    /// A fresh simulation at the run's seed and start, its tree naming this
    /// scenario.
    pub fn simulation(&self, cfg: PlantConfig, models: Models) -> Simulation {
        let mut state = SiteState::new(&cfg, self.params.seed, self.params.start_unix_s);
        state.meta.scenario = Some(self.id.clone());
        Simulation::from_state(cfg, models, state)
    }

    /// Resume a checkpointed tree under this scenario, refusing one taken
    /// under another file, under none, or under another seed. The player's
    /// position is not in the tree; `Player::catch_up` rebuilds it.
    pub fn resume(
        &self,
        cfg: PlantConfig,
        models: Models,
        state: SiteState,
    ) -> Result<Simulation, ResumeError> {
        match &state.meta.scenario {
            None => return Err(ResumeError::NoScenario(self.id.name.clone())),
            Some(found) if found.content_hash != self.id.content_hash => {
                return Err(ResumeError::OtherScenario {
                    name: self.id.name.clone(),
                    expected: self.id.content_hash,
                    found_name: found.name.clone(),
                    found: found.content_hash,
                })
            }
            Some(_) => {}
        }
        if state.meta.seed != self.params.seed {
            return Err(ResumeError::OtherSeed {
                expected: self.params.seed,
                found: state.meta.seed,
            });
        }
        Ok(Simulation::from_state(cfg, models, state))
    }
}
