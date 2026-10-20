//! The error model: noise, wander and the S-curve, derived per tick.

use bess_core::config::PlantConfig;
use bess_core::digest::fnv1a_64;

use super::inventory::{full_scale, instrument_of, ClassSpec, Instrument};
use crate::map::Point;

/// Share of the class limit the modeled wander consumes. The rest is noise
/// and headroom, so a reading stays inside its class by construction.
const WANDER_SHARE: f64 = 0.6;
/// Noise sigma as a share of the class limit at the current load.
const NOISE_SHARE: f64 = 0.1;
/// Noise at zero reading, as a share of full scale: a meter's zero is not
/// silent. The compliance gate allows three sigma of it.
pub const EXACT_FLOOR_OF_FULL_SCALE: f64 = 1.0e-4;
/// Load at and above which the rated limit applies.
const RATED_LOAD: f64 = 0.05;
/// Load below which the low-load limit applies unchanged (the meter's
/// starting range; the class makes no promise below it, so the model stops
/// getting worse there).
const LOW_LOAD: f64 = 0.01;

/// Periods of the wander's three components, seconds: days, a fraction of
/// a day, hours. Incommensurate so the sum never repeats within a year.
const WANDER_PERIODS_S: [f64; 3] = [267_840.0, 60_480.0, 19_080.0];
const WANDER_WEIGHTS: [f64; 3] = [0.5, 0.3, 0.2];

/// The class's relative limit at a load (reading over full scale): the
/// rated figure from 5 % up, the low figure at 1 %, log-linear between.
pub fn class_limit(spec: ClassSpec, load: f64) -> f64 {
    if load >= RATED_LOAD {
        spec.rated
    } else if load <= LOW_LOAD {
        spec.low
    } else {
        let t = (load / LOW_LOAD).ln() / (RATED_LOAD / LOW_LOAD).ln();
        spec.low + t * (spec.rated - spec.low)
    }
}

/// How one point is instrumented.
#[derive(Debug, Clone, Copy)]
struct Slot {
    instrument: Instrument,
    full_scale: f64,
    /// A cumulative energy register: a constant gain, never a wander, so
    /// the published register stays monotonic.
    cumulative: bool,
}

/// The inventory applied to a map: per point, its instrument or none.
pub struct Instruments {
    seed: u64,
    on: bool,
    slots: Vec<Option<Slot>>,
}

impl Instruments {
    /// Instrument the points of a map for a run with `seed`.
    pub fn new(points: &[Point], cfg: &PlantConfig, seed: u64, on: bool) -> Self {
        let slots = points
            .iter()
            .map(|p| {
                instrument_of(&p.name).map(|instrument| Slot {
                    instrument,
                    full_scale: full_scale(&p.name, cfg),
                    cumulative: p.name.ends_with("_kwh"),
                })
            })
            .collect();
        Self { seed, on, slots }
    }

    /// The instrument behind point `index`, if any.
    #[cfg(test)]
    pub fn instrument(&self, index: usize) -> Option<Instrument> {
        self.slots[index].map(|s| s.instrument)
    }

    /// The reading of point `index` at `tick` for a true value.
    pub fn measure(&self, index: usize, tick: u64, truth: f64) -> f64 {
        let Some(slot) = (self.on).then_some(self.slots[index]).flatten() else {
            return truth;
        };
        let spec = slot.instrument.class();
        let id = slot.instrument.id();
        if let Some(abs) = spec.absolute {
            let wander = wander(self.seed, id, index, tick);
            let noise = noise(self.seed, id, index, tick, 0);
            return truth + abs * (WANDER_SHARE * wander + NOISE_SHARE * noise);
        }
        if slot.cumulative {
            // One ratio error per register, drawn once: the CT chain's gain.
            let gain = uniform(self.seed, id, index, 0, 1) * 2.0 - 1.0;
            return truth * (1.0 + WANDER_SHARE * spec.rated * gain);
        }
        let load = truth.abs() / slot.full_scale;
        let limit = class_limit(spec, load);
        let wander = wander(self.seed, id, index, tick);
        let reading_noise = noise(self.seed, id, index, tick, 0);
        let zero = noise(self.seed, id, index, tick, 1);
        truth * (1.0 + limit * (WANDER_SHARE * wander + NOISE_SHARE * reading_noise))
            + EXACT_FLOOR_OF_FULL_SCALE * slot.full_scale * zero
    }

    /// The largest error the class allows a reading of point `index`, in
    /// the point's unit: what the compliance gate holds every reading to.
    #[cfg(test)]
    pub fn allowed_error(&self, index: usize, truth: f64) -> Option<f64> {
        let slot = self.slots[index]?;
        let spec = slot.instrument.class();
        if let Some(abs) = spec.absolute {
            return Some(abs);
        }
        if slot.cumulative {
            return Some(spec.rated * truth.abs());
        }
        let load = truth.abs() / slot.full_scale;
        Some(
            class_limit(spec, load) * truth.abs()
                + 3.0 * EXACT_FLOOR_OF_FULL_SCALE * slot.full_scale,
        )
    }
}

/// A uniform draw in [0, 1) from the derivation hash. `lane` separates
/// draws that share every other coordinate.
fn uniform(seed: u64, id: u64, index: usize, tick: u64, lane: u64) -> f64 {
    let mut bytes = [0u8; 40];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..16].copy_from_slice(&id.to_le_bytes());
    bytes[16..24].copy_from_slice(&(index as u64).to_le_bytes());
    bytes[24..32].copy_from_slice(&tick.to_le_bytes());
    bytes[32..].copy_from_slice(&lane.to_le_bytes());
    (fnv1a_64(&bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// Noise with unit variance, bounded in [-3, 3]: the sum of three uniform
/// draws, which is close enough to Gaussian for a spectrum and never
/// unbounded, so a class limit can be a gate.
fn noise(seed: u64, id: u64, index: usize, tick: u64, lane: u64) -> f64 {
    let a = uniform(seed, id, index, tick, 10 + lane * 3);
    let b = uniform(seed, id, index, tick, 11 + lane * 3);
    let c = uniform(seed, id, index, tick, 12 + lane * 3);
    (a + b + c - 1.5) * 2.0
}

/// A bounded slow wander in [-1, 1]: three sinusoids with seeded phases,
/// derived from the tick rather than walked, so a resume reproduces it.
fn wander(seed: u64, id: u64, index: usize, tick: u64) -> f64 {
    let t = tick as f64;
    let mut w = 0.0;
    for (k, (period, weight)) in WANDER_PERIODS_S.iter().zip(WANDER_WEIGHTS).enumerate() {
        let phase = uniform(seed, id, index, 0, 20 + k as u64) * std::f64::consts::TAU;
        w += weight * (std::f64::consts::TAU * t / period + phase).sin();
    }
    w
}
