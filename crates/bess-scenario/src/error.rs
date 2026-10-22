//! Why a scenario was refused, and where.
//!
//! Two kinds of position, because the file has two kinds of mistake. A
//! file that does not parse, or a top-level field with a bad value, points
//! at a line and column. A well-formed event that means nothing on this
//! site names the event by index and field (`events[3].target`), which is
//! unambiguous in a file the reader has open, and adds the line the event
//! starts on. `Reason` has one variant per rejection, so a test asserts the
//! variant and a person reads the text.

use std::fmt;

use crate::schema::duration::DurationError;
use crate::schema::target::TargetError;

/// A refused scenario: where, and why.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{position}: {reason}")]
pub struct LoadError {
    /// Where in the file.
    pub position: Position,
    /// What is wrong there.
    pub reason: Reason,
}

/// Where a rejection points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// A line and column, one-based, as an editor counts them.
    Line {
        /// Line, from 1.
        line: u64,
        /// Column, from 1.
        column: u64,
    },
    /// One field of one event.
    Event {
        /// Index into `events`, from 0.
        index: usize,
        /// The field at fault.
        field: &'static str,
        /// The line the event starts on.
        line: u64,
    },
    /// The document as a whole, when the parser gives no location.
    Document,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Line { line, column } => write!(f, "line {line}, column {column}"),
            Self::Event { index, field, line } => {
                write!(f, "events[{index}].{field} (line {line})")
            }
            Self::Document => f.write_str("scenario"),
        }
    }
}

/// What is wrong.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Reason {
    /// The parser refused the text: bad YAML, a wrong type, an unknown or
    /// duplicate field. Its own message, without the position it repeats.
    #[error("{0}")]
    Syntax(String),
    /// `schema` is not a version this crate reads.
    #[error("schema {0} is not supported; this engine reads schema 1")]
    UnsupportedSchema(u32),
    /// `name` breaks its rule.
    #[error("name {0:?} must be 1 to 64 characters of a-z, 0-9 and '-'")]
    BadName(String),
    /// `start` is not a UTC timestamp in whole seconds.
    #[error("start {value:?}: {why}")]
    BadStart {
        /// What the file says.
        value: String,
        /// Which rule it breaks.
        why: &'static str,
    },
    /// `length` is PT0S.
    #[error("length must be at least PT1S")]
    ZeroLength,
    /// An event is at or past the file's `length`, so it would never fire.
    #[error("this offset is at or past the file's length of {length_s} s, so the event would never fire")]
    EventAfterLength {
        /// The file's length, seconds.
        length_s: u64,
    },
    /// `speed` is outside the shell's clamp.
    #[error("speed {0} is outside 1 to 3600")]
    SpeedOutOfRange(f64),
    /// `age_years` outside 0 to 20.
    #[error("age_years {0} is outside 0 to 20")]
    AgeOutOfRange(u32),
    /// An event names both a target and a surface.
    #[error("an event has a `target` (a physical fault) or a `surface` (a data fault), not both")]
    BothTargetAndSurface,
    /// An event names neither.
    #[error("an event needs a `target` (a physical fault) or a `surface` (a data fault)")]
    NeitherTargetNorSurface,
    /// `at` or `duration` is not a duration this grammar reads.
    #[error(transparent)]
    BadDuration(#[from] DurationError),
    /// `target` does not resolve against this site.
    #[error(transparent)]
    BadTarget(#[from] TargetError),
    /// `points` does not select anything the surfaces publish.
    #[error("points {0}")]
    BadPoints(TargetError),
    /// A backfill burst has nothing to replay: no dropout on its surface
    /// has ended by its offset.
    #[error("a backfill burst replays what a dropout buffered; it needs a `dropout` on the same surface that has ended by this offset")]
    BackfillWithoutDropout,
    /// The fault does not apply to the target's kind.
    #[error("{kind} takes {allowed}, not {fault:?}")]
    FaultNotForTarget {
        /// The fault as written.
        fault: String,
        /// The target's kind.
        kind: &'static str,
        /// The faults that kind takes.
        allowed: &'static str,
    },
    /// No data fault has this name.
    #[error("no data fault is called {0:?}; the set is dropout, freeze, unit_error, chatter, timestamp_drift, nan_burst, backfill_burst")]
    UnknownDataFault(String),
    /// The data fault does not exist on this surface.
    #[error("{fault} does not apply to {surface}: {why}")]
    FaultNotForSurface {
        /// The fault.
        fault: &'static str,
        /// The surface.
        surface: &'static str,
        /// Why the surface cannot carry it.
        why: &'static str,
    },
    /// The fault needs a duration and has none.
    #[error("{0} needs a `duration`")]
    DurationRequired(&'static str),
    /// The fault takes no duration and has one.
    #[error("{0} takes no `duration`")]
    DurationForbidden(&'static str),
    /// The duration is PT0S: a fault that lasts no time at all.
    #[error("{0} takes a `duration` of at least one second, not PT0S")]
    ZeroDuration(&'static str),
    /// A parameter the fault needs is missing.
    #[error("{fault} needs `{parameter}`")]
    ParameterMissing {
        /// The fault.
        fault: &'static str,
        /// The missing parameter.
        parameter: &'static str,
    },
    /// A parameter is given to a fault that does not take it.
    #[error("{fault} does not take `{parameter}`")]
    ParameterNotTaken {
        /// The fault.
        fault: &'static str,
        /// The parameter given.
        parameter: &'static str,
    },
    /// A parameter is outside its range.
    #[error("`{parameter}` {value} is outside {range}")]
    ParameterOutOfRange {
        /// The parameter.
        parameter: &'static str,
        /// What the file says (for a range, the end at fault).
        value: f64,
        /// The range it must be in.
        range: &'static str,
    },
    /// Two events on one target share a tick.
    #[error("events[{other}] acts on the same target at the same offset; which one wins is not the engine's to decide")]
    SharedTick {
        /// The earlier event in the file.
        other: usize,
    },
    /// Events on one target are not in time order in the file.
    #[error(
        "events[{other}] acts on the same target later; list each target's events in time order"
    )]
    OutOfOrder {
        /// The earlier event in the file, which is later in time.
        other: usize,
    },
}
