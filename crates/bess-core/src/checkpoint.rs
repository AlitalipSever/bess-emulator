//! Versioned checkpoint format.
//!
//! A checkpoint is the complete state tree in a tagged envelope. Format v1
//! encodes as compact JSON: inspectable, diffable, and with deterministic
//! output (struct field order is fixed, floats print shortest-roundtrip).
//! The envelope leaves room for a binary encoding later: readers dispatch on
//! the tag and version, and unsupported versions fail loudly instead of
//! misparsing.
//!
//! Checkpoints enable "play five years, continue from year five", pre-aged
//! plant presets, and shareable reproductions (checkpoint + scenario + seed).

use serde::{Deserialize, Serialize};

use crate::state::SiteState;

/// Envelope tag identifying a bess checkpoint.
pub const FORMAT_TAG: &str = "bess-checkpoint";
/// Current checkpoint format version.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    kernel_version: String,
    state: SiteState,
}

/// Errors from saving or loading a checkpoint.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// The bytes are not valid JSON or do not match the schema.
    #[error("checkpoint (de)serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
    /// The envelope tag is not `bess-checkpoint`.
    #[error("not a bess checkpoint (format tag {0:?})")]
    WrongFormat(String),
    /// The checkpoint was written by an unsupported format version.
    #[error("unsupported checkpoint version {0} (supported: {FORMAT_VERSION})")]
    UnsupportedVersion(u32),
}

/// Serialize a state tree into checkpoint bytes.
pub fn save(state: &SiteState) -> Result<Vec<u8>, CheckpointError> {
    let envelope = Envelope {
        format: FORMAT_TAG.to_owned(),
        version: FORMAT_VERSION,
        kernel_version: crate::version().to_owned(),
        state: state.clone(),
    };
    Ok(serde_json::to_vec(&envelope)?)
}

/// Restore a state tree from checkpoint bytes.
pub fn load(bytes: &[u8]) -> Result<SiteState, CheckpointError> {
    // Probe the envelope tag and version first, so a foreign or newer file
    // reports what it is instead of a schema mismatch deep inside the state.
    #[derive(Deserialize)]
    struct Probe {
        format: String,
        version: u32,
    }
    let probe: Probe = serde_json::from_slice(bytes)?;
    if probe.format != FORMAT_TAG {
        return Err(CheckpointError::WrongFormat(probe.format));
    }
    if probe.version != FORMAT_VERSION {
        return Err(CheckpointError::UnsupportedVersion(probe.version));
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    Ok(envelope.state)
}

/// Deterministic 64-bit digest of a state tree (FNV-1a over the canonical
/// checkpoint bytes, minus the kernel version so a version bump alone does
/// not change the digest). Used by golden-snapshot tests and by
/// `--assert-snapshot` CI runs.
pub fn state_digest(state: &SiteState) -> u64 {
    // Digest the state serialization directly, not the envelope.
    let bytes = serde_json::to_vec(state).expect("state serialization is infallible");
    fnv1a64(&bytes)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{load, save, state_digest, CheckpointError};
    use crate::config::PlantConfig;
    use crate::state::SiteState;

    #[test]
    fn roundtrip_preserves_state_exactly() {
        let cfg = PlantConfig::gw01();
        let state = SiteState::new(&cfg, 42, 1_767_225_600);
        let bytes = save(&state).unwrap();
        let restored = load(&bytes).unwrap();
        assert_eq!(state, restored);
        assert_eq!(state_digest(&state), state_digest(&restored));
    }

    #[test]
    fn rejects_foreign_format() {
        let err = load(br#"{"format":"other","version":1,"kernel_version":"0","state":null}"#)
            .unwrap_err();
        assert!(matches!(err, CheckpointError::WrongFormat(_)));
    }
}
