//! The library against the public incident record (phase 5 design D4):
//! every case loads, carries a tag and a length, and the failure cases'
//! composition stays within the gate around the EPRI snapshot pinned in
//! CALIBRATION.md.

use std::path::PathBuf;

use bess_core::PlantConfig;
use bess_scenario::{load, Taxonomy};

/// The snapshot: EPRI, "Insights from EPRI's Battery Energy Storage
/// Systems (BESS) Failure Incident Database: Analysis of Failure Root
/// Cause", white paper 3002030360, May 2024, figure 6, the 26 incidents
/// with a classified failed element (two with dual causes, so 28
/// classifications). Retrieved 2026-10-15. Shares in percent.
const EPRI_CONTROLS_PCT: f64 = 46.0;
const EPRI_BOS_PCT: f64 = 43.0;
const EPRI_CELL_PCT: f64 = 11.0;

/// How far a class may sit from the snapshot, percentage points. With a
/// dozen failure cases one case is about nine points, so the band admits
/// one case of drift and refuses two.
const TOLERANCE_PCT: f64 = 15.0;

fn library() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

/// Every case in the directory, loaded against the reference site.
fn cases() -> Vec<(String, Taxonomy, u64)> {
    let cfg = PlantConfig::gw01();
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(library()).expect("scenarios/ exists") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "yaml") {
            let bytes = std::fs::read(&path).unwrap();
            let scenario = load(&bytes, &cfg).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            assert_eq!(scenario.id.name, name, "a case is named after its file");
            let taxonomy = scenario
                .taxonomy
                .unwrap_or_else(|| panic!("{name}: a library case carries a taxonomy tag"));
            let length_s = scenario
                .length_s
                .unwrap_or_else(|| panic!("{name}: a library case states its length"));
            assert!(
                path.with_extension("digest.json").exists(),
                "{name}: a library case has its record beside it"
            );
            cases.push((name, taxonomy, length_s));
        }
    }
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}

#[test]
fn every_case_loads_with_a_tag_a_length_and_a_record() {
    let cases = cases();
    assert!(cases.len() >= 12, "a dozen cases at least: {}", cases.len());
}

/// The failure mix is the public record's: balance of system and controls
/// form the majority, cells are the smallest class and never absent, and
/// each class sits within the band of its EPRI share.
#[test]
fn the_failure_cases_follow_the_epri_shares() {
    let cases = cases();
    let count = |t: Taxonomy| cases.iter().filter(|c| c.1 == t).count();
    let (bos, controls, cell) = (
        count(Taxonomy::Bos),
        count(Taxonomy::Controls),
        count(Taxonomy::Cell),
    );
    let failures = bos + controls + cell;
    let share = |n: usize| 100.0 * n as f64 / failures as f64;
    let composition = format!(
        "{failures} failure cases: bos {bos} ({:.0}%), controls {controls} ({:.0}%), \
         cell {cell} ({:.0}%); EPRI snapshot bos {EPRI_BOS_PCT}%, controls \
         {EPRI_CONTROLS_PCT}%, cell {EPRI_CELL_PCT}%, band {TOLERANCE_PCT} points",
        share(bos),
        share(controls),
        share(cell)
    );
    assert!(cell >= 1, "the cell class is never empty: {composition}");
    assert!(
        cell < bos && cell < controls,
        "cells are the smallest class: {composition}"
    );
    for (name, n, snapshot) in [
        ("bos", bos, EPRI_BOS_PCT),
        ("controls", controls, EPRI_CONTROLS_PCT),
        ("cell", cell, EPRI_CELL_PCT),
    ] {
        assert!(
            (share(n) - snapshot).abs() <= TOLERANCE_PCT,
            "{name} is outside the band: {composition}"
        );
    }
    let outside = cases
        .iter()
        .filter(|c| matches!(c.1, Taxonomy::Calendar | Taxonomy::Maintenance))
        .count();
    assert!(
        outside >= 2,
        "calendar and maintenance cases sit outside the gate: {composition}"
    );
}

/// The embedded library is the directory: the same names, the same bytes,
/// and every case with a headline of a readable length.
#[test]
fn the_embedded_library_is_the_directory_byte_for_byte() {
    use bess_scenario::library::{CASES, HEADLINE_MAX};
    let on_disk: Vec<String> = cases().into_iter().map(|(name, ..)| name).collect();
    let mut embedded: Vec<String> = CASES.iter().map(|c| c.name.to_owned()).collect();
    embedded.sort();
    assert_eq!(
        embedded, on_disk,
        "scenarios/ and library::CASES list different cases"
    );
    for case in CASES {
        let bytes = std::fs::read(library().join(format!("{}.yaml", case.name))).unwrap();
        assert_eq!(
            case.yaml.as_bytes(),
            bytes,
            "{}: embedded bytes differ",
            case.name
        );
        let headline = case.headline();
        assert!(!headline.is_empty(), "{}: no header comment", case.name);
        assert!(
            headline.len() <= HEADLINE_MAX,
            "{}: headline is {} characters",
            case.name,
            headline.len()
        );
    }
}
