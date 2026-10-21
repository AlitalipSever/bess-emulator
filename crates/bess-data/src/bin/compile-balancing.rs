//! Compiles regelleistung.net's daily result files, as CSV, into the
//! balancing artifact a run loads with `--balancing`.
//!
//! Usage:
//! `cargo run -p bess-data --bin compile-balancing -- <raw-dir> <out-file>`
//! where `<raw-dir>` holds `FCR_CAPACITY_<date>.csv`,
//! `aFRR_CAPACITY_<date>.csv` and `aFRR_ENERGY_<date>.csv` for every day
//! of 2024, written by `scripts/xlsx-to-csv.py` beside the files
//! `scripts/fetch-balancing.sh` downloads. Prints a report and the
//! artifact's hash; nothing is pinned in code, since the artifact is not
//! bundled.
//!
//! Gap policy: every day needs its six FCR blocks, its twelve aFRR
//! capacity blocks and its quarter-hours of energy results (96, or 92 and
//! 100 on the DST days); a missing or unreadable cell stops the compile,
//! with one exception the files themselves explain: on days the German
//! FCR demand cleared without a local constraint the German settlement
//! price column reads `-` and the cross-border settlement price is the
//! price Germany paid, so that column is read instead and the count of
//! such blocks is reported (none in 2024: every block had a German
//! price in its first tender's row). A day may carry a second tender's
//! rows with no price at all (an additional procurement that cleared
//! nothing); those rows are skipped, and only a block with no priced row
//! stops the compile. The German columns are read
//! otherwise: the FCR settlement capacity price, the aFRR volume-weighted
//! average capacity price, the energy merit order's minimum and average
//! price.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bess_data::balancing::codec::{encode, ArtifactData};
use bess_data::balancing::BLOCKS;
use bess_data::calendar::{civil_from_days, days_from_civil};
use bess_data::fnv1a_64;

const YEAR: i64 = 2024;
const START_OFFSET_H: i16 = 1;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(raw), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: compile-balancing <raw-dir> <out-file>");
        return ExitCode::FAILURE;
    };
    match run(&PathBuf::from(raw), &PathBuf::from(out)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compile-balancing: {err}");
            ExitCode::FAILURE
        }
    }
}

/// A CSV as the converter writes it: a header row, then rows, `;`
/// separated.
struct Table {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut lines = text
            .lines()
            .map(|l| l.split(';').map(str::to_owned).collect::<Vec<_>>());
        let header = lines
            .next()
            .ok_or_else(|| format!("{}: empty", path.display()))?;
        Ok(Self {
            header,
            rows: lines.collect(),
        })
    }

    fn column(&self, name: &str) -> Result<usize, String> {
        self.header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("no column {name}"))
    }

    fn cell(row: &[String], col: usize) -> Result<f64, String> {
        row.get(col)
            .and_then(|v| v.parse::<f64>().ok())
            .ok_or_else(|| format!("unreadable cell {:?}", row.get(col)))
    }
}

fn cents(v: f64) -> i32 {
    (v * 100.0).round() as i32
}

