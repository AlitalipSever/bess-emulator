//! Compiles SMARD's weekly day-ahead price blocks into the price artifact
//! bundled with `bess-data`.
//!
//! Usage:
//! `cargo run -p bess-data --bin compile-prices -- <raw-dir> <out-file>`
//! where `<raw-dir>` holds the `4169_DE_hour_<ms>.json` blocks fetched by
//! `scripts/fetch-prices.sh`. Prints a sanity report plus the FNV-1a hash
//! to pin in `DAYAHEAD_DE_LU_2024_FNV1A`.
//!
//! Gap policy: none. The series must be exactly the 8784 consecutive hours
//! of the 2024 Europe/Berlin year; a missing, repeated or null hour stops
//! the compile, because prices have no physical continuity that would
//! justify a fill, and a gap in the source is a decision, not a default.
//!
//! The local-day table comes from the block stamps. SMARD starts every
//! block at Monday 00:00 local time, so local midnight falls every 24 hours
//! from a block's start until the next block begins; a block that holds a
//! DST switch is 167 or 169 hours long, and the difference lands on its
//! last day, the Sunday the EU switches on. Nothing here names the two
//! dates; the tests that pin them are the check.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bess_data::calendar::{civil_from_days, days_from_civil};
use bess_data::fnv1a_64;
use bess_data::prices::codec::{encode, ArtifactData};

/// Calendar year of the local days compiled.
const YEAR: i64 = 2024;
/// Europe/Berlin is UTC+1 on both ends of the year (winter time).
const START_OFFSET_H: i16 = 1;
/// The 2024 local year in hours (leap year; 23 + 25 on the DST days).
const HOURS: usize = 8784;
/// SMARD file name prefix: filter 4169 (day-ahead DE-LU), region DE, hourly.
const PREFIX: &str = "4169_DE_hour_";
/// Harmonised single day-ahead coupling price limits in force in 2024.
const PRICE_LIMITS_EUR_MWH: (f64, f64) = (-500.0, 4000.0);
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// One SMARD weekly block: its start stamp and its `[ms, value]` entries.
struct Block {
    start_ms: i64,
    entries: Vec<(i64, Option<f64>)>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(raw), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: compile-prices <raw-dir> <out-file>");
        return ExitCode::FAILURE;
    };
    match run(&PathBuf::from(raw), &PathBuf::from(out)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compile-prices: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(raw_dir: &Path, out_file: &Path) -> Result<(), String> {
    let year_start_ms = local_midnight_ms(YEAR);
    let year_end_ms = local_midnight_ms(YEAR + 1);
    let blocks = read_blocks(raw_dir)?;

    let prices = hourly_series(&blocks, year_start_ms, year_end_ms)?;
    let day_starts = day_table(&blocks, year_start_ms, year_end_ms)?;
    let price_ct_mwh = prices.iter().map(|&v| (v * 100.0).round() as i32).collect();
    let data = ArtifactData {
        year: YEAR as u16,
        start_offset_h: START_OFFSET_H,
        price_ct_mwh,
        day_starts,
    };
    let bytes = encode(&data)?;
    fs::write(out_file, &bytes).map_err(|e| format!("{}: {e}", out_file.display()))?;
    report(&data, &prices, &bytes, out_file);
    Ok(())
}

/// UTC milliseconds of 00:00 Europe/Berlin on January 1st of `year`.
fn local_midnight_ms(year: i64) -> i64 {
    (days_from_civil(year, 1, 1) * 86_400 - i64::from(START_OFFSET_H) * 3600) * 1000
}

fn read_blocks(raw_dir: &Path) -> Result<Vec<Block>, String> {
    let listing = fs::read_dir(raw_dir).map_err(|e| format!("{}: {e}", raw_dir.display()))?;
    let mut blocks = Vec::new();
    for entry in listing {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let Some(stamp) = name
            .strip_prefix(PREFIX)
            .and_then(|s| s.strip_suffix(".json"))
        else {
            continue;
        };
        let start_ms: i64 = stamp
            .parse()
            .map_err(|_| format!("bad block name {name}"))?;
        let text = fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        let entries = parse_series(&text).map_err(|e| format!("{name}: {e}"))?;
        if entries.first().map(|e| e.0) != Some(start_ms) {
            return Err(format!("{name}: first entry is not the block start"));
        }
        blocks.push(Block { start_ms, entries });
    }
    blocks.sort_by_key(|b| b.start_ms);
    if blocks.is_empty() {
        return Err(format!("no {PREFIX}*.json blocks in {}", raw_dir.display()));
    }
    Ok(blocks)
}

/// The `series` array of a SMARD chart-data file: `[[ms, value|null], ...]`.
fn parse_series(text: &str) -> Result<Vec<(i64, Option<f64>)>, String> {
    let at = text.find("\"series\"").ok_or("no series key")?;
    let body = &text[at..];
    let open = body.find('[').ok_or("series is not an array")?;
    let mut rest = &body[open + 1..];
    let mut entries = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            return Ok(entries);
        }
        let inner_end = rest.find(']').ok_or("unterminated entry")?;
        let inner = rest.strip_prefix('[').ok_or("entry is not an array")?;
        let (ms, value) = inner[..inner_end - 1]
            .split_once(',')
            .ok_or("entry without a value")?;
        let ms: i64 = ms.trim().parse().map_err(|_| format!("bad stamp {ms}"))?;
        let value = match value.trim() {
            "null" => None,
            v => Some(v.parse().map_err(|_| format!("bad value {v}"))?),
        };
        entries.push((ms, value));
        rest = rest[inner_end + 1..].trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest);
    }
}

