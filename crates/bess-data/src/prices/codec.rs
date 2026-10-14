//! Binary layout of the price artifact (version 1).
//!
//! Exists for the `compile-prices` tool and the loader; simulation code
//! should consume [`PriceYear`](super::PriceYear) instead. Layout,
//! little-endian: magic `BPD1`, version u16, year u16, hours u16, days u16,
//! start offset i16 (hours the series starts after the UTC year, +1 for a
//! Europe/Berlin year), reserved to a 32-byte header; then `hours` prices
//! (i32, 0.01 EUR/MWh) and `days + 1` day starts (u16, the hour index each
//! local day begins at, the last entry the end sentinel `hours`).
//!
//! Not the weather container widened: the two artifacts cover different
//! years (UTC versus local), follow different gap policies and carry
//! different licences, and a shared format would have to explain all three.

/// Artifact magic bytes.
pub const MAGIC: [u8; 4] = *b"BPD1";
/// Artifact format version this build reads and writes.
pub const VERSION: u16 = 1;
const HEADER_LEN: usize = 32;

/// The price year exactly as stored in the artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactData {
    /// Calendar year of the local days.
    pub year: u16,
    /// Hours the first entry starts after 00:00 UTC on January 1st.
    pub start_offset_h: i16,
    /// Hourly price, 0.01 EUR/MWh.
    pub price_ct_mwh: Vec<i32>,
    /// Hour index each local day starts at, plus the end sentinel.
    pub day_starts: Vec<u16>,
}

/// Encodes `data` into artifact bytes.
///
/// # Errors
/// Returns an error when the day table does not describe the price series
/// (see [`check_day_starts`]) or a count overflows u16.
pub fn encode(data: &ArtifactData) -> Result<Vec<u8>, String> {
    let hours = data.price_ct_mwh.len();
    check_day_starts(&data.day_starts, hours)?;
    let hours_u16 = u16::try_from(hours).map_err(|_| format!("{hours} hours overflow u16"))?;
    let days_u16 = u16::try_from(data.day_starts.len() - 1).map_err(|_| "days overflow u16")?;

    let mut out = Vec::with_capacity(HEADER_LEN + hours * 4 + data.day_starts.len() * 2);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&data.year.to_le_bytes());
    out.extend_from_slice(&hours_u16.to_le_bytes());
    out.extend_from_slice(&days_u16.to_le_bytes());
    out.extend_from_slice(&data.start_offset_h.to_le_bytes());
    out.resize(HEADER_LEN, 0);
    for v in &data.price_ct_mwh {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in &data.day_starts {
        out.extend_from_slice(&v.to_le_bytes());
    }
    Ok(out)
}

/// Decodes artifact bytes.
///
/// # Errors
/// Returns an error on bad magic, unknown version, a length that does not
/// match the header, or a day table that does not describe the series.
pub fn decode(bytes: &[u8]) -> Result<ArtifactData, String> {
    if bytes.len() < HEADER_LEN {
        return Err("artifact shorter than its header".into());
    }
    if bytes[0..4] != MAGIC {
        return Err("bad artifact magic".into());
    }
    let read_u16 = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let version = read_u16(4);
    if version != VERSION {
        return Err(format!("artifact version {version}, expected {VERSION}"));
    }
    let year = read_u16(6);
    let hours = usize::from(read_u16(8));
    let days = usize::from(read_u16(10));
    let start_offset_h = i16::from_le_bytes([bytes[12], bytes[13]]);
    let expected = HEADER_LEN + hours * 4 + (days + 1) * 2;
    if bytes.len() != expected {
        return Err(format!(
            "artifact is {} bytes, layout expects {expected}",
            bytes.len()
        ));
    }

    let prices_end = HEADER_LEN + hours * 4;
    let price_ct_mwh = bytes[HEADER_LEN..prices_end]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| i32::from_le_bytes(*c))
        .collect();
    let day_starts: Vec<u16> = bytes[prices_end..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    check_day_starts(&day_starts, hours)?;
    Ok(ArtifactData {
        year,
        start_offset_h,
        price_ct_mwh,
        day_starts,
    })
}

/// A day table describes `hours` when it starts at 0, ends at `hours`, and
/// every day is 23, 24 or 25 hours long (the only lengths a whole-hour DST
/// shift can make).
///
/// # Errors
/// Returns an error naming the first entry that breaks the rule.
pub fn check_day_starts(day_starts: &[u16], hours: usize) -> Result<(), String> {
    let (Some(&first), Some(&last)) = (day_starts.first(), day_starts.last()) else {
        return Err("day table is empty".into());
    };
    if first != 0 || usize::from(last) != hours {
        return Err(format!(
            "day table spans {first}..{last}, series has {hours} hours"
        ));
    }
    for (d, pair) in day_starts.windows(2).enumerate() {
        let len = pair[1].saturating_sub(pair[0]);
        if !(23..=25).contains(&len) {
            return Err(format!("local day {d} is {len} hours long"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, ArtifactData};

    fn two_days() -> ArtifactData {
        ArtifactData {
            year: 2024,
            start_offset_h: 1,
            price_ct_mwh: (0..47).map(|h| (h - 10) * 1_234).collect(),
            day_starts: vec![0, 23, 47],
        }
    }

    #[test]
    fn encode_decode_round_trips() {
        let data = two_days();
        let bytes = encode(&data).expect("encode");
        assert_eq!(decode(&bytes).expect("decode"), data);
    }

    #[test]
    fn decode_rejects_corruption() {
        let bytes = encode(&two_days()).expect("encode");
        let mut short = bytes.clone();
        short.truncate(short.len() - 1);
        assert!(decode(&short).is_err());
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(decode(&bad_magic).is_err());
        let mut bad_table = bytes;
        let last = bad_table.len() - 2;
        bad_table[last] = 46; // the sentinel no longer meets the series
        assert!(decode(&bad_table).is_err());
    }

    #[test]
    fn day_tables_outside_the_dst_lengths_are_refused() {
        let mut data = two_days();
        data.day_starts = vec![0, 22, 47];
        assert!(encode(&data).is_err());
    }
}
