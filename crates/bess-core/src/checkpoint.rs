//! Versioned checkpoint format.
//!
//! A checkpoint is the complete state tree in a tagged envelope. The current
//! format encodes as compact JSON: inspectable, diffable, and with deterministic
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
///
/// v10 (M2.5): racks gained the BMS estimator: the reported SoC and the
/// two parameters of the rack current sensor it counts with (offset and
/// gain), so a resumed run keeps the error it had accumulated.
/// v9 (M2.5): racks gained the OCV hysteresis state (`hysteresis_v`), so a
/// resumed rack rests on the branch it was on, not on the mean curve.
/// v8 (M2): run identity gained the scenario (name and content hash) the
/// run was started under, so a resume can refuse a different file, and
/// racks gained the excess self-discharge rate a fault sets. The player's
/// position is not stored: it follows from the file, the seed and the tick.
/// v7 (M2): the substation gained the fiscal revenue meter: running totals,
/// the open quarter-hour period and the last closed one, so a resumed run
/// continues the open period and closes it on the same tick.
/// v6 (M2): blocks gained an operating mode, the plant controller the
/// return sequencer's count and countdown, and the substation a
/// station-service meter.
/// v5 (M2): alarm words on blocks and site, the setpoint-miss timer, the
/// HVAC failure flag, the event log's count and digest, and the events a
/// reset left to go out with the next tick. Latched bits and the hysteresis
/// side a bit sits on are carried in the words, so a resumed run neither
/// re-raises nor silently clears.
/// v4 (M2): racks gained the cell spread (`cell_dsoc`, `cell_dv_v`) and
/// the balancing flag. v3 (M1): the auxiliary inventory added the itemized draw to the site
/// state and the per-item accumulators to the energy meters. v2 (M1):
/// container HVAC gained a staged mode and its anti short-cycle timer.
/// Pre-1.0 checkpoints are not migrated; an older file is rejected with its
/// version named rather than half-restored, since a missing item would
/// otherwise restore as a plant whose auxiliary meters silently read zero.
pub const FORMAT_VERSION: u32 = 10;

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
    crate::digest::fnv1a_64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{load, save, state_digest, CheckpointError, FORMAT_TAG, FORMAT_VERSION};
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

    /// A checkpoint from an older format is refused by version, not parsed
    /// on a best-effort basis. Without the version probe a v1 file would
    /// deserialize into a v2 tree with defaults quietly filling the fields
    /// that were added since, which is the half-restore the envelope exists
    /// to prevent.
    #[test]
    fn rejects_an_older_format_version() {
        let older = format!(
            r#"{{"format":"{FORMAT_TAG}","version":{},"kernel_version":"0","state":null}}"#,
            FORMAT_VERSION - 1
        );
        let err = load(older.as_bytes()).unwrap_err();
        match err {
            CheckpointError::UnsupportedVersion(v) => assert_eq!(v, FORMAT_VERSION - 1),
            other => panic!("expected an unsupported-version error, got {other:?}"),
        }
    }
}
