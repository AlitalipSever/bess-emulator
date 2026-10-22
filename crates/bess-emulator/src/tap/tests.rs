//! The shared tap against the map: which windows are open when, which
//! points they select, and what a point reads.

use bess_core::PlantConfig;
use bess_scenario::{load, RunParams, Surface};

use super::Taps;
use crate::map::{self, Point};

/// One of nearly every kind, on both surfaces.
const STORY: &[u8] = br"
schema: 1
name: every-kind
events:
  - at: PT1M
    surface: mqtt
    fault: dropout
    duration: PT1M
  - at: PT2M
    surface: mqtt
    fault: backfill_burst
  - at: PT0S
    surface: modbus
    fault: freeze
    duration: PT10M
    points: block[2].pcs
  - at: PT0S
    surface: modbus
    fault: unit_error
    points: site
  - at: PT5M
    surface: mqtt
    fault: nan_burst
    duration: PT1M
  - at: PT0S
    surface: mqtt
    fault: chatter
    duration: PT1H
    rate_per_min: 2
    points: block[3]
  - at: PT0S
    surface: mqtt
    fault: timestamp_drift
    duration: PT2H
    drift_s_per_h: 36
";

fn taps() -> (Taps, Vec<Point>) {
    let cfg = PlantConfig::gw01();
    let points = map::build_points(&cfg);
    let params = RunParams {
        seed: 1,
        start_unix_s: 0,
        speed: 1.0,
        age_years: 0,
    };
    let resolved = load(STORY, &cfg).unwrap().resolve(params);
    (Taps::new(&resolved, &points), points)
}

fn index(points: &[Point], name: &str) -> usize {
    points.iter().position(|p| p.name == name).unwrap()
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

#[test]
fn windows_open_and_close_on_their_surface() {
    let (taps, _) = taps();
    assert!(!taps.dropout(Surface::Mqtt, 59));
    assert!(taps.dropout(Surface::Mqtt, 60));
    assert!(taps.dropout(Surface::Mqtt, 119));
    assert!(!taps.dropout(Surface::Mqtt, 120));
    assert!(!taps.dropout(Surface::Modbus, 60));
    // A unit error without a duration runs to the end; a burst is an
    // instant that is due once its tick has passed.
    assert!(taps.any_open(Surface::Modbus, 1_000_000));
    assert_eq!(taps.backfills_due(Surface::Mqtt, 119).count(), 0);
    assert_eq!(taps.backfills_due(Surface::Mqtt, 120).count(), 1);
    assert_eq!(taps.backfills_due(Surface::Modbus, 120).count(), 0);
}

#[test]
fn a_selector_reaches_its_points_and_a_kind_its_classes() {
    let (taps, points) = taps();
    let window = |index: usize| taps.windows().iter().find(|w| w.index == index).unwrap();
    let at = |name: &str| index(&points, name);

    let freeze = window(2);
    assert!(freeze.selected[at("block02.pcs.p_ac_kw")]);
    assert!(freeze.selected[at("block02.pcs.state")]);
    assert!(!freeze.selected[at("block02.soc_pct")]);
    assert!(!freeze.selected[at("site.soc_pct")]);

    let unit = window(3);
    assert!(unit.selected[at("site.poi.active_power_w")]);
    assert!(
        !unit.selected[at("site.alarm_bits")],
        "a word is not a quantity"
    );
    assert!(
        !unit.selected[at("control.site_setpoint_w")],
        "never the operator's registers"
    );

    let nan = window(4);
    assert!(nan.selected[at("block19.pcs.p_dc_kw")], "`*` by default");
    assert!(!nan.selected[at("block19.alarm_bits")]);

    let chatter = window(5);
    assert!(chatter.selected[at("block03.alarm_bits")]);
    assert!(chatter.selected[at("block03.block_alarm_bits")]);
    assert!(
        !chatter.selected[at("block03.racks_derated")],
        "a count is not a word"
    );
    assert!(!chatter.selected[at("block03.pcs.p_ac_kw")]);
    assert!(!chatter.selected[at("block04.alarm_bits")]);

    assert!(
        window(0).selected.iter().all(|s| !s),
        "a dropout is the whole surface"
    );
}

#[test]
fn a_point_reads_frozen_then_scaled_then_nan() {
    let (mut taps, points) = taps();
    let p = index(&points, "block02.pcs.p_ac_kw");
    // Until the freeze has captured, the truth.
    assert!(same(taps.value(Surface::Modbus, p, 5, 12.5), 12.5));
    let mut values = vec![0.0; points.len()];
    values[p] = 40.0;
    taps.open(2, values);
    assert!(same(taps.value(Surface::Modbus, p, 5, 12.5), 40.0));
    assert!(
        same(taps.value(Surface::Modbus, p, 600, 12.5), 12.5),
        "after the window"
    );

    let site = index(&points, "site.poi.active_power_w");
    assert!(same(taps.value(Surface::Modbus, site, 5, -2.0), -2000.0));
    assert!(
        same(taps.value(Surface::Mqtt, site, 5, -2.0), -2.0),
        "the other surface is honest"
    );
    assert!(taps.value(Surface::Mqtt, site, 300, -2.0).is_nan());
    assert!(same(taps.value(Surface::Mqtt, site, 360, -2.0), -2.0));
}

#[test]
fn a_flap_is_a_period_and_a_drift_is_a_ramp() {
    let (taps, points) = taps();
    let word = index(&points, "block03.alarm_bits");
    // Two flaps a minute: a 30 s period, flipped through its second half.
    let flipped: Vec<bool> = [0, 14, 15, 29, 30, 44, 45]
        .iter()
        .map(|&t| taps.flipped(Surface::Mqtt, word, t))
        .collect();
    assert_eq!(flipped, [false, false, true, true, false, false, true]);
    assert!(!taps.flipped(Surface::Modbus, word, 15));
    assert!(
        !taps.flipped(Surface::Mqtt, word, 3600),
        "the window has closed"
    );

    assert!(same(taps.drift_s(Surface::Mqtt, 0), 0.0));
    assert!(same(taps.drift_s(Surface::Mqtt, 1800), 18.0));
    assert!(
        same(taps.drift_s(Surface::Mqtt, 7200), 0.0),
        "the clock steps back when the window closes"
    );
    assert!(same(taps.drift_s(Surface::Modbus, 1800), 0.0));
}