/// The year's hourly prices, strict: every hour present once, none null.
fn hourly_series(blocks: &[Block], start_ms: i64, end_ms: i64) -> Result<Vec<f64>, String> {
    let mut entries: Vec<(i64, Option<f64>)> = blocks
        .iter()
        .flat_map(|b| b.entries.iter().copied())
        .filter(|&(ms, _)| (start_ms..end_ms).contains(&ms))
        .collect();
    entries.sort_by_key(|e| e.0);
    let mut prices = Vec::with_capacity(HOURS);
    for (idx, &(ms, value)) in entries.iter().enumerate() {
        let expected = start_ms + idx as i64 * HOUR_MS;
        if ms != expected {
            let what = if ms < expected { "repeated" } else { "missing" };
            return Err(format!("{what} hour at {}", stamp(expected.min(ms))));
        }
        let value = value.ok_or_else(|| format!("null price at {}", stamp(ms)))?;
        if !(PRICE_LIMITS_EUR_MWH.0..=PRICE_LIMITS_EUR_MWH.1).contains(&value) {
            return Err(format!(
                "{value} EUR/MWh at {} is outside the market limits",
                stamp(ms)
            ));
        }
        prices.push(value);
    }
    if prices.len() != HOURS {
        return Err(format!(
            "{} hours in the local year, expected {HOURS}",
            prices.len()
        ));
    }
    Ok(prices)
}

/// Hour index of every local midnight, plus the end sentinel.
fn day_table(blocks: &[Block], start_ms: i64, end_ms: i64) -> Result<Vec<u16>, String> {
    let starts: Vec<i64> = blocks
        .iter()
        .map(|b| b.start_ms)
        .filter(|&ms| (start_ms..end_ms).contains(&ms))
        .collect();
    if starts.first() != Some(&start_ms) {
        return Err(format!(
            "no block starts at the local year's first hour {}",
            stamp(start_ms)
        ));
    }
    let mut table = Vec::new();
    for (i, &block_ms) in starts.iter().enumerate() {
        let next_ms = starts.get(i + 1).copied().unwrap_or(end_ms);
        // Whole days to the nearest: a switch week is 7 days of 167 or 169
        // hours, and only its last day absorbs the odd hour.
        let days = (next_ms - block_ms + DAY_MS / 2) / DAY_MS;
        for k in 0..days {
            table.push(hour_index(block_ms + k * DAY_MS, start_ms)?);
        }
    }
    table.push(hour_index(end_ms, start_ms)?);
    Ok(table)
}

fn hour_index(ms: i64, start_ms: i64) -> Result<u16, String> {
    if (ms - start_ms) % HOUR_MS != 0 {
        return Err(format!("{} is not on a whole hour", stamp(ms)));
    }
    u16::try_from((ms - start_ms) / HOUR_MS).map_err(|_| format!("{} overflows u16", stamp(ms)))
}

/// `YYYY-MM-DD HH:00 UTC` of a millisecond stamp, for messages.
fn stamp(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(DAY_MS));
    let h = ms.rem_euclid(DAY_MS) / HOUR_MS;
    format!("{y:04}-{m:02}-{d:02} {h:02}:00 UTC")
}

fn report(data: &ArtifactData, prices: &[f64], bytes: &[u8], out_file: &Path) {
    let days = data.day_starts.len() - 1;
    println!(
        "DE-LU day-ahead {}, {} hours, {days} local days",
        data.year,
        prices.len()
    );
    let first_day = days_from_civil(YEAR, 1, 1);
    for (d, pair) in data.day_starts.windows(2).enumerate() {
        let len = pair[1] - pair[0];
        if len != 24 {
            let (y, m, dd) = civil_from_days(first_day + d as i64);
            println!("  local day {d} ({y:04}-{m:02}-{dd:02}): {len} hours");
        }
    }
    let min = prices.iter().copied().fold(f64::INFINITY, f64::min);
    let max = prices.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = prices.iter().sum::<f64>() / prices.len() as f64;
    let negative = prices.iter().filter(|&&p| p < 0.0).count();
    println!(
        "prices: mean {mean:.2} · min {min:.2} · max {max:.2} EUR/MWh · {negative} negative hours"
    );
    println!("artifact: {} bytes -> {}", bytes.len(), out_file.display());
    println!("FNV-1a 64: 0x{:016x}", fnv1a_64(bytes));
}
