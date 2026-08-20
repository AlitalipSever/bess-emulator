//! Replayed exogenous datasets for the GW-01 reference site.
//!
//! From M1 on, weather is a first-class replayed input rather than a
//! synthetic function of the timestamp. This crate is both the data home and
//! the licensing boundary: every series bundled here is recorded in
//! DATA-LICENSES.md with its source and terms before it lands, and sources
//! whose terms forbid redistribution ship as fetch scripts instead.
//!
//! The bundled artifact is a fixed release asset. Its content hash is pinned
//! ([`LINDENBERG_2024_FNV1A`]) and asserted at load, so the determinism
//! contract ("same seed + scenario + dataset = byte-identical output")
//! extends to the dataset. Regenerating the artifact is reproducible:
//! `scripts/fetch-weather.sh` downloads the raw DWD archives and the
//! `compile-weather` binary in this crate compiles them.
//!
//! Series semantics, recorded once here: temperature, humidity, wind and
//! cloud cover are instantaneous observations at their stamped hour. Global
//! irradiance is an interval energy sum which the compiler distributes onto
//! UTC hour buckets by exact interval overlap (DWD stamps solar hours in
//! true solar time). Precipitation is an interval sum assigned to its
//! stamped hour as-is; the up-to-one-hour phase offset is irrelevant to its
//! only consumer (the view layer's scenery) and is revisited if
//! precipitation ever becomes a physics input. In M1 the physics consumers
//! are temperature and irradiance alone; precipitation, wind, cloud cover
//! and humidity are scenery-only inputs for the view layer.

use std::sync::OnceLock;

/// Hours in the 2024 reference year (leap year: 366 days).
pub const HOURS_2024: usize = 8784;

/// Pinned FNV-1a 64 hash of the bundled Lindenberg 2024 artifact.
///
/// [`lindenberg_2024`] refuses to serve data whose bytes do not hash to this
/// value. When the artifact is legitimately regenerated, `compile-weather`
/// prints the new hash and this constant moves with it in the same commit.
pub const LINDENBERG_2024_FNV1A: u64 = 0x8a43_a44f_2f2e_a196;

/// DWD station id of the bundled weather year.
pub const STATION_ID: u16 = 3015;
/// Station name as listed in the DWD station catalogue.
pub const STATION_NAME: &str = "Lindenberg (Mark)";
/// Station latitude, degrees north (DWD station metadata, current record).
pub const STATION_LAT_DEG: f64 = 52.2085;
/// Station longitude, degrees east.
pub const STATION_LON_DEG: f64 = 14.1180;
/// Station altitude, metres above sea level.
pub const STATION_ALT_M: f64 = 97.69;

/// Precipitation form, decoded from the DWD `WRTR` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecipForm {
    /// No precipitation fell in the hour (code 0).
    NoPrecip,
    /// Liquid precipitation only (code 6; code 1 in pre-1979 records).
    Rain,
    /// Solid precipitation only (code 7).
    Snow,
    /// Mixed rain and snow (code 8).
    Mixed,
    /// Precipitation reported but form unknown (code 4, 9, or unexpected).
    Unknown,
}

impl PrecipForm {
    /// Decodes a raw DWD `WRTR` code.
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::NoPrecip,
            1 | 6 => Self::Rain,
            7 => Self::Snow,
            8 => Self::Mixed,
            _ => Self::Unknown,
        }
    }
}

/// All observed quantities for one hour bucket.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HourSample {
    /// 2 m air temperature, degrees Celsius.
    pub temp_c: f32,
    /// Relative humidity, percent.
    pub rel_humidity_pct: f32,
    /// Mean global horizontal irradiance over the hour, W/m2.
    pub ghi_wm2: f32,
    /// Precipitation amount in the hour, millimetres.
    pub precip_mm: f32,
    /// Precipitation form.
    pub precip_form: PrecipForm,
    /// Wind speed, m/s.
    pub wind_ms: f32,
    /// Wind direction, degrees (360 = north; 0 = calm/undetermined).
    pub wind_dir_deg: f32,
    /// Total cloud cover, okta (0 clear .. 8 overcast).
    pub cloud_okta: u8,
}

/// How many hours per series the compiler filled to close observation gaps.
///
/// Fill policies (implemented in `compile-weather`, recorded here for
/// honesty): physics-consumed series are strict, temperature and humidity
/// interpolate linearly across gaps up to 24 h and irradiance requires full
/// coverage; scenery-only series get slack up to 72 h, wind speed
/// interpolating and wind direction plus cloud cover copying the nearest
/// observation; missing precipitation amounts are treated as no
/// precipitation. Longer gaps fail compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FillCounts {
    /// Temperature hours filled.
    pub temp: u16,
    /// Relative humidity hours filled.
    pub rel_humidity: u16,
    /// Irradiance hours filled.
    pub ghi: u16,
    /// Precipitation amount hours filled.
    pub precip_mm: u16,
    /// Precipitation form hours filled.
    pub precip_form: u16,
    /// Wind speed hours filled.
    pub wind_ms: u16,
    /// Wind direction hours filled.
    pub wind_dir: u16,
    /// Cloud cover hours filled.
    pub cloud: u16,
}

