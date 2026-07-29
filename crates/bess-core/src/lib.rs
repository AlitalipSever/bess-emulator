//! Deterministic simulation kernel for a synthetic grid-scale battery plant.
//!
//! `bess-core` owns the state tree, the layer traits, and the tick loop.
//! The kernel is a pure function of state and inputs: no wall clock, no IO,
//! no threads, randomness only from a seeded PRNG stored inside the state.
//! Given the same seed, configuration, and input series, two runs produce
//! byte-identical state; a golden-snapshot test enforces this.
//!
//! Default model implementations live in `bess-models`. Protocol surfaces
//! and time control (real time, accelerated, as-fast-as-possible) live in
//! the shells; the kernel cannot tell the difference.

pub mod checkpoint;
pub mod config;
pub mod kernel;
pub mod rng;
pub mod state;
pub mod traits;

pub use config::PlantConfig;
pub use kernel::{Inputs, Simulation};
pub use state::SiteState;
pub use traits::Models;

/// Simulation tick length in seconds. Power flow is quasi-static within a
/// tick; publication layers decimate per-signal on top of it.
pub const TICK_SECONDS: u64 = 1;

/// Kernel crate version, embedded in checkpoints and health endpoints.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
