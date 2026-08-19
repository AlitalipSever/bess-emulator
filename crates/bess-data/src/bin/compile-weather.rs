//! Compiles raw DWD CDC hourly station observations into the weather
//! artifact bundled with `bess-data`.
//!
//! Usage:
//! `cargo run -p bess-data --bin compile-weather -- <raw-dir> <out-file>`
//! where `<raw-dir>` holds the unpacked archives fetched by
//! `scripts/fetch-weather.sh`. Prints a gap and sanity report plus the
//! FNV-1a hash to pin in `LINDENBERG_2024_FNV1A`.
//!
//! Gap policy (bounded and deterministic): temperature and humidity
//! interpolate linearly across gaps of at most [`MAX_PHYSICS_GAP_H`] hours
//! (edge gaps copy the nearest observation); irradiance requires full
//! interval coverage outright. Scenery-only series get more slack, since
//! their sole consumer is the view layer: wind speed interpolates and wind
//! direction plus cloud cover copy the nearest observation across gaps up
//! to [`MAX_SCENERY_GAP_H`] hours; missing precipitation amounts count as
//! no precipitation. Anything longer fails compilation: a year that ragged
//! should fall back to another station (design decision D1), not be
//! papered over.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bess_data::codec::{encode, ArtifactData};
use bess_data::{fnv1a_64, FillCounts, HOURS_2024, STATION_ID};

/// Longest observation gap a physics-consumed series may bridge.
const MAX_PHYSICS_GAP_H: usize = 24;
/// Longest observation gap a scenery-only series may bridge.
const MAX_SCENERY_GAP_H: usize = 72;
/// DWD missing-value marker.
const MISSING: f64 = -999.0;
/// Year the artifact covers.
const YEAR: u16 = 2024;
/// Cumulative days before each month of the (leap) reference year.
const DAYS_BEFORE_MONTH: [usize; 12] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];

/// Two per-hour observation series parsed from one product file.
type TwoSeries = (Vec<Option<f64>>, Vec<Option<f64>>);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(raw), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: compile-weather <raw-dir> <out-file>");
        return ExitCode::FAILURE;
    };
    let (raw_dir, out_file) = (PathBuf::from(raw), PathBuf::from(out));
    match run(&raw_dir, &out_file) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compile-weather: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(raw_dir: &Path, out_file: &Path) -> Result<(), String> {
    let tu = find_product(raw_dir, "produkt_tu_stunde")?;
    let ff = find_product(raw_dir, "produkt_ff_stunde")?;
    let n = find_product(raw_dir, "produkt_n_stunde")?;
    let rr = find_product(raw_dir, "produkt_rr_stunde")?;
    let st = find_product(raw_dir, "produkt_st_stunde")?;

    let mut fills = FillCounts::default();

    let (mut temp, mut rh) = parse_two_columns(&tu, 3, 4)?;
    fills.temp = fill_linear(&mut temp, MAX_PHYSICS_GAP_H, "temperature")?;
    fills.rel_humidity = fill_linear(&mut rh, MAX_PHYSICS_GAP_H, "humidity")?;

    let (mut wind_ms, mut wind_dir) = parse_two_columns(&ff, 3, 4)?;
    report_long_gap(&wind_ms, "wind");
    fills.wind_ms = fill_linear(&mut wind_ms, MAX_SCENERY_GAP_H, "wind speed")?;
    fills.wind_dir = fill_nearest(&mut wind_dir, MAX_SCENERY_GAP_H, "wind direction")?;

    let (mut cloud, _) = parse_two_columns(&n, 4, 4)?;
    report_long_gap(&cloud, "cloud cover");
    fills.cloud = fill_nearest(&mut cloud, MAX_SCENERY_GAP_H, "cloud cover")?;

    let (mut precip_mm, mut precip_form) = parse_two_columns(&rr, 3, 5)?;
    fills.precip_mm = fill_zero(&mut precip_mm);
    fills.precip_form = fill_precip_form(&mut precip_form, &precip_mm);

    let ghi = distribute_solar(&st)?;

    let unwrap_all = |series: Vec<Option<f64>>| -> Vec<f64> {
        series.into_iter().map(|v| v.expect("filled")).collect()
    };
    let temp = unwrap_all(temp);
    let rh = unwrap_all(rh);
    let wind_ms = unwrap_all(wind_ms);
    let wind_dir = unwrap_all(wind_dir);
    let cloud = unwrap_all(cloud);
    let precip_mm = unwrap_all(precip_mm);
    let precip_form = unwrap_all(precip_form);

    let data = ArtifactData {
        station_id: STATION_ID,
        year: YEAR,
        fills,
        temp_c_x10: quantize_x10(&temp, -500, 500, "temperature")?,
        rh_pct_x10: quantize_x10(&rh, 0, 1000, "humidity")?,
        ghi_wm2_x10: quantize_x10(&ghi, 0, 15000, "irradiance")?,
        precip_mm_x10: quantize_x10(&precip_mm, 0, 1000, "precipitation")?,
        wind_ms_x10: quantize_x10(&wind_ms, 0, 600, "wind speed")?,
        wind_dir_deg: quantize_x1(&wind_dir, 0, 360, "wind direction")?,
        precip_form: quantize_code(&precip_form, "precipitation form")?,
        cloud_okta: quantize_code(&cloud, "cloud cover")?,
    };
    let bytes = encode(&data)?;
    fs::write(out_file, &bytes).map_err(|e| format!("write {}: {e}", out_file.display()))?;

    report(&data, &ghi, &temp, &precip_mm, &bytes, out_file);
    Ok(())
}