/// One compiled weather year, hourly resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherYear {
    year: u16,
    station_id: u16,
    fills: FillCounts,
    temp_c: Vec<f32>,
    rel_humidity_pct: Vec<f32>,
    ghi_wm2: Vec<f32>,
    precip_mm: Vec<f32>,
    precip_form: Vec<PrecipForm>,
    wind_ms: Vec<f32>,
    wind_dir_deg: Vec<f32>,
    cloud_okta: Vec<u8>,
}

impl WeatherYear {
    /// Calendar year the series covers.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// DWD station id the series was observed at.
    pub fn station_id(&self) -> u16 {
        self.station_id
    }

    /// Number of hour buckets (8784 for a leap year).
    pub fn len(&self) -> usize {
        self.temp_c.len()
    }

    /// True when the series holds no hours (never, for a valid artifact).
    pub fn is_empty(&self) -> bool {
        self.temp_c.is_empty()
    }

    /// Gap-fill bookkeeping for every series.
    pub fn fill_counts(&self) -> FillCounts {
        self.fills
    }

    /// All quantities for hour bucket `idx` (0 = Jan 1st, 00:00-01:00 UTC).
    ///
    /// # Panics
    /// Panics when `idx` is out of range; the caller owns time mapping.
    pub fn hour(&self, idx: usize) -> HourSample {
        HourSample {
            temp_c: self.temp_c[idx],
            rel_humidity_pct: self.rel_humidity_pct[idx],
            ghi_wm2: self.ghi_wm2[idx],
            precip_mm: self.precip_mm[idx],
            precip_form: self.precip_form[idx],
            wind_ms: self.wind_ms[idx],
            wind_dir_deg: self.wind_dir_deg[idx],
            cloud_okta: self.cloud_okta[idx],
        }
    }

    /// 2 m air temperature series, degrees Celsius.
    pub fn temp_c(&self) -> &[f32] {
        &self.temp_c
    }

    /// Mean global horizontal irradiance series, W/m2.
    pub fn ghi_wm2(&self) -> &[f32] {
        &self.ghi_wm2
    }

    /// Relative humidity series, percent.
    pub fn rel_humidity_pct(&self) -> &[f32] {
        &self.rel_humidity_pct
    }

    /// Precipitation amount series, millimetres per hour.
    pub fn precip_mm(&self) -> &[f32] {
        &self.precip_mm
    }

    /// Precipitation form series.
    pub fn precip_form(&self) -> &[PrecipForm] {
        &self.precip_form
    }

    /// Wind speed series, m/s.
    pub fn wind_ms(&self) -> &[f32] {
        &self.wind_ms
    }

    /// Wind direction series, degrees.
    pub fn wind_dir_deg(&self) -> &[f32] {
        &self.wind_dir_deg
    }

    /// Total cloud cover series, okta.
    pub fn cloud_okta(&self) -> &[u8] {
        &self.cloud_okta
    }
}

/// The Lindenberg (Mark) 2024 weather year bundled with this release.
///
/// # Panics
/// Panics when the bundled bytes fail to decode or do not hash to
/// [`LINDENBERG_2024_FNV1A`]; either means artifact and pin were not moved
/// together and the build is not trustworthy.
pub fn lindenberg_2024() -> &'static WeatherYear {
    static YEAR: OnceLock<WeatherYear> = OnceLock::new();
    YEAR.get_or_init(|| {
        static RAW: &[u8] = include_bytes!("../data/lindenberg-2024.bin");
        assert_eq!(
            fnv1a_64(RAW),
            LINDENBERG_2024_FNV1A,
            "bundled weather artifact does not match the pinned hash; \
             regenerate with compile-weather and move LINDENBERG_2024_FNV1A \
             in the same commit"
        );
        let data = codec::decode(RAW).expect("bundled weather artifact must decode");
        WeatherYear {
            year: data.year,
            station_id: data.station_id,
            fills: data.fills,
            temp_c: data
                .temp_c_x10
                .iter()
                .map(|&v| f32::from(v) / 10.0)
                .collect(),
            rel_humidity_pct: data
                .rh_pct_x10
                .iter()
                .map(|&v| f32::from(v) / 10.0)
                .collect(),
            ghi_wm2: data
                .ghi_wm2_x10
                .iter()
                .map(|&v| f32::from(v) / 10.0)
                .collect(),
            precip_mm: data
                .precip_mm_x10
                .iter()
                .map(|&v| f32::from(v) / 10.0)
                .collect(),
            precip_form: data
                .precip_form
                .iter()
                .map(|&c| PrecipForm::from_code(c))
                .collect(),
            wind_ms: data
                .wind_ms_x10
                .iter()
                .map(|&v| f32::from(v) / 10.0)
                .collect(),
            wind_dir_deg: data.wind_dir_deg.iter().map(|&v| f32::from(v)).collect(),
            cloud_okta: data.cloud_okta,
        }
    })
}

