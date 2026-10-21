//! Compiles a per-second aFRR request series into the replay artifact a
//! run loads with `--afrr`.
//!
//! Usage:
//! `cargo run -p bess-data --bin compile-afrr -- <csv> <demand-mw> <out-file>`
//! where `<csv>` holds `timestamp;mw` lines (UTC ISO 8601 or unix
//! seconds; megawatts, positive to deliver), as `scripts/fetch-afrr.sh`
//! describes how to obtain them, and `<demand-mw>` is the control area's
//! aFRR demand the samples are normalized by (about 2,000 MW in
//! Germany). Gaps up to `MAX_GAP_S` are filled linearly and counted;
//! longer ones stop the compile. The samples are stored per mille of
//! the demand, clamped to ±1000.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bess_data::afrr::codec::{encode, encode_samples, ArtifactData};
use bess_data::calendar::days_from_civil;
use bess_data::fnv1a_64;

const YEAR: i64 = 2024;
const SECONDS: usize = 366 * 86_400;
const MAX_GAP_S: usize = 60;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(csv), Some(demand), Some(out)) = (args.get(1), args.get(2), args.get(3)) else {
        eprintln!("usage: compile-afrr <csv> <demand-mw> <out-file>");
        return ExitCode::FAILURE;
    };
    let Ok(demand_mw) = demand.parse::<u32>() else {
        eprintln!("compile-afrr: demand must be a whole number of MW");
        return ExitCode::FAILURE;
    };
    match run(&PathBuf::from(csv), demand_mw, &PathBuf::from(out)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compile-afrr: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(csv: &Path, demand_mw: u32, out: &Path) -> Result<(), String> {
    let start_unix_s = days_from_civil(YEAR, 1, 1) * 86_400;
    let text = fs::read_to_string(csv).map_err(|e| format!("{}: {e}", csv.display()))?;
    let mut samples: Vec<Option<i16>> = vec![None; SECONDS];
    let mut read = 0usize;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || n == 0 && !line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let (stamp, mw) = line
            .split_once([';', ','])
            .ok_or_else(|| format!("line {}: no separator", n + 1))?;
        let unix_s = parse_stamp(stamp.trim()).map_err(|e| format!("line {}: {e}", n + 1))?;
        let mw: f64 = mw
            .trim()
            .replace(',', ".")
            .parse()
            .map_err(|_| format!("line {}: bad value {mw:?}", n + 1))?;
        let offset = unix_s - start_unix_s;
        if offset < 0 || offset as usize >= SECONDS {
            continue;
        }
        let permille = (mw / f64::from(demand_mw) * 1000.0)
            .round()
            .clamp(-1000.0, 1000.0);
        samples[offset as usize] = Some(permille as i16);
        read += 1;
    }
    if read == 0 {
        return Err("no samples inside the year".into());
    }
    let (permille, filled) = fill(&samples)?;
    let data = ArtifactData {
        year: YEAR as u16,
        start_unix_s,
        demand_mw,
        permille,
    };
    let bytes = encode(&data)?;
    fs::write(out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let mean = data.permille.iter().map(|&v| f64::from(v)).sum::<f64>() / SECONDS as f64;
    println!(
        "{read} samples read, {filled} filled, mean {mean:.1} per mille of {demand_mw} MW, \
         samples hash {:#018x}, written to {}",
        fnv1a_64(&encode_samples(&data.permille)),
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

/// `YYYY-MM-DDThh:mm:ss` (UTC, `Z` or `+00:00` or nothing), a space in
/// place of the `T`, or unix seconds.
fn parse_stamp(stamp: &str) -> Result<i64, String> {
    if let Ok(unix) = stamp.parse::<i64>() {
        return Ok(unix);
    }
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
    fn stamps_in_three_spellings() {
        assert_eq!(parse_stamp("1704067200").expect("unix"), 1_704_067_200);
        assert_eq!(
            parse_stamp("2024-01-01T00:00:00Z").expect("iso"),
            1_704_067_200
        );
        assert_eq!(
            parse_stamp("2024-01-01 00:00:00").expect("space"),
            1_704_067_200
        );
        assert!(parse_stamp("2024-01-01T00:00:00+01:00").is_err());
    }

    #[test]
    fn fills_short_gaps_only() {
        let (out, filled) = fill(&[Some(0), None, Some(10)]).expect("fill");
        assert_eq!(out, vec![0, 5, 10]);
        assert_eq!(filled, 1);
        assert!(fill(&[None, Some(1)]).is_err());
    }
}
