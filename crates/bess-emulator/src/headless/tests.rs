//! A headless run plays the file for its length, the digest is the run's
//! and nothing else's, and a record that drifted says which field moved.

use bess_core::PlantConfig;
use bess_scenario::{load, RunParams};

use super::{assert, record, run, Record};

const STORY: &str = "
schema: 1
name: headless
seed: 7
start: 2026-07-14T14:00:00Z
length: PT10S
events:
  - at: PT0S
    target: block[2].container[0].hvac
    fault: failure
  - at: PT5S
    surface: mqtt
    fault: dropout
    duration: PT1M
";

/// The file's seed and start where it states them, as the command line
/// would resolve them.
fn digest(text: &str) -> Result<Record, String> {
    let scenario = load(text.as_bytes(), &PlantConfig::gw01()).unwrap();
    let params = RunParams {
        seed: scenario.seed.unwrap_or(42),
        start_unix_s: scenario.start_unix_s.unwrap_or(0),
        speed: 60.0,
        age_years: 0,
    };
    run(&scenario.resolve(params))
}

#[test]
fn the_run_plays_the_length_and_the_digest_is_the_physics_alone() {
    let a = digest(STORY).unwrap();
    assert_eq!((a.ticks, a.seed, a.start_unix_s), (10, 7, 1_784_037_600));
    assert_eq!(a.scenario, "headless");
    assert!(a.content_hash.starts_with("0x") && a.content_hash.len() == 18);
    assert!(a.state_digest.starts_with("0x") && a.state_digest.len() == 18);
    assert!(a.event_log.count >= 1, "the failure is marked in the log");
    assert_eq!(a.engine_version, bess_core::version());
    assert_eq!(a, digest(STORY).unwrap());

    // Another seed is another run.
    let reseeded = digest(&STORY.replace("seed: 7", "seed: 8")).unwrap();
    assert_ne!(reseeded.state_digest, a.state_digest);
    // The data fault is not the kernel's: without it, the same log and
    // meter. The tree's digest still moves, since the tree carries the
    // file's identity (phase 4 D8), which is another file now.
    let (story, _) = STORY.split_once("  - at: PT5S").unwrap();
    let quiet = digest(story).unwrap();
    assert_eq!(quiet.event_log, a.event_log);
    assert_eq!(quiet.revenue_periods_closed, a.revenue_periods_closed);
    assert_ne!(quiet.content_hash, a.content_hash);
    assert_ne!(quiet.state_digest, a.state_digest);
}

#[test]
fn a_file_without_a_length_cannot_run_headless() {
    let err = digest("schema: 1\nname: open\nevents: []\n").unwrap_err();
    assert!(err.contains("`length`"), "{err}");
}

#[test]
fn a_record_holds_and_a_drift_names_its_field() {
    let a = digest(STORY).unwrap();
    let dir = std::env::temp_dir().join(format!("bess-headless-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("headless.digest.json");
    record(&path, &a).unwrap();
    assert_eq!(assert(&path, &a), Ok(()));

    let mut moved = a.clone();
    moved.state_digest = "0x0000000000000000".to_owned();
    moved.event_log.count += 1;
    let why = assert(&path, &moved).unwrap_err();
    assert!(why.contains("state_digest: recorded"), "{why}");
    assert!(why.contains("event_log.count: recorded"), "{why}");
    assert!(!why.contains("seed:"), "{why}");

    let missing = assert(&dir.join("nothing.json"), &a).unwrap_err();
    assert!(missing.contains("--record"), "{missing}");
    std::fs::remove_dir_all(&dir).unwrap();
}
