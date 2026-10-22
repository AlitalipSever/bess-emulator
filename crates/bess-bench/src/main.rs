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
//! cargo run --release -p bess-bench -- --periods p.csv # every closed quarter-hour, as CSV
//! ```

mod aging;
mod alarms;
mod bands;
mod estimator;
mod periods;
mod report;
mod run;
mod series;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;

use run::{Kpis, RunSpec, DEFAULT_DAYS, DEFAULT_SEED, DEFAULT_START_UNIX_S};

/// Committed machine-readable record of the annual run.
const RECORD_PATH: &str = "calibration/m1-annual.json";
/// The market participant's year (M4 phase 4): its own record, no series
/// and no generated block, the revenue band as its gate.
const MARKET_RECORD_PATH: &str = "calibration/m4-market.json";
/// The projected decade (M5 phase 2).
const TRAJECTORY_PATH: &str = "calibration/m5-aging.json";
/// Document carrying the generated block.
const CALIBRATION_PATH: &str = "CALIBRATION.md";
/// Committed chart material: the year month by month, and one hard week.
const SERIES_PATH: &str = "calibration/m1-study-series.json";

/// Run the reference plant over a replayed year and measure the milestone
/// gates.
#[derive(Debug, Parser)]
#[command(name = "bess-bench", version)]
// Command-line flags, not program state: a mode enum would collapse
// combinations the CLI is meant to accept separately.
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Days to simulate.
    #[arg(long, default_value_t = DEFAULT_DAYS, value_parser = clap::value_parser!(u64).range(1..))]
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

    /// Replay a measured grid-frequency year (`compile-frequency` output)
    /// instead of the synthetic series. A research run: --write and
    /// --check publish the synthetic year and refuse it.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["write", "check"])]
    frequency: Option<PathBuf>,

    /// Replay a year of balancing tender results (`compile-balancing`
    /// output) in place of the bundled annual figures. A research run,
    /// like `--frequency`.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["write", "check"])]
    balancing: Option<PathBuf>,

    /// Run the market participant's strategy (M4 phase 4) in place of the
    /// day-ahead plan: its own record (`calibration/m4-market.json`), the
    /// revenue band as its gate, no series and no generated block.
    #[arg(long)]
    market: bool,

    /// Replay a per-second aFRR request year (`compile-afrr` output) in
    /// place of the synthetic request. A research run.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["write", "check"])]
    afrr: Option<PathBuf>,

    /// Years in the published trajectory (M5 phase 2): the measured year
    /// and the rest projected from its stress through the aging model,
    /// seconds a year. With --write and no --market, `calibration/
    /// m5-aging.json` is written when this is above one.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=40))]
    years: u32,

    /// Tick the physics for every projected year instead of replaying the
    /// stress year (hours per year). A research run.
    #[arg(long, conflicts_with_all = ["write", "check"])]
    resimulate: bool,

    /// Write the trajectory to this path as JSON, whatever --years.
    #[arg(long, value_name = "PATH")]
    trajectory: Option<PathBuf>,

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

    /// Write every closed quarter-hour of the fiscal meter to this path as
    /// CSV (about a megabyte for the year; never committed).
    #[arg(long, value_name = "PATH")]
    periods: Option<PathBuf>,

    /// Do not print per-week progress.
    #[arg(long)]
    quiet: bool,

    /// Path to the committed record.
    #[arg(long, value_name = "PATH", default_value = RECORD_PATH)]
    record: PathBuf,

    /// Path to the document carrying the generated block.
    #[arg(long, value_name = "PATH", default_value = CALIBRATION_PATH)]
    calibration: PathBuf,

    /// Path to the committed study series.
    #[arg(long, value_name = "PATH", default_value = SERIES_PATH)]
    series: PathBuf,
}

impl Cli {
    fn spec(&self) -> RunSpec {
        RunSpec {
            years: self.years,
            resimulate: self.resimulate,
            seed: self.seed,
            start_unix_s: self.start,
            days: self.days,
            market: self.market,
        }
    }

    /// The record a published run reads and writes: the market year has
    /// its own unless one was named.
    fn record_path(&self) -> PathBuf {
        if self.market && self.record == Path::new(RECORD_PATH) {
            PathBuf::from(MARKET_RECORD_PATH)
        } else {
            self.record.clone()
        }
    }
}

