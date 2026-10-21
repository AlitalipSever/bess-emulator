//! Compiles Energy-Charts frequency responses into the replay artifact a
//! run loads with `--frequency`.
//!
//! Usage:
//! `cargo run -p bess-data --bin compile-frequency -- <raw-dir> <out-file>`
//! where `<raw-dir>` holds the `frequency_<YYYY-MM-DD>.json` responses
//! `scripts/fetch-frequency.sh` downloads (three days each). Prints a
//! report and the FNV-1a hash of the samples, which a replayed run's
//! record names.
//!
//! The artifact is not bundled and no hash is pinned in code: the
//! source's terms are the user's to accept (DATA-LICENSES.md).
//!
//! Gap policy: a missing second is filled linearly between its
//! neighbours up to `MAX_GAP_S`; a longer gap or a gap at either end stops
//! the compile, since a frequency controller tested against invented
//! minutes would be tested against nothing. The filled count rides in the
//! header. A sample outside 47.0 to 53.0 Hz is refused as a parse error.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bess_data::calendar::days_from_civil;
use bess_data::fnv1a_64;
use bess_data::frequency::codec::{encode, encode_samples, ArtifactData};

/// Calendar year compiled.
const YEAR: i64 = 2024;
/// Seconds in the (leap) year.
const SECONDS: usize = 366 * 86_400;
/// Longest gap filled by interpolation, s.
const MAX_GAP_S: usize = 60;
const PREFIX: &str = "frequency_";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(raw), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: compile-frequency <raw-dir> <out-file>");
        return ExitCode::FAILURE;
    };
    match run(&PathBuf::from(raw), &PathBuf::from(out)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compile-frequency: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(raw: &Path, out: &Path) -> Result<(), String> {
    let start_unix_s = days_from_civil(YEAR, 1, 1) * 86_400;
    let mut samples: Vec<Option<i16>> = vec![None; SECONDS];
    let mut files = 0usize;
    let mut entries = fs::read_dir(raw)
        .map_err(|e| format!("cannot read {}: {e}", raw.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|name| {
                name.starts_with(PREFIX)
                    && Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            })
        })
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for (unix_s, hz) in parse(&text)? {
            let offset = unix_s - start_unix_s;
            if offset < 0 || offset as usize >= SECONDS {
                continue;
            }
            let mhz = ((hz - 50.0) * 1000.0).round();
            if !(-3000.0..=3000.0).contains(&mhz) {
                return Err(format!(
                    "{}: sample {hz} Hz at {unix_s} out of range",
                    path.display()
                ));
            }
            samples[offset as usize] = Some(mhz as i16);
        }
        files += 1;
    }
    if files == 0 {
        return Err(format!("no {PREFIX}*.json under {}", raw.display()));
    }
    let (mhz, filled) = fill(&samples)?;
    let data = ArtifactData {
        year: YEAR as u16,
        start_unix_s,
        filled_s: filled as u32,
        mhz,
    };
    let bytes = encode(&data)?;
    fs::write(out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let hash = fnv1a_64(&encode_samples(&data.mhz));
    let mean = data.mhz.iter().map(|&v| f64::from(v)).sum::<f64>() / SECONDS as f64;
    let var = data
        .mhz
        .iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / SECONDS as f64;
    println!(
        "{files} files, {SECONDS} seconds, {filled} filled, mean {mean:.2} mHz, \
         std {:.2} mHz, samples hash {hash:#018x}, written to {}",
        var.sqrt(),
        out.display()
    );
    Ok(())
}

/// Fill gaps up to `MAX_GAP_S` linearly; refuse longer ones and gaps at
/// the ends.
fn fill(samples: &[Option<i16>]) -> Result<(Vec<i16>, usize), String> {
    let len = samples.len();
    let mut out = Vec::with_capacity(len);
    let mut filled = 0usize;
    let mut at = 0usize;
    while at < len {
        if let Some(value) = samples[at] {
            out.push(value);
            at += 1;
            continue;
        }
        let gap_start = at;
        while at < len && samples[at].is_none() {
            at += 1;
        }
        let gap = at - gap_start;
        if gap_start == 0 || at == len {
            return Err(format!(
                "the year has no sample at an end (a gap of {gap} s)"
            ));
        }
        if gap > MAX_GAP_S {
            return Err(format!(
                "a gap of {gap} s at second {gap_start} exceeds the {MAX_GAP_S} s fill bound"
            ));
        }
        let before = f64::from(out[gap_start - 1]);
        let after = f64::from(samples[at].expect("a sample ends the gap"));
        for step in 1..=gap {
            let value = before + (after - before) * step as f64 / (gap + 1) as f64;
            out.push(value.round() as i16);
        }
        filled += gap;
    }
    Ok((out, filled))
}

/// `(unix seconds, Hz)` pairs from one API response: every
/// `"timestamp":"<ISO 8601 UTC>"` followed by `"frequency":<number>`.
fn parse(text: &str) -> Result<Vec<(i64, f64)>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("\"timestamp\":\"") {
        rest = &rest[at + 13..];
        let end = rest.find('"').ok_or("unterminated timestamp")?;
        let stamp = &rest[..end];
        let unix_s = parse_stamp(stamp)?;
        let f_at = rest
            .find("\"frequency\":")
            .ok_or("timestamp without a frequency")?;
        let num = &rest[f_at + 12..];
        let num_end = num
            .find(|c: char| {
                !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E' || c == '+')
            })
            .unwrap_or(num.len());
        let hz: f64 = num[..num_end]
            .parse()
            .map_err(|_| format!("bad frequency {:?} at {stamp}", &num[..num_end]))?;
        out.push((unix_s, hz));
        rest = &num[num_end..];
    }
    Ok(out)
}

/// `YYYY-MM-DDThh:mm:ss+00:00` or `...Z` to unix seconds.
fn parse_stamp(stamp: &str) -> Result<i64, String> {
    let bad = || format!("bad timestamp {stamp:?}");
    if stamp.len() < 19 {
        return Err(bad());
    }
    let field = |from: usize, to: usize| stamp[from..to].parse::<i64>().map_err(|_| bad());
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    let tail = &stamp[19..];
    if !(tail.is_empty() || tail == "Z" || tail == "+00:00") {
        return Err(format!("timestamp {stamp:?} is not UTC"));
    }
    Ok(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_api_shape() {
        let text = r#"{"data":[{"timestamp":"2024-07-14T00:00:00+00:00","values":{"frequency":49.9847}},{"timestamp":"2024-07-14T00:00:01+00:00","values":{"frequency":49.9822}}]}"#;
        let got = parse(text).expect("parse");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0 + 1, got[1].0);
        assert!((got[1].1 - 49.9822).abs() < 1e-12);
    }

    #[test]
    fn fills_short_gaps_and_refuses_long_ones() {
        let s = [Some(0), None, None, Some(30), Some(1)];
        let (out, filled) = fill(&s).expect("fill");
        assert_eq!(out, vec![0, 10, 20, 30, 1]);
        assert_eq!(filled, 2);
        let mut long = vec![Some(0)];
        long.extend(std::iter::repeat_n(None, MAX_GAP_S + 1));
        long.push(Some(0));
        assert!(fill(&long).is_err());
        assert!(fill(&[None, Some(1)]).is_err());
    }
}