/// Finds exactly one file under `dir` (recursively) whose name starts with
/// `prefix`.
fn find_product(dir: &Path, prefix: &str) -> Result<PathBuf, String> {
    fn walk(dir: &Path, prefix: &str, hits: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, prefix, hits);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix))
            {
                hits.push(path);
            }
        }
    }
    let mut hits = Vec::new();
    walk(dir, prefix, &mut hits);
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(format!("no {prefix}* file under {}", dir.display())),
        n => Err(format!("{n} {prefix}* files under {}", dir.display())),
    }
}

/// Parses a DWD hourly product file into two per-hour series for [`YEAR`].
///
/// `col_a` and `col_b` are zero-based semicolon field indices. Timestamps
/// are `YYYYMMDDHH`. Missing markers and absent rows both become `None`.
fn parse_two_columns(path: &Path, col_a: usize, col_b: usize) -> Result<TwoSeries, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut a = vec![None; HOURS_2024];
    let mut b = vec![None; HOURS_2024];
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split(';').map(str::trim).collect();
        if fields.len() <= col_a.max(col_b) {
            continue;
        }
        let stamp = fields[1];
        if !stamp.starts_with("2024") || stamp.len() != 10 {
            continue;
        }
        let idx = hour_index(stamp)?;
        a[idx] = parse_value(fields[col_a]);
        b[idx] = parse_value(fields[col_b]);
    }
    Ok((a, b))
}

fn parse_value(field: &str) -> Option<f64> {
    let value: f64 = field.parse().ok()?;
    if (value - MISSING).abs() < 0.5 {
        None
    } else {
        Some(value)
    }
}

/// Hour bucket index of a `YYYYMMDDHH` stamp inside [`YEAR`].
fn hour_index(stamp: &str) -> Result<usize, String> {
    let month: usize = stamp[4..6].parse().map_err(|_| bad_stamp(stamp))?;
    let day: usize = stamp[6..8].parse().map_err(|_| bad_stamp(stamp))?;
    let hour: usize = stamp[8..10].parse().map_err(|_| bad_stamp(stamp))?;
    if !(1..=12).contains(&month) || day == 0 || hour > 23 {
        return Err(bad_stamp(stamp));
    }
    let idx = (DAYS_BEFORE_MONTH[month - 1] + day - 1) * 24 + hour;
    if idx >= HOURS_2024 {
        return Err(bad_stamp(stamp));
    }
    Ok(idx)
}

fn bad_stamp(stamp: &str) -> String {
    format!("unparseable timestamp {stamp}")
}

/// Linear interpolation across interior gaps, nearest copy at the edges.
fn fill_linear(series: &mut [Option<f64>], max_gap: usize, name: &str) -> Result<u16, String> {
    check_longest_gap(series, max_gap, name)?;
    let mut filled: u16 = 0;
    let known: Vec<usize> = (0..series.len()).filter(|&i| series[i].is_some()).collect();
    if known.is_empty() {
        return Err(format!("{name}: no observations at all"));
    }
    for i in 0..series.len() {
        if series[i].is_some() {
            continue;
        }
        let next = known.partition_point(|&k| k < i);
        let value = match (next.checked_sub(1).map(|p| known[p]), known.get(next)) {
            (Some(prev), Some(&nxt)) => {
                let span = (nxt - prev) as f64;
                let t = (i - prev) as f64 / span;
                series[prev].expect("known") * (1.0 - t) + series[nxt].expect("known") * t
            }
            (Some(prev), None) => series[prev].expect("known"),
            (None, Some(&nxt)) => series[nxt].expect("known"),
            (None, None) => unreachable!("known is non-empty"),
        };
        series[i] = Some(value);
        filled += 1;
    }
    Ok(filled)
}

