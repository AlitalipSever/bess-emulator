//! Calibration harness for the reference plant.
//!
//! Realism in this project is never claimed, always measured. `bess-bench`
//! is where the measuring happens: it runs GW-01 over a replayed year as fast
//! as the machine allows, reads the meters the kernel kept while it ran, and
//! holds the result against bands drawn from public data. Its output is the
//! generated block of CALIBRATION.md and the record that block is rendered
//! from.
//!
//! ```text
//! cargo run --release -p bess-bench                 # measure and print
//! cargo run --release -p bess-bench -- --write      # update the record and the document
//! cargo run --release -p bess-bench -- --check      # what CI runs
//! cargo run --release -p bess-bench -- --check-docs # document matches the record, no simulation
//! ```

mod bands;
mod report;
mod run;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;

use run::{Kpis, RunSpec, DEFAULT_DAYS, DEFAULT_SEED, DEFAULT_START_UNIX_S};

/// Committed machine-readable record of the annual run.
const RECORD_PATH: &str = "calibration/m1-annual.json";
/// Document carrying the generated block.
const CALIBRATION_PATH: &str = "CALIBRATION.md";

/// Run the reference plant over a replayed year and measure the milestone
/// gates.
#[derive(Debug, Parser)]
#[command(name = "bess-bench", version)]
// Command-line flags, not program state: a mode enum would collapse
// combinations the CLI is meant to accept separately.
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Days to simulate.
    #[arg(long, default_value_t = DEFAULT_DAYS)]
    days: u64,

    /// PRNG seed.
    #[arg(long, default_value_t = DEFAULT_SEED)]
    seed: u64,

    /// Unix timestamp (UTC seconds) to start from.
    #[arg(long, default_value_t = DEFAULT_START_UNIX_S)]
    start: i64,

    /// Write the measurement to this path as JSON, in addition to printing it.
    #[arg(long, value_name = "PATH")]
    json: Option<PathBuf>,

    /// Update the committed record and the generated block of CALIBRATION.md.
    #[arg(long)]
    write: bool,

    /// Measure, then fail if a gate is missed or the committed record is stale.
    #[arg(long)]
    check: bool,

    /// Check that the generated block matches the committed record. Reads
    /// both files and simulates nothing, so it is free to run on every build.
    #[arg(long)]
    check_docs: bool,

    /// Rewrite the generated block from the committed record without
    /// simulating. For a change to how the block is rendered, where a fresh
    /// measurement would produce the same record it already has.
    #[arg(long)]
    render: bool,

    /// Do not print per-week progress.
    #[arg(long)]
    quiet: bool,

    /// Path to the committed record.
    #[arg(long, value_name = "PATH", default_value = RECORD_PATH)]
    record: PathBuf,

    /// Path to the document carrying the generated block.
    #[arg(long, value_name = "PATH", default_value = CALIBRATION_PATH)]
    calibration: PathBuf,
}

