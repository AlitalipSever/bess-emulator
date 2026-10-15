//! A validated scenario: every event typed and checked against the site,
//! nothing drawn yet.
//!
//! What `load` returns. The file's identity, the run parameters it states
//! (each optional: the shell's precedence rule decides what applies), and
//! its events in file order, each a physical action waiting only for its
//! draw or a data fault waiting for phase 5's tap.

use bess_core::kernel::FaultAction;
use bess_core::state::ScenarioId;
use serde::Serialize;

use crate::schema::target::Target;
use crate::schema::{Rate, Surface, Taxonomy};

/// A scenario file that passed every rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    /// Name and content hash, as the checkpoint and `/health` carry them.
    pub id: ScenarioId,
    /// The failure category, when the file states one.
    pub taxonomy: Option<Taxonomy>,
    /// The seed the file states, if any.
    pub seed: Option<u64>,
    /// The start the file states, unix seconds UTC, if any.
    pub start_unix_s: Option<i64>,
    /// The speed the file states, if any.
    pub speed: Option<f64>,
    /// The events, in file order.
    pub events: Vec<ScenarioEvent>,
}

/// One validated event.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioEvent {
    /// Position in the file's `events`, from 0.
    pub index: usize,
    /// Offset from the scenario's start, seconds.
    pub at_s: u64,
    /// What happens.
    pub fault: Fault,
}

/// A physical fault on a node, or a data fault on a surface.
#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    /// Executes in the kernel through `Simulation::apply`.
    Physical {
        /// The node, for ordering and messages.
        target: Target,
        /// The action, once any range is drawn.
        action: PendingAction,
    },
    /// Executes in the shells, from phase 5.
    Data(DataFault),
}

/// A physical action before resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PendingAction {
    /// Fully determined by the file.
    Ready(FaultAction),
    /// A leak whose rate may be a range, drawn at resolution.
    SelfDischarge {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
        /// Excess self-discharge, % SoC per month.
        rate_pct_per_month: Rate,
    },
}

/// A data fault, its parameters checked for presence and range and their
/// defaults applied. Phase 5 gives them meaning.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DataFault {
    /// The surface it corrupts.
    pub surface: Surface,
    /// Which fault.
    pub fault: DataFaultKind,
    /// How long, seconds; `None` where the fault takes none or runs to the
    /// end of the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<u64>,
    /// The point prefix (`*` for the whole surface), where the fault takes
    /// one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<String>,
    /// `unit_error`'s power of ten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exponent: Option<i32>,
    /// `chatter`'s flaps per minute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_per_min: Option<f64>,
    /// `timestamp_drift`'s seconds of device-clock drift per hour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_s_per_h: Option<f64>,
}

/// The data fault set v1 (design D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFaultKind {
    /// The surface goes silent.
    Dropout,
    /// Points hold their last value.
    Freeze,
    /// Points are off by a power of ten.
    UnitError,
    /// Points flap.
    Chatter,
    /// The device clock drifts.
    TimestampDrift,
    /// Points read NaN.
    NanBurst,
    /// Buffered samples arrive at once.
    BackfillBurst,
}

impl DataFaultKind {
    /// Every kind, in the order the design lists them.
    pub const ALL: [Self; 7] = [
        Self::Dropout,
        Self::Freeze,
        Self::UnitError,
        Self::Chatter,
        Self::TimestampDrift,
        Self::NanBurst,
        Self::BackfillBurst,
    ];

    /// The name as the file spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dropout => "dropout",
            Self::Freeze => "freeze",
            Self::UnitError => "unit_error",
            Self::Chatter => "chatter",
            Self::TimestampDrift => "timestamp_drift",
            Self::NanBurst => "nan_burst",
            Self::BackfillBurst => "backfill_burst",
        }
    }

    /// The kind a name spells, if any.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}
