//! The headless run: play a scenario for its length as fast as the machine
//! allows, no socket bound, take the digest of what it left, and hold it
//! against a committed record (phase 5 design D5). `--record` writes the
//! record; `--assert` compares and exits nonzero on drift, which is what CI
//! runs over the library.
//!
//! The digest covers the final tree and the event log, count and hash, and
//! names the revenue meter's closed periods, the number a calendar case is
//! about. Intermediate trajectories are deliberately not held: the M1
//! lesson is to hold the contract, not seven hundred samples. The engine
//! version is part of the record, so a release regenerates every digest
//! loudly, as the annual record is.

use std::fmt::Write as _;
use std::path::Path;

use bess_core::checkpoint::state_digest;
use bess_core::PlantConfig;
use bess_models::{gw01_models, gw01_weather};
use bess_scenario::{Player, Resolved};
use serde::{Deserialize, Serialize};

/// What a headless run leaves behind: the committed record of a case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The scenario's name.
    pub scenario: String,
    /// FNV-1a 64 of the file's bytes, `0x` and sixteen hex digits.
    pub content_hash: String,
    /// The kernel that produced it.
    pub engine_version: String,
    /// The seed the run used.
    pub seed: u64,
    /// Unix seconds UTC of tick 0.
    pub start_unix_s: i64,
    /// Ticks played: the file's `length`.
    pub ticks: u64,
    /// `bess_core::checkpoint::state_digest` of the final tree.
    pub state_digest: String,
    /// The event log's footprint.
    pub event_log: EventLog,
    /// Quarter-hours the fiscal meter closed.
    pub revenue_periods_closed: u32,
    /// Years the plant was aged at commissioning (M5 phase 2); absent on
    /// a fresh plant. The age is in the tree the digest covers; the field
    /// says why the digest is what it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_years: Option<u32>,
    /// FNV-1a 64 of the replayed frequency year's samples when one was
    /// attached (`--frequency`, M4 phase 1); absent on the synthetic
    /// series. A record made with a file is reproducible by whoever holds
    /// the same file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    /// FNV-1a 64 of the replayed balancing year's bytes when one was
    /// attached (`--balancing`, M4 phase 3); absent on the bundled
    /// annual figures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balancing: Option<String>,
    /// FNV-1a 64 of the replayed aFRR request year's samples when one
    /// was attached (`--afrr`); absent on the synthetic request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afrr: Option<String>,
}

/// The kernel's event log at the end: how many, and the hash of all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLog {
    /// Events emitted since tick 0.
    pub count: u64,
    /// FNV-1a over every event, in order.
    pub digest: String,
}

fn hex(value: u64) -> String {
    format!("{value:#018x}")
}

/// Play the scenario for its length and take the digest.
pub fn run(resolved: &Resolved) -> Result<Record, String> {
    let ticks = resolved
        .ticks
        .ok_or("the scenario states no `length`; a headless run plays a file for its length")?;
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = resolved.simulation(cfg, models);
    let mut player = Player::new(resolved);
    let weather = gw01_weather();
    for _ in 0..ticks {
        if let Some(refused) = player.fire_due(&mut sim).refused {
            return Err(refused.to_string());
        }
        sim.step(&weather.inputs_at(sim.unix_time_s()));
    }
    let state = sim.state();
    Ok(Record {
        scenario: resolved.id.name.clone(),
        content_hash: hex(resolved.id.content_hash),
        engine_version: bess_core::version().to_owned(),
        seed: resolved.params.seed,
        start_unix_s: resolved.params.start_unix_s,
        ticks,
        state_digest: hex(state_digest(state)),
        event_log: EventLog {
            count: state.event_log.count,
            digest: hex(state.event_log.digest),
        },
        revenue_periods_closed: state.substation.revenue.periods_closed,
        age_years: (resolved.params.age_years > 0).then_some(resolved.params.age_years),
        frequency: weather.frequency().map(|year| hex(year.hash())),
        balancing: bess_models::replayed_balancing().map(|year| hex(year.hash())),
        afrr: weather.afrr().map(|year| hex(year.hash())),
    })
}

/// Write the record.
pub fn record(path: &Path, digest: &Record) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(digest).expect("a digest is plain data");
    text.push('\n');
    std::fs::write(path, text).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

/// Compare with the record at `path`: every field that differs, in one
/// message, or nothing.
pub fn assert(path: &Path, digest: &Record) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        format!(
            "cannot read the record {}: {err}; `--record` writes one",
            path.display()
        )
    })?;
    let recorded: Record = serde_json::from_str(&text)
        .map_err(|err| format!("{} is not a digest: {err}", path.display()))?;
    let differences = differences(&recorded, digest);
    if differences.is_empty() {
        return Ok(());
    }
    let mut why = format!("{} drifted from {}:\n", digest.scenario, path.display());
    for (field, recorded, got) in differences {
        let _ = writeln!(why, "  {field}: recorded {recorded}, got {got}");
    }
    why.push_str(
        "a deliberate physics change regenerates the record with --record; \
         anything else is a regression",
    );
    Err(why)
}

fn differences(recorded: &Record, got: &Record) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    let mut field = |name: &'static str, a: String, b: String| {
        if a != b {
            out.push((name, a, b));
        }
    };
    field("scenario", recorded.scenario.clone(), got.scenario.clone());
    field(
        "content_hash",
        recorded.content_hash.clone(),
        got.content_hash.clone(),
    );
    field(
        "engine_version",
        recorded.engine_version.clone(),
        got.engine_version.clone(),
    );
    field("seed", recorded.seed.to_string(), got.seed.to_string());
    field(
        "start_unix_s",
        recorded.start_unix_s.to_string(),
        got.start_unix_s.to_string(),
    );
    field("ticks", recorded.ticks.to_string(), got.ticks.to_string());
    field(
        "age_years",
        format!("{:?}", recorded.age_years),
        format!("{:?}", got.age_years),
    );
    field(
        "state_digest",
        recorded.state_digest.clone(),
        got.state_digest.clone(),
    );
    field(
        "event_log.count",
        recorded.event_log.count.to_string(),
        got.event_log.count.to_string(),
    );
    field(
        "event_log.digest",
        recorded.event_log.digest.clone(),
        got.event_log.digest.clone(),
    );
    field(
        "revenue_periods_closed",
        recorded.revenue_periods_closed.to_string(),
        got.revenue_periods_closed.to_string(),
    );
    out
}

#[cfg(test)]
mod tests;
