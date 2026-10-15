//! The one hash the workspace pins things with: FNV-1a, 64 bits.
//!
//! The checkpoint digest, the event log's running digest and a scenario's
//! content hash all use it, so the constants live once. It is not a
//! cryptographic hash and nothing here needs one: it names bytes so that a
//! change to them is visible, which is what a pin is for.

/// FNV-1a 64-bit offset basis: the hash of no bytes.
pub const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

const PRIME: u64 = 0x0000_0100_0000_01b3;

/// Continue a running hash over more bytes. `fold(OFFSET, a)` followed by
/// `fold(_, b)` equals `fnv1a_64` of `a` and `b` concatenated, which is how
/// the event log digests an unbounded stream without storing it.
pub fn fold(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// FNV-1a 64 of a byte string.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    fold(OFFSET, bytes)
}

#[cfg(test)]
mod tests {
    use super::{fnv1a_64, fold, OFFSET};

    /// Published FNV-1a 64 test vectors, so a refactor of the callers
    /// cannot move every pin in the workspace silently.
    #[test]
    fn matches_the_reference_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn folding_in_pieces_equals_hashing_the_whole() {
        assert_eq!(fold(fold(OFFSET, b"foo"), b"bar"), fnv1a_64(b"foobar"));
    }
}