/// FNV-1a 64 over `bytes`; the artifact pinning hash (integrity, not
/// security). Matches the digest family bess-core uses for state digests.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Binary layout of the weather artifact (version 1).
///
/// Exists for the `compile-weather` tool and the loader; simulation code
/// should consume [`WeatherYear`] instead. Layout, little-endian:
/// magic `BWD1`, version u16, station u16, year u16, hours u16, eight u16
/// fill counts, u32 reserved; then per-series arrays of `hours` entries:
/// temperature (i16, 0.1 C), humidity (i16, 0.1 %), irradiance
/// (i16, 0.1 W/m2), precipitation (i16, 0.1 mm), wind speed (i16, 0.1 m/s),
/// wind direction (i16, deg), precipitation form (u8, raw WRTR code),
/// cloud cover (u8, okta).
pub mod codec {
    use super::FillCounts;

    /// Artifact magic bytes.
    pub const MAGIC: [u8; 4] = *b"BWD1";
    /// Artifact format version this build reads and writes.
    pub const VERSION: u16 = 1;
    const HEADER_LEN: usize = 32;

    /// Quantized series exactly as stored in the artifact.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ArtifactData {
        /// DWD station id.
        pub station_id: u16,
        /// Calendar year.
        pub year: u16,
        /// Gap-fill bookkeeping.
        pub fills: FillCounts,
        /// Temperature, 0.1 degrees Celsius.
        pub temp_c_x10: Vec<i16>,
        /// Relative humidity, 0.1 percent.
        pub rh_pct_x10: Vec<i16>,
        /// Mean global irradiance, 0.1 W/m2.
        pub ghi_wm2_x10: Vec<i16>,
        /// Precipitation, 0.1 mm.
        pub precip_mm_x10: Vec<i16>,
        /// Wind speed, 0.1 m/s.
        pub wind_ms_x10: Vec<i16>,
        /// Wind direction, degrees.
        pub wind_dir_deg: Vec<i16>,
        /// Raw DWD WRTR precipitation form codes.
        pub precip_form: Vec<u8>,
        /// Total cloud cover, okta.
        pub cloud_okta: Vec<u8>,
    }

    /// Encodes `data` into artifact bytes.
    ///
    /// # Errors
    /// Returns an error when the series lengths disagree or overflow u16.
    pub fn encode(data: &ArtifactData) -> Result<Vec<u8>, String> {
        let hours = data.temp_c_x10.len();
        let all_equal = [
            data.rh_pct_x10.len(),
            data.ghi_wm2_x10.len(),
            data.precip_mm_x10.len(),
            data.wind_ms_x10.len(),
            data.wind_dir_deg.len(),
            data.precip_form.len(),
            data.cloud_okta.len(),
        ]
        .iter()
        .all(|&l| l == hours);
        if !all_equal {
            return Err("series lengths disagree".into());
        }
        let hours_u16 = u16::try_from(hours).map_err(|_| format!("{hours} hours overflow u16"))?;

        let mut out = Vec::with_capacity(HEADER_LEN + hours * 14);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&data.station_id.to_le_bytes());
        out.extend_from_slice(&data.year.to_le_bytes());
        out.extend_from_slice(&hours_u16.to_le_bytes());
        for count in fill_array(data.fills) {
            out.extend_from_slice(&count.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        for series in [
            &data.temp_c_x10,
            &data.rh_pct_x10,
            &data.ghi_wm2_x10,
            &data.precip_mm_x10,
            &data.wind_ms_x10,
            &data.wind_dir_deg,
        ] {
            for v in series {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&data.precip_form);
        out.extend_from_slice(&data.cloud_okta);
        Ok(out)
    }

    /// Decodes artifact bytes.
    ///
    /// # Errors
    /// Returns an error on bad magic, unknown version, or truncated data.
    pub fn decode(bytes: &[u8]) -> Result<ArtifactData, String> {
        if bytes.len() < HEADER_LEN {
            return Err("artifact shorter than its header".into());
        }
        if bytes[0..4] != MAGIC {
            return Err("bad artifact magic".into());
        }
        let version = read_u16(bytes, 4);
        if version != VERSION {
            return Err(format!("artifact version {version}, expected {VERSION}"));
        }
        let station_id = read_u16(bytes, 6);
        let year = read_u16(bytes, 8);
        let hours = usize::from(read_u16(bytes, 10));
        let mut counts = [0u16; 8];
        for (i, slot) in counts.iter_mut().enumerate() {
            *slot = read_u16(bytes, 12 + i * 2);
        }
        let expected = HEADER_LEN + hours * 14;
        if bytes.len() != expected {
            return Err(format!(
                "artifact is {} bytes, layout expects {expected}",
                bytes.len()
            ));
        }

        let mut offset = HEADER_LEN;
        let next_i16_series = |offset: &mut usize| -> Vec<i16> {
            let series = bytes[*offset..*offset + hours * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| i16::from_le_bytes(*c))
                .collect();
            *offset += hours * 2;
            series
        };
        let temp_c_x10 = next_i16_series(&mut offset);
        let rh_pct_x10 = next_i16_series(&mut offset);
        let ghi_wm2_x10 = next_i16_series(&mut offset);
        let precip_mm_x10 = next_i16_series(&mut offset);
        let wind_ms_x10 = next_i16_series(&mut offset);
        let wind_dir_deg = next_i16_series(&mut offset);
        let precip_form = bytes[offset..offset + hours].to_vec();
        offset += hours;
        let cloud_okta = bytes[offset..offset + hours].to_vec();

        Ok(ArtifactData {
            station_id,
            year,
            fills: fills_from_array(counts),
            temp_c_x10,
            rh_pct_x10,
            ghi_wm2_x10,
            precip_mm_x10,
            wind_ms_x10,
            wind_dir_deg,
            precip_form,
            cloud_okta,
        })
    }

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn fill_array(fills: FillCounts) -> [u16; 8] {
        [
            fills.temp,
            fills.rel_humidity,
            fills.ghi,
            fills.precip_mm,
            fills.precip_form,
            fills.wind_ms,
            fills.wind_dir,
            fills.cloud,
        ]
    }

    fn fills_from_array(counts: [u16; 8]) -> FillCounts {
        FillCounts {
            temp: counts[0],
            rel_humidity: counts[1],
            ghi: counts[2],
            precip_mm: counts[3],
            precip_form: counts[4],
            wind_ms: counts[5],
            wind_dir: counts[6],
            cloud: counts[7],
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{decode, encode, ArtifactData};
        use crate::FillCounts;

        #[test]
        fn encode_decode_round_trips() {
            let data = ArtifactData {
                station_id: 3015,
                year: 2024,
                fills: FillCounts {
                    temp: 1,
                    rel_humidity: 2,
                    ghi: 3,
                    precip_mm: 4,
                    precip_form: 5,
                    wind_ms: 6,
                    wind_dir: 7,
                    cloud: 8,
                },
                temp_c_x10: vec![-128, 53, 421],
                rh_pct_x10: vec![830, 1000, 0],
                ghi_wm2_x10: vec![0, 8503, 11000],
                precip_mm_x10: vec![0, 17, 250],
                wind_ms_x10: vec![33, 0, 412],
                wind_dir_deg: vec![190, 0, 360],
                precip_form: vec![0, 6, 7],
                cloud_okta: vec![8, 0, 4],
            };
            let bytes = encode(&data).expect("encode");
            assert_eq!(decode(&bytes).expect("decode"), data);
        }

        #[test]
        fn decode_rejects_corruption() {
            let data = ArtifactData {
                station_id: 3015,
                year: 2024,
                fills: FillCounts::default(),
                temp_c_x10: vec![0],
                rh_pct_x10: vec![0],
                ghi_wm2_x10: vec![0],
                precip_mm_x10: vec![0],
                wind_ms_x10: vec![0],
                wind_dir_deg: vec![0],
                precip_form: vec![0],
                cloud_okta: vec![0],
            };
            let mut bytes = encode(&data).expect("encode");
            bytes.truncate(bytes.len() - 1);
            assert!(decode(&bytes).is_err());
            let mut bad_magic = encode(&data).expect("encode");
            bad_magic[0] = b'X';
            assert!(decode(&bad_magic).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fnv1a_64, PrecipForm};

    #[test]
    fn fnv1a_matches_reference_vectors() {
        // Reference vectors from the FNV specification (Fowler/Noll/Vo).
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn wrtr_codes_decode_to_forms() {
        assert_eq!(PrecipForm::from_code(0), PrecipForm::NoPrecip);
        assert_eq!(PrecipForm::from_code(1), PrecipForm::Rain);
        assert_eq!(PrecipForm::from_code(6), PrecipForm::Rain);
        assert_eq!(PrecipForm::from_code(7), PrecipForm::Snow);
        assert_eq!(PrecipForm::from_code(8), PrecipForm::Mixed);
        assert_eq!(PrecipForm::from_code(4), PrecipForm::Unknown);
        assert_eq!(PrecipForm::from_code(9), PrecipForm::Unknown);
    }
}
