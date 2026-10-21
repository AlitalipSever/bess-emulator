//! Binary layout of the balancing artifact (version 1).
//!
//! Little-endian: magic `BBL1`, version u16, year u16, days u16, start
//! offset i16, quarters u32, reserved to a 32-byte header; then, each as
//! i32 in 0.01 EUR: `days × 6` FCR block prices, `days × 6` aFRR positive
//! and `days × 6` aFRR negative capacity prices, then `quarters` values
//! each of the energy merit order's positive minimum, positive average,
//! negative minimum and negative average; then `days + 1` quarter starts
//! (u32, the quarter index each local day begins at, the last the end
//! sentinel).

/// Artifact magic bytes.
pub const MAGIC: [u8; 4] = *b"BBL1";
/// Artifact format version this build reads and writes.
pub const VERSION: u16 = 1;
const HEADER_LEN: usize = 32;

/// The balancing year exactly as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactData {
    /// Calendar year of the local days.
    pub year: u16,
    /// Hours the local year starts after 00:00 UTC on January 1st.
    pub start_offset_h: i16,
    /// FCR settlement capacity price per day and block, 0.01 EUR/MW.
    pub fcr_ct: Vec<i32>,
    /// aFRR positive average capacity price per day and block, 0.01
    /// EUR/MW/h.
    pub afrr_pos_ct: Vec<i32>,
    /// aFRR negative, likewise.
    pub afrr_neg_ct: Vec<i32>,
    /// aFRR energy merit order, positive, minimum price per quarter,
    /// 0.01 EUR/MWh.
    pub energy_pos_min_ct: Vec<i32>,
    /// Positive, average.
    pub energy_pos_avg_ct: Vec<i32>,
    /// Negative, minimum.
    pub energy_neg_min_ct: Vec<i32>,
    /// Negative, average.
    pub energy_neg_avg_ct: Vec<i32>,
    /// Quarter index each local day starts at, plus the end sentinel.
    pub quarter_starts: Vec<u32>,
}

/// Encodes `data` into artifact bytes.
///
/// # Errors
/// Returns an error when a count overflows its field.
pub fn encode(data: &ArtifactData) -> Result<Vec<u8>, String> {
    let days = u16::try_from(data.quarter_starts.len().saturating_sub(1))
        .map_err(|_| "days overflow u16")?;
    let quarters = *data.quarter_starts.last().ok_or("an empty day table")?;
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&data.year.to_le_bytes());
    out.extend_from_slice(&days.to_le_bytes());
    out.extend_from_slice(&data.start_offset_h.to_le_bytes());
    out.extend_from_slice(&quarters.to_le_bytes());
    out.resize(HEADER_LEN, 0);
    for series in [
        &data.fcr_ct,
        &data.afrr_pos_ct,
        &data.afrr_neg_ct,
        &data.energy_pos_min_ct,
        &data.energy_pos_avg_ct,
        &data.energy_neg_min_ct,
        &data.energy_neg_avg_ct,
    ] {
        for v in series {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for v in &data.quarter_starts {
        out.extend_from_slice(&v.to_le_bytes());
    }
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
    let version = u16_at(4);
    if version != VERSION {
        return Err(format!("artifact version {version}, expected {VERSION}"));
    }
    let year = u16_at(6);
    let days = usize::from(u16_at(8));
    let start_offset_h = i16::from_le_bytes([bytes[10], bytes[11]]);
    let quarters = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes")) as usize;
    let per_block = days * super::BLOCKS;
    let counts = [
        per_block, per_block, per_block, quarters, quarters, quarters, quarters,
    ];
    let body_len: usize = counts.iter().sum::<usize>() * 4 + (days + 1) * 4;
    if bytes.len() != HEADER_LEN + body_len {
        return Err(format!(
            "artifact is {} bytes, header implies {}",
            bytes.len(),
            HEADER_LEN + body_len
        ));
    }
    let mut at = HEADER_LEN;
    let mut take_i32 = |n: usize| {
        let v: Vec<i32> = bytes[at..at + n * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| i32::from_le_bytes(*c))
            .collect();
        at += n * 4;
        v
    };
    let fcr_ct = take_i32(per_block);
    let afrr_pos_ct = take_i32(per_block);
    let afrr_neg_ct = take_i32(per_block);
    let energy_pos_min_ct = take_i32(quarters);
    let energy_pos_avg_ct = take_i32(quarters);
    let energy_neg_min_ct = take_i32(quarters);
    let energy_neg_avg_ct = take_i32(quarters);
    let quarter_starts = bytes[at..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    Ok(ArtifactData {
        year,
        start_offset_h,
        fcr_ct,
        afrr_pos_ct,
        afrr_neg_ct,
        energy_pos_min_ct,
        energy_pos_avg_ct,
        energy_neg_min_ct,
        energy_neg_avg_ct,
        quarter_starts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = ArtifactData {
            year: 2024,
            start_offset_h: 1,
            fcr_ct: (0..12).collect(),
            afrr_pos_ct: vec![5; 12],
            afrr_neg_ct: vec![-5; 12],
            energy_pos_min_ct: vec![1; 192],
            energy_pos_avg_ct: vec![2; 192],
            energy_neg_min_ct: vec![3; 192],
            energy_neg_avg_ct: vec![4; 192],
            quarter_starts: vec![0, 96, 192],
        };
        let bytes = encode(&data).expect("encode");
        assert_eq!(decode(&bytes).expect("decode"), data);
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(decode(&bad).is_err());
    }
}