/// Nearest-observation copy (earlier wins ties) for categorical series.
fn fill_nearest(series: &mut [Option<f64>], max_gap: usize, name: &str) -> Result<u16, String> {
    check_longest_gap(series, max_gap, name)?;
    let mut filled: u16 = 0;
    let known: Vec<usize> = (0..series.len()).filter(|&i| series[i].is_some()).collect();
    if known.is_empty() {
        return Err(format!("{name}: no observations at all"));
    }
    for i in 0..series.len() {
        if series[i].is_some() {
            continue;
        }
        let next = known.partition_point(|&k| k < i);
        let source = match (next.checked_sub(1).map(|p| known[p]), known.get(next)) {
            (Some(prev), Some(&nxt)) => {
                if i - prev <= nxt - i {
                    prev
                } else {
                    nxt
                }
            }
            (Some(prev), None) => prev,
            (None, Some(&nxt)) => nxt,
            (None, None) => unreachable!("known is non-empty"),
        };
        series[i] = series[source];
        filled += 1;
    }
    Ok(filled)
}

/// Missing precipitation amounts become "no precipitation".
fn fill_zero(series: &mut [Option<f64>]) -> u16 {
    let mut filled: u16 = 0;
    for slot in series.iter_mut() {
        if slot.is_none() {
            *slot = Some(0.0);
            filled += 1;
        }
    }
    filled
}

/// Missing form: no-precipitation hours get code 0, wet hours code 4
/// (precipitation reported, form unknown).
fn fill_precip_form(form: &mut [Option<f64>], mm: &[Option<f64>]) -> u16 {
    let mut filled: u16 = 0;
    for (slot, amount) in form.iter_mut().zip(mm) {
        if slot.is_none() {
            let wet = amount.expect("mm filled first") > 0.0;
            *slot = Some(if wet { 4.0 } else { 0.0 });
            filled += 1;
        }
    }
    filled
}

fn check_longest_gap(series: &[Option<f64>], max_gap: usize, name: &str) -> Result<(), String> {
    let (longest, _) = longest_gap(series);
    if longest > max_gap {
        return Err(format!(
            "{name}: longest gap {longest} h exceeds {max_gap} h; \
             use a fallback station (design decision D1)"
        ));
    }
    Ok(())
}

/// Length and start index of the longest missing run.
fn longest_gap(series: &[Option<f64>]) -> (usize, usize) {
    let mut longest = 0usize;
    let mut longest_start = 0usize;
    let mut run = 0usize;
    for (i, value) in series.iter().enumerate() {
        if value.is_none() {
            run += 1;
            if run > longest {
                longest = run;
                longest_start = i + 1 - run;
            }
        } else {
            run = 0;
        }
    }
    (longest, longest_start)
}

/// Prints where a notable (>24 h) scenery outage sits, for the record.
fn report_long_gap(series: &[Option<f64>], name: &str) {
    let (longest, start) = longest_gap(series);
    if longest > MAX_PHYSICS_GAP_H {
        let day_of_year = start / 24;
        let month = DAYS_BEFORE_MONTH
            .iter()
            .rposition(|&d| d <= day_of_year)
            .unwrap_or(0);
        let day = day_of_year - DAYS_BEFORE_MONTH[month] + 1;
        println!(
            "note: {name} outage of {longest} h starting {YEAR}-{:02}-{day:02} (hour index {start})",
            month + 1
        );
    }
}

/// Distributes DWD solar interval sums onto UTC hour buckets.
///
/// The hourly solar product stamps intervals in true solar time with an
/// exact UTC end (`MESS_DATUM`, `YYYYMMDDHH:MM`). Because true solar hours
/// drift against UTC, consecutive stamps are not exactly 3600 s apart; each
/// row's interval therefore runs from the previous row's end to its own end
/// (which tiles the year seamlessly), and its energy is spread over the UTC
/// hour buckets it overlaps. The result is the mean W/m2 per UTC hour.
/// Fails on gaps in the solar series or a missing value in a covering row.
fn distribute_solar(path: &Path) -> Result<Vec<f64>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let span_s = (HOURS_2024 * 3600) as i64;
    let mut energy_jm2 = vec![0.0f64; HOURS_2024];
    let mut covered_s = vec![0i64; HOURS_2024];
    let mut prev_end: Option<i64> = None;
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split(';').map(str::trim).collect();
        if fields.len() < 9 {
            continue;
        }
        let stamp = fields[1];
        // Stamps that can matter: the year plus a day of slack on each side
        // (the previous end must be known before the window starts).
        if stamp.len() != 13
            || !(stamp.starts_with("2024")
                || stamp.starts_with("20231230")
                || stamp.starts_with("20231231")
                || stamp.starts_with("20250101"))
        {
            continue;
        }
        let end_s = seconds_from_year_start(stamp)?;
        let start_s = match prev_end {
            Some(prev) => {
                let duration = end_s - prev;
                if !(1800..=5400).contains(&duration) {
                    return Err(format!(
                        "solar series gap: interval ending {stamp} is {duration} s long"
                    ));
                }
                prev
            }
            None => end_s - 3600,
        };
        prev_end = Some(end_s);
        if end_s <= 0 || start_s >= span_s {
            continue;
        }
        let fg = parse_value(fields[5])
            .ok_or_else(|| format!("missing FG_LBERG in interval ending {stamp}"))?;
        // J/cm2 over the interval -> J/m2.
        let interval_jm2 = fg * 10_000.0;
        let duration = end_s - start_s;
        let first_bucket = (start_s.max(0) / 3600) as usize;
        let last_bucket = (((end_s.min(span_s) - 1) / 3600) as usize).min(HOURS_2024 - 1);
        for bucket in first_bucket..=last_bucket {
            let bucket_start = (bucket as i64) * 3600;
            let overlap = (end_s.min(bucket_start + 3600) - start_s.max(bucket_start)).max(0);
            energy_jm2[bucket] += interval_jm2 * (overlap as f64) / (duration as f64);
            covered_s[bucket] += overlap;
        }
    }
    if let Some(bucket) = covered_s.iter().position(|&s| s != 3600) {
        return Err(format!(
            "solar coverage hole: UTC hour bucket {bucket} covered {} s of 3600",
            covered_s[bucket]
        ));
    }
    Ok(energy_jm2.iter().map(|e| e / 3600.0).collect())
}