fn run(raw: &Path, out: &Path) -> Result<(), String> {
    let first = days_from_civil(YEAR, 1, 1);
    let last = days_from_civil(YEAR + 1, 1, 1);
    let mut data = ArtifactData {
        year: YEAR as u16,
        start_offset_h: START_OFFSET_H,
        fcr_ct: Vec::new(),
        afrr_pos_ct: Vec::new(),
        afrr_neg_ct: Vec::new(),
        energy_pos_min_ct: Vec::new(),
        energy_pos_avg_ct: Vec::new(),
        energy_neg_min_ct: Vec::new(),
        energy_neg_avg_ct: Vec::new(),
        quarter_starts: vec![0],
    };
    for day in first..last {
        let (y, m, d) = civil_from_days(day);
        let date = format!("{y:04}-{m:02}-{d:02}");
        fcr_day(raw, &date, &mut data)?;
        afrr_capacity_day(raw, &date, &mut data)?;
        let quarters = afrr_energy_day(raw, &date, &mut data)?;
        let next = data.quarter_starts.last().expect("a table") + quarters as u32;
        data.quarter_starts.push(next);
    }
    let bytes = encode(&data)?;
    fs::write(out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let days = data.quarter_starts.len() - 1;
    let crossborder = CROSSBORDER.load(std::sync::atomic::Ordering::Relaxed);
    let fcr_year: f64 = data.fcr_ct.iter().map(|&c| f64::from(c) / 100.0).sum();
    println!(
        "{days} days, {} quarters, FCR {:.0} EUR/MW over the year holding every block \
         ({crossborder} blocks priced from the cross-border column), artifact hash {:#018x}, \
         written to {}",
        data.quarter_starts.last().expect("a table"),
        fcr_year,
        fnv1a_64(&bytes),
        out.display()
    );
    Ok(())
}

/// FCR blocks whose German price came from the cross-border column.
static CROSSBORDER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The six FCR blocks of one day, in block order.
fn fcr_day(raw: &Path, date: &str, data: &mut ArtifactData) -> Result<(), String> {
    let t = Table::read(&raw.join(format!("FCR_CAPACITY_{date}.csv")))?;
    let name = t.column("PRODUCTNAME")?;
    let price = t.column("GERMANY_SETTLEMENTCAPACITY_PRICE_[EUR/MW]")?;
    let crossborder = t.column("CROSSBORDER_SETTLEMENTCAPACITY_PRICE_[EUR/MW]")?;
    let mut blocks = vec![None; BLOCKS];
    for row in &t.rows {
        let Some(product) = row.get(name) else {
            continue;
        };
        if let Some(b) = block_of(product) {
            if blocks[b].is_some() {
                continue;
            }
            let v = match Table::cell(row, price) {
                Ok(v) => Some(v),
                Err(_) if row.get(price).is_some_and(|c| c == "-") => {
                    match Table::cell(row, crossborder) {
                        Ok(v) => {
                            CROSSBORDER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            Some(v)
                        }
                        // A tender that cleared nothing: no price anywhere.
                        Err(_) => None,
                    }
                }
                Err(e) => return Err(format!("FCR {date} {product}: {e}")),
            };
            blocks[b] = v;
        }
    }
    for (b, v) in blocks.into_iter().enumerate() {
        data.fcr_ct.push(cents(
            v.ok_or_else(|| format!("FCR {date}: block {b} missing"))?,
        ));
    }
    Ok(())
}

/// The twelve aFRR capacity blocks of one day, positive then negative.
fn afrr_capacity_day(raw: &Path, date: &str, data: &mut ArtifactData) -> Result<(), String> {
    let t = Table::read(&raw.join(format!("aFRR_CAPACITY_{date}.csv")))?;
    let name = t.column("PRODUCT")?;
    let price = t.column("GERMANY_AVERAGE_CAPACITY_PRICE_[(EUR/MW)/h]")?;
    let mut pos = vec![None; BLOCKS];
    let mut neg = vec![None; BLOCKS];
    for row in &t.rows {
        let Some(product) = row.get(name) else {
            continue;
        };
        let Some(b) = block_of(product) else { continue };
        let v = Table::cell(row, price).map_err(|e| format!("aFRR {date} {product}: {e}"))?;
        if product.starts_with("POS") {
            pos[b] = Some(v);
        } else if product.starts_with("NEG") {
            neg[b] = Some(v);
        }
    }
    for (b, v) in pos.into_iter().enumerate() {
        data.afrr_pos_ct.push(cents(
            v.ok_or_else(|| format!("aFRR {date}: POS block {b} missing"))?,
        ));
    }
    for (b, v) in neg.into_iter().enumerate() {
        data.afrr_neg_ct.push(cents(
            v.ok_or_else(|| format!("aFRR {date}: NEG block {b} missing"))?,
        ));
    }
    Ok(())
}

/// The quarter-hours of one day's energy merit order; returns how many.
fn afrr_energy_day(raw: &Path, date: &str, data: &mut ArtifactData) -> Result<usize, String> {
    let t = Table::read(&raw.join(format!("aFRR_ENERGY_{date}.csv")))?;
    let name = t.column("PRODUCT")?;
    let min = t.column("GERMANY_MIN_ENERGY_PRICE_[EUR/MWh]")?;
    let avg = t.column("GERMANY_AVERAGE_ENERGY_PRICE_[EUR/MWh]")?;
    let mut pos: Vec<(usize, f64, f64)> = Vec::new();
    let mut neg: Vec<(usize, f64, f64)> = Vec::new();
    for row in &t.rows {
        let Some(product) = row.get(name) else {
            continue;
        };
        let Some((dir, q)) = product.split_once('_') else {
            continue;
        };
        let Ok(q) = q.parse::<usize>() else { continue };
        let entry = (
            q,
            Table::cell(row, min).map_err(|e| format!("energy {date} {product}: {e}"))?,
            Table::cell(row, avg).map_err(|e| format!("energy {date} {product}: {e}"))?,
        );
        match dir {
            "POS" => pos.push(entry),
            "NEG" => neg.push(entry),
            _ => {}
        }
    }
    pos.sort_by_key(|e| e.0);
    neg.sort_by_key(|e| e.0);
    if pos.len() != neg.len() || !(92..=100).contains(&pos.len()) {
        return Err(format!(
            "energy {date}: {} positive and {} negative quarters",
            pos.len(),
            neg.len()
        ));
    }
    for (i, e) in pos.iter().enumerate() {
        if e.0 != i + 1 {
            return Err(format!("energy {date}: positive quarter {} missing", i + 1));
        }
        data.energy_pos_min_ct.push(cents(e.1));
        data.energy_pos_avg_ct.push(cents(e.2));
    }
    for (i, e) in neg.iter().enumerate() {
        if e.0 != i + 1 {
            return Err(format!("energy {date}: negative quarter {} missing", i + 1));
        }
        data.energy_neg_min_ct.push(cents(e.1));
        data.energy_neg_avg_ct.push(cents(e.2));
    }
    Ok(pos.len())
}

/// The block a product name ends in: `NEGPOS_00_04` is 0, `POS_20_24` is 5.
fn block_of(product: &str) -> Option<usize> {
    let start = product.rsplit('_').nth(1)?.parse::<usize>().ok()?;
    if start % 4 == 0 && start < 24 {
        Some(start / 4)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::block_of;

    #[test]
    fn blocks_are_read_off_the_product_name() {
        assert_eq!(block_of("NEGPOS_00_04"), Some(0));
        assert_eq!(block_of("POS_20_24"), Some(5));
        assert_eq!(block_of("NEG_12_16"), Some(3));
        assert_eq!(block_of("NEG_001"), None);
    }
}
