//! Seeded, serializable PRNG for the kernel.
//!
//! xoshiro256++ implemented locally so that the byte-identical-run contract
//! does not depend on an external crate's stream stability across versions.
//! The generator state is serialized inside checkpoints, so a resumed run
//! continues the exact same stream.

use serde::{Deserialize, Serialize};

/// xoshiro256++ generator. All kernel randomness flows through this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Create a generator from a seed, expanding it with SplitMix64 as
    /// recommended by the xoshiro authors (avoids all-zero states).
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = seed;
        let mut s = [0u64; 4];
        for slot in &mut s {
            *slot = splitmix64(&mut sm);
        }
        Self { s }
    }

    /// Next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[0]
            .wrapping_add(self.s[3])
            .rotate_left(23)
            .wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform value in `[0, 1)` built from the upper 53 bits.
    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        (self.next_u64() >> 11) as f64 * SCALE
    }

    /// Uniform value in `[lo, hi)`.
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn uniform_stays_in_range() {
        let mut rng = Rng::from_seed(7);
        for _ in 0..10_000 {
            let v = rng.uniform(-3.0, 5.0);
            assert!((-3.0..5.0).contains(&v));
        }
    }

    #[test]
    fn serde_roundtrip_continues_stream() {
        let mut a = Rng::from_seed(9);
        a.next_u64();
        let json = serde_json::to_string(&a).unwrap();
        let mut b: Rng = serde_json::from_str(&json).unwrap();
        assert_eq!(a.next_u64(), b.next_u64());
    }
}
