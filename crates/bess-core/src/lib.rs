//! Deterministic simulation kernel for a synthetic grid-scale battery plant.
//!
//! The kernel is a pure function over its state: `step(state, inputs)` advances
//! the plant by one fixed tick. No wall clock, no IO, no threads; all randomness
//! comes from a seeded PRNG. Given the same seed, scenario, and dataset version,
//! two runs produce byte-identical output.

/// Simulation tick length in seconds. All plant dynamics advance in steps of
/// this size; publication layers decimate per-signal on top of it.
pub const TICK_SECONDS: u64 = 1;

/// Crate version, e.g. for shells to report in health endpoints.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_nonempty() {
        assert!(!super::version().is_empty());
    }
}
