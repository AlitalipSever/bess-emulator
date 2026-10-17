//! Scenario files for the bess-emulator: a YAML file that describes a run
//! and everything that goes wrong in it, as a reproducible input.
//!
//! `load` turns bytes into a validated `Scenario`; `Scenario::resolve`
//! draws its ranges and fixes a `Resolved` schedule for one run; a
//! `Player` hands the shell what is due before each tick. Physical faults
//! resolve to `bess_core::kernel::FaultAction` and execute in the kernel
//! through `Simulation::apply`; data faults are scheduled here and executed
//! by the shells from phase 5. The crate never steps a simulation and does
//! no IO beyond the bytes it is handed: the same purity as the kernel.
//! [`library`] carries the repository's cases, embedded at build time, so a
//! shell without a file system (the browser) can play them.

use bess_core::digest::fnv1a_64;
use bess_core::PlantConfig;

mod error;
pub mod library;
mod player;
mod resolve;
mod scenario;
pub mod schema;
mod validate;

pub use error::{LoadError, Position, Reason};
pub use player::{Fired, Player, Progress, Refused};
pub use resolve::{ppm_per_day, Due, Resolved, ResumeError, RunParams, Scheduled};
pub use scenario::{DataFault, DataFaultKind, Fault, PendingAction, Scenario, ScenarioEvent};
pub use schema::selector::Selector;
pub use schema::target::Target;
pub use schema::{Rate, Surface, Taxonomy};

/// Parse, validate and hash a scenario file against a site. The hash is
/// FNV-1a 64 over `bytes` as given, so a file is identified by exactly what
/// its author wrote, comments included.
pub fn load(bytes: &[u8], cfg: &PlantConfig) -> Result<Scenario, LoadError> {
    let file: schema::File = serde_saphyr::from_slice(bytes).map_err(|e| syntax(&e))?;
    validate::validate(file, cfg, fnv1a_64(bytes))
}

/// The parser's refusal as a `LoadError`: its location, and its message
/// without the snippet and without the position it already appends.
fn syntax(err: &serde_saphyr::Error) -> LoadError {
    let message = err.without_snippet().to_string();
    match err.location() {
        Some(loc) if loc.line() > 0 => {
            let (line, column) = (loc.line(), loc.column());
            let suffix = format!(" at line {line}, column {column}");
            let text = message.strip_suffix(&suffix).unwrap_or(&message);
            LoadError {
                position: Position::Line { line, column },
                reason: Reason::Syntax(text.to_owned()),
            }
        }
        _ => LoadError {
            position: Position::Document,
            reason: Reason::Syntax(message),
        },
    }
}
