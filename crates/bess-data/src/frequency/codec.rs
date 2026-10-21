//! Binary layout of the frequency artifact (version 1).
//!
//! Little-endian: magic `BFQ1`, version u16, year u16, start unix seconds
//! i64, sample count u32, filled seconds u32, reserved to a 32-byte header;
//! then `count` samples (i16, millihertz deviation from 50 Hz).

/// Artifact magic bytes.
pub const MAGIC: [u8; 4] = *b"BFQ1";
/// Artifact format version this build reads and writes.
pub const VERSION: u16 = 1;
const HEADER_LEN: usize = 32;

/// The frequency year exactly as stored in the artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactData {
    /// Calendar year of the samples.
    pub year: u16,
    /// Unix seconds UTC of the first sample.
    pub start_unix_s: i64,
    /// Seconds the compiler filled by interpolation.
    pub filled_s: u32,
    /// One-second samples, millihertz from 50 Hz.
    pub mhz: Vec<i16>,
}

/// The samples as bytes, what the year's hash covers.
pub fn encode_samples(mhz: &[i16]) -> Vec<u8> {
    mhz.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Encodes `data` into artifact bytes.
///
/// # Errors
/// Returns an error when the sample count overflows u32.
pub fn encode(data: &ArtifactData) -> Result<Vec<u8>, String> {
    let count = u32::try_from(data.mhz.len()).map_err(|_| "sample count overflows u32")?;
    let mut out = Vec::with_capacity(HEADER_LEN + data.mhz.len() * 2);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&data.year.to_le_bytes());
    out.extend_from_slice(&data.start_unix_s.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&data.filled_s.to_le_bytes());
    out.resize(HEADER_LEN, 0);
    out.extend(encode_samples(&data.mhz));
    Ok(out)
}

/// Decodes artifact bytes.
///
/// # Errors
/// Returns an error on bad magic, unknown version, or a length that does
/// not match the header.
pub fn decode(bytes: &[u8]) -> Result<ArtifactData, String> {
    if bytes.len() < HEADER_LEN {
        return Err("artifact shorter than its header".into());
    }
    if bytes[0..4] != MAGIC {
        return Err("bad artifact magic".into());
    }
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
    let version = u16_at(4);
    if version != VERSION {
        return Err(format!("artifact version {version}, expected {VERSION}"));
    }
    let year = u16_at(6);
    let start_unix_s = i64::from_le_bytes(bytes[8..16].try_into().expect("8 bytes"));
    let count = u32_at(16) as usize;
    let filled_s = u32_at(20);
    let body = &bytes[HEADER_LEN..];
    if body.len() != count * 2 {
        return Err(format!(
            "artifact body is {} bytes, header says {count} samples",
            body.len()
        ));
    }
    let mhz = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect();
    Ok(ArtifactData {
        year,
        start_unix_s,
        filled_s,
        mhz,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = ArtifactData {
            year: 2024,
            start_unix_s: 1_704_067_200,
            filled_s: 7,
            mhz: vec![-32_000, -1, 0, 1, 32_000],
        };
        let bytes = encode(&data).expect("encode");
        assert_eq!(decode(&bytes).expect("decode"), data);
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(decode(&bad).is_err());
        bad = bytes;
        bad.push(0);
        assert!(decode(&bad).is_err());
    }
}