impl Cli {
    fn spec(&self) -> RunSpec {
        RunSpec {
            seed: self.seed,
            start_unix_s: self.start,
            days: self.days,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.check_docs {
        return match check_docs(&cli.record, &cli.calibration) {
            Ok(()) => {
                println!("CALIBRATION.md matches {}", cli.record.display());
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    if cli.render {
        return match render_document(&cli.record, &cli.calibration) {
            Ok(()) => {
                println!(
                    "rewrote the generated block of {} from {}",
                    cli.calibration.display(),
                    cli.record.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    let spec = cli.spec();
    if (cli.write || cli.check) && spec != RunSpec::default() {
        return fail(
            "--write and --check publish the annual run, so they only accept the \
             default --days, --seed and --start. Drop them, or drop --write/--check \
             to explore a different run.",
        );
    }

    let started = Instant::now();
    let kpis = measure(spec, cli.quiet);
    let elapsed = started.elapsed().as_secs_f64();
    print_report(&kpis, elapsed);

    if let Some(path) = &cli.json {
        if let Err(message) = write_file(path, &report::to_json(&kpis)) {
            return fail(&message);
        }
        println!("wrote {}", path.display());
    }

    if cli.write {
        return match write_published(&kpis, &cli.record, &cli.calibration) {
            Ok(()) => {
                println!(
                    "wrote {} and the generated block of {}",
                    cli.record.display(),
                    cli.calibration.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    if cli.check {
        return match check(&kpis, &cli.record, &cli.calibration) {
            Ok(()) => {
                println!("every gate holds and the published record is current");
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    ExitCode::SUCCESS
}

/// Run the plant, reporting progress on stderr so a redirected stdout stays
/// machine-readable.
fn measure(spec: RunSpec, quiet: bool) -> Kpis {
    let started = Instant::now();
    run::run(spec, |day, days| {
        if quiet || day % 7 != 0 {
            return;
        }
        let elapsed = started.elapsed().as_secs_f64();
        let remaining = elapsed / day as f64 * (days - day) as f64;
        eprintln!("  day {day}/{days}, {elapsed:.0}s elapsed, {remaining:.0}s left");
    })
}

fn print_report(kpis: &Kpis, elapsed_s: f64) {
    let e = &kpis.energy;
    println!("{} over {} days", kpis.run.site_id, kpis.run.days);
    println!(
        "  {:.0} ticks in {elapsed_s:.1}s ({:.0} ticks/s)",
        kpis.run.ticks as f64,
        kpis.run.ticks as f64 / elapsed_s.max(1.0e-9)
    );
    println!(
        "  import {:.1} MWh, export {:.1} MWh, RTE {:.4}, {:.1} equivalent full cycles",
        e.import_mwh, e.export_mwh, e.round_trip_efficiency, e.equivalent_full_cycles
    );
    println!(
        "  auxiliary {:.2}% of import, residual {:.5}% of throughput",
        e.aux_share_of_import * 100.0,
        e.balance_residual_share * 100.0
    );
    for (name, mwh) in kpis.losses.waterfall() {
        println!("  {name:<36} {mwh:>9.1} MWh");
    }
}

/// Write the record, then render the document from what was written. Reading
/// the record back means the document can only ever describe a record that
/// exists on disk.
fn write_published(kpis: &Kpis, record: &Path, calibration: &Path) -> Result<(), String> {
    write_file(record, &report::to_json(kpis))?;
    render_document(record, calibration)
}

/// Render the generated block from whatever the record on disk says.
fn render_document(record: &Path, calibration: &Path) -> Result<(), String> {
    let published = read_record(record)?;
    let document = read_file(calibration)?;
    let spliced = report::splice(&document, &report::render(&published))
        .map_err(|err| format!("{}: {err}", calibration.display()))?;
    write_file(calibration, &spliced)
}

/// The CI gate: bands hold, the record is current, the document matches it.
fn check(kpis: &Kpis, record: &Path, calibration: &Path) -> Result<(), String> {
    let mut failures = bands::check(kpis);

    match read_record(record) {
        Ok(published) => {
            let drift = report::drift(kpis, &published);
            if !drift.is_empty() {
                failures.push(format!(
                    "{} is stale ({} figures moved); regenerate with \
                     `cargo run --release -p bess-bench -- --write`:\n    {}",
                    record.display(),
                    drift.len(),
                    drift.join("\n    ")
                ));
            }
        }
        Err(message) => failures.push(message),
    }

    if let Err(message) = check_docs(record, calibration) {
        failures.push(message);
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Whether the generated block still is what the record renders to.
fn check_docs(record: &Path, calibration: &Path) -> Result<(), String> {
    let published = read_record(record)?;
    let document = read_file(calibration)?;
    let block =
        report::extract(&document).map_err(|err| format!("{}: {err}", calibration.display()))?;
    if block == report::render(&published) {
        Ok(())
    } else {
        Err(format!(
            "the generated block of {} does not match {}; regenerate with \
             `cargo run --release -p bess-bench -- --write`",
            calibration.display(),
            record.display()
        ))
    }
}

fn read_record(path: &Path) -> Result<Kpis, String> {
    report::from_json(&read_file(path)?).map_err(|err| format!("{}: {err}", path.display()))
}

fn read_file(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|err| {
        format!(
            "cannot read {}: {err} (paths are relative to the repository root)",
            path.display()
        )
    })
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

fn fail(message: &str) -> ExitCode {
    eprintln!("bess-bench: {message}");
    ExitCode::FAILURE
}