/// The replayed years a research run attaches before anything builds a
/// driver (M4): the aFRR request, the balancing results, the frequency.
fn attach_replays(cli: &Cli) -> Result<(), String> {
    if let Some(path) = &cli.afrr {
        let year = bess_models::AfrrRequestYear::load(path)?;
        println!("replaying aFRR request year {:#018x}", year.hash());
        bess_models::replay_afrr(year).ok_or("an aFRR request year is already attached")?;
    }
    if let Some(path) = &cli.balancing {
        let year = bess_models::BalancingYear::load(path)?;
        println!("replaying balancing year {:#018x}", year.hash());
        bess_models::replay_balancing(year).ok_or("a balancing year is already attached")?;
    }
    if let Some(path) = &cli.frequency {
        let year = bess_models::FrequencyYear::load(path)?;
        println!("replaying frequency year {:#018x}", year.hash());
        bess_models::replay_frequency(year).map_err(|_| "a frequency year is already attached")?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(message) = attach_replays(&cli) {
        return fail(&message);
    }

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
    let published = RunSpec {
        market: cli.market,
        years: cli.years,
        ..RunSpec::default()
    };
    if (cli.write || cli.check) && spec != published {
        return fail(
            "--write and --check publish the annual run, so they only accept the \
             default --days, --seed and --start. Drop them, or drop --write/--check \
             to explore a different run.",
        );
    }

    let started = Instant::now();
    let (kpis, series, period_rows, trajectory) = measure(spec, cli.quiet);
    let elapsed = started.elapsed().as_secs_f64();
    print_report(&kpis, elapsed);

    if let Err(message) = write_side_outputs(&cli, &kpis, &trajectory, &period_rows) {
        return fail(&message);
    }

    if cli.write && cli.market {
        let record = cli.record_path();
        return match write_file(&record, &report::to_json(&kpis)) {
            Ok(()) => {
                println!("wrote {}", record.display());
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    if cli.check && cli.market {
        let record = cli.record_path();
        return match check_market(&kpis, &record) {
            Ok(()) => {
                println!("the market year's gate holds and its record is current");
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    if cli.write {
        return match write_published(&kpis, &series, &cli.record, &cli.calibration, &cli.series) {
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
        if cli.years > 1 {
            if let Err(message) = check_trajectory(&trajectory, Path::new(TRAJECTORY_PATH)) {
                return fail(&message);
            }
        }
        return match check(&kpis, &cli.record, &cli.calibration, &cli.series) {
            Ok(()) => {
                println!("every gate holds and the published record is current");
                ExitCode::SUCCESS
            }
            Err(message) => fail(&message),
        };
    }

    ExitCode::SUCCESS
}

/// The optional outputs a run writes beside the report: the JSON copy,
/// the trajectory, the closed periods.
fn write_side_outputs(
    cli: &Cli,
    kpis: &Kpis,
    trajectory: &aging::AgingTrajectory,
    period_rows: &[periods::ClosedPeriod],
) -> Result<(), String> {
    if let Some(path) = &cli.json {
        write_file(path, &report::to_json(kpis))?;
        println!("wrote {}", path.display());
    }
    if let Some(path) = &cli.trajectory {
        write_file(path, &aging::to_json(trajectory))?;
        println!("wrote {}", path.display());
    }
    if cli.write && !cli.market && cli.years > 1 {
        let path = Path::new(TRAJECTORY_PATH);
        write_file(path, &aging::to_json(trajectory))?;
        println!("wrote {} ({} years)", path.display(), cli.years);
    }
    if let Some(path) = &cli.periods {
        write_file(path, &periods::to_csv(period_rows))?;
        println!("wrote {} periods to {}", period_rows.len(), path.display());
    }
    Ok(())
}

/// Run the plant, reporting progress on stderr so a redirected stdout stays
/// machine-readable.
fn measure(spec: RunSpec, quiet: bool) -> run::Measured {
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
fn write_published(
    kpis: &Kpis,
    series: &series::StudySeries,
    record: &Path,
    calibration: &Path,
    series_path: &Path,
) -> Result<(), String> {
    write_file(record, &report::to_json(kpis))?;
    write_file(series_path, &report::series_to_json(series))?;
    render_document(record, calibration)
}

/// The trajectory's gates (M5 phase 4), and the committed trajectory came
/// from this run and says what this run says, year for year.
fn check_trajectory(measured: &aging::AgingTrajectory, path: &Path) -> Result<(), String> {
    let mut failures = bands::check_aging(measured);
    match read_file(path).and_then(|text| aging::from_json(&text)) {
        Ok(published) if published != *measured => failures.push(format!(
            "{} is stale; regenerate with `cargo run --release -p bess-bench -- --write --years {}`",
            path.display(),
            measured.years.len()
        )),
        Ok(_) => {}
        Err(message) => failures.push(message),
    }
    if failures.is_empty() {
        println!("the projected decade holds its bands and its record is current");
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Render the generated block from whatever the record on disk says.
fn render_document(record: &Path, calibration: &Path) -> Result<(), String> {
    let published = read_record(record)?;
    let document = read_file(calibration)?;
    let spliced = report::splice(&document, &report::render(&published))
        .map_err(|err| format!("{}: {err}", calibration.display()))?;
    write_file(calibration, &spliced)
}

/// The market year's gate (M4 phase 4): the bands, the revenue band among
/// them, and its own record current; no series, no generated block.
fn check_market(kpis: &Kpis, record: &Path) -> Result<(), String> {
    let mut failures = bands::check(kpis);
    match read_record(record) {
        Ok(published) => {
            let drift = report::drift(kpis, &published);
            if !drift.is_empty() {
                failures.push(format!(
                    "{} is stale ({} figures moved); regenerate with \
                     `cargo run --release -p bess-bench -- --market --write`:\n    {}",
                    record.display(),
                    drift.len(),
                    drift.join("\n    ")
                ));
            }
        }
        Err(message) => failures.push(message),
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The CI gate: bands hold, the record is current, the document matches it.
fn check(kpis: &Kpis, record: &Path, calibration: &Path, series_path: &Path) -> Result<(), String> {
    let mut failures = bands::check(kpis);

    // The series is chart material, not a gate: comparing seven hundred
    // samples field by field would put a wall of diff in front of every
    // physics change for no extra safety. What is checked is that it came
    // from the same run as the record, which is what catches a regeneration
    // that refreshed one and forgot the other.
    match read_file(series_path).and_then(|text| report::series_from_json(&text)) {
        Ok(series) if series.run != kpis.run => failures.push(format!(
            "{} was produced by a different run than the record; regenerate with \
             `cargo run --release -p bess-bench -- --write`",
            series_path.display()
        )),
        Ok(_) => {}
        Err(message) => failures.push(message),
    }

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