/// Seconds since 2024-01-01 00:00 UTC of a `YYYYMMDDHH:MM` stamp.
fn seconds_from_year_start(stamp: &str) -> Result<i64, String> {
    let year: i64 = stamp[0..4].parse().map_err(|_| bad_stamp(stamp))?;
    let month: i64 = stamp[4..6].parse().map_err(|_| bad_stamp(stamp))?;
    let day: i64 = stamp[6..8].parse().map_err(|_| bad_stamp(stamp))?;
    let hour: i64 = stamp[8..10].parse().map_err(|_| bad_stamp(stamp))?;
    let minute: i64 = stamp[11..13].parse().map_err(|_| bad_stamp(stamp))?;
    let days = days_from_civil(year, month, day) - days_from_civil(2024, 1, 1);
    Ok(days * 86_400 + hour * 3600 + minute * 60)
}

/// Days since the civil epoch, after Howard Hinnant's algorithm (the same
/// calendar arithmetic the view layer uses).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn quantize_x10(series: &[f64], min: i32, max: i32, name: &str) -> Result<Vec<i16>, String> {
    series
        .iter()
        .map(|&v| {
            let q = (v * 10.0).round();
            let q_int = q as i32;
            if q_int < min || q_int > max {
                return Err(format!("{name}: value {v} outside [{min}, {max}] x0.1"));
            }
            Ok(q_int as i16)
        })
        .collect()
}

fn quantize_x1(series: &[f64], min: i32, max: i32, name: &str) -> Result<Vec<i16>, String> {
    series
        .iter()
        .map(|&v| {
            let q = v.round() as i32;
            if q < min || q > max {
                return Err(format!("{name}: value {v} outside [{min}, {max}]"));
            }
            Ok(q as i16)
        })
        .collect()
}

fn quantize_code(series: &[f64], name: &str) -> Result<Vec<u8>, String> {
    series
        .iter()
        .map(|&v| {
            let q = v.round() as i64;
            if !(0..=255).contains(&q) {
                return Err(format!("{name}: code {v} outside u8"));
            }
            Ok(q as u8)
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation)]
fn report(
    data: &ArtifactData,
    ghi: &[f64],
    temp: &[f64],
    precip_mm: &[f64],
    bytes: &[u8],
    out_file: &Path,
) {
    let fills = data.fills;
    let annual_ghi_kwh_m2 = ghi.iter().sum::<f64>() / 1000.0;
    let mean_temp = temp.iter().sum::<f64>() / temp.len() as f64;
    let annual_precip_mm = precip_mm.iter().sum::<f64>();
    println!(
        "station {} ({}), year {}, {} hours",
        data.station_id,
        bess_data::STATION_NAME,
        data.year,
        data.temp_c_x10.len()
    );
    println!(
        "fills: temp {} · rh {} · ghi {} · precip mm {} · form {} · wind {} · dir {} · cloud {}",
        fills.temp,
        fills.rel_humidity,
        fills.ghi,
        fills.precip_mm,
        fills.precip_form,
        fills.wind_ms,
        fills.wind_dir,
        fills.cloud
    );
    println!(
        "annual: mean temp {mean_temp:.1} C · GHI {annual_ghi_kwh_m2:.0} kWh/m2 · precip {annual_precip_mm:.0} mm"
    );
    println!("artifact: {} bytes -> {}", bytes.len(), out_file.display());
    println!("FNV-1a 64: 0x{:016x}", fnv1a_64(bytes));
}
