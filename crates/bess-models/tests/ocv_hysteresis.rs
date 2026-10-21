//! M2.5 phase 1 gate: the rested voltage of a rack lies on two curves, and
//! the gap between them is the one the pinned characterization measured.
//!
//! Barai et al. 2015 (J. Power Sources 295, a 20 Ah LFP pouch at 25 C, 4 %
//! steps at 1C, 4 h rests) put the charge-minus-discharge rest-voltage gap
//! at 38 mV near 5 % SoC, the maximum of the curve. The plateau level is
//! not stated in the text and is held to a sanity bound (see
//! CALIBRATION.md). The second test is the switch-off proof: with the
//! transition rate at zero the plant is the pre-phase plant, byte for byte.

use bess_core::checkpoint::state_digest;
use bess_core::config::PlantConfig;
use bess_core::digest::fnv1a_64;
use bess_core::state::RackState;
use bess_core::traits::CellModel;
use bess_core::{Simulation, SiteState};
use bess_models::cell::Ecm1Rc;
use bess_models::{gw01_models, gw01_weather, BasicBms};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;
/// Pinned gate: full gap at 5 % SoC, mV per cell, sourced (Barai 2015).
const GAP_AT_5PCT_MV: (f64, f64) = (32.0, 44.0);
/// Sanity bound: full gap at 50 % SoC, mV per cell.
const GAP_AT_50PCT_MV: (f64, f64) = (10.0, 25.0);
/// About 0.2C on a 314 Ah rack.
const SLOW_CURRENT_A: f64 = 63.0;

fn rack_at(soc: f64) -> RackState {
    let cfg = PlantConfig::gw01();
    let mut r = SiteState::new(&cfg, 7, START_UNIX_S).blocks[0].containers[0].racks[0].clone();
    r.soc = soc;
    r.resistance_scale = 1.0;
    r
}

/// Move a rack to `target` from the far side and let it rest; returns the
/// rested terminal voltage. Charging approaches from below, discharging
/// from above, both by more than a transition's worth of throughput.
fn rested_voltage_v(model: &Ecm1Rc, target: f64, charging: bool) -> (f64, f64) {
    let cfg = PlantConfig::gw01().rack;
    let start = if charging {
        target - 0.10
    } else {
        target + 0.10
    };
    let mut r = rack_at(start.clamp(0.0, 1.0));
    let p_w = SLOW_CURRENT_A * cfg.nominal_v() * if charging { -1.0 } else { 1.0 };
    let mut guard = 0u32;
    while if charging {
        r.soc < target
    } else {
        r.soc > target
    } {
        model.step_rack(&mut r, &cfg, p_w, 1.0);
        guard += 1;
        assert!(guard < 4 * 3600, "never reached SoC {target}");
    }
    for _ in 0..900 {
        model.step_rack(&mut r, &cfg, 0.0, 1.0);
    }
    (r.voltage_v, r.soc)
}

fn gap_mv_per_cell(model: &Ecm1Rc, soc: f64) -> f64 {
    let cells = PlantConfig::gw01().rack.cells_series as f64;
    let (v_ch, s_ch) = rested_voltage_v(model, soc, true);
    let (v_dis, s_dis) = rested_voltage_v(model, soc, false);
    assert!(
        (s_ch - s_dis).abs() < 1.0e-3,
        "branches rested at {s_ch} and {s_dis}"
    );
    (v_ch - v_dis) / cells * 1000.0
}

#[test]
fn rested_voltage_lies_on_two_branches_with_the_pinned_gap() {
    let model = Ecm1Rc::lfp_314ah_rack();
    let knee = gap_mv_per_cell(&model, 0.05);
    println!("gap at 5 % SoC: {knee:.1} mV per cell");
    assert!(
        (GAP_AT_5PCT_MV.0..=GAP_AT_5PCT_MV.1).contains(&knee),
        "gap at 5 % SoC {knee:.1} mV outside the sourced band {GAP_AT_5PCT_MV:?}; see CALIBRATION.md"
    );
    let plateau = gap_mv_per_cell(&model, 0.50);
    println!("gap at 50 % SoC: {plateau:.1} mV per cell");
    assert!(
        (GAP_AT_50PCT_MV.0..=GAP_AT_50PCT_MV.1).contains(&plateau),
        "gap at 50 % SoC {plateau:.1} mV outside the sanity bound {GAP_AT_50PCT_MV:?}"
    );
    assert!(knee > plateau, "the gap should peak at the low knee");
}

#[test]
fn without_hysteresis_the_two_branches_coincide() {
    let model = Ecm1Rc::lfp_314ah_rack_without_hysteresis();
    let gap = gap_mv_per_cell(&model, 0.50);
    assert!(
        gap.abs() < 0.5,
        "switched off, the branches still differ by {gap} mV"
    );
}

/// Energy conservation closes exactly through the hysteresis work: what
/// the rack stored, delivered and dissipated over a charge, rest, discharge,
/// rest cycle sums to the RC branch's few watt-hours and nothing more.
#[test]
fn the_energy_books_close_through_a_hysteresis_cycle() {
    let model = Ecm1Rc::lfp_314ah_rack();
    let cfg = PlantConfig::gw01().rack;
    let mut r = rack_at(0.5);
    let e0 = model.stored_energy_wh(&r, &cfg);
    let mut terminal_wh = 0.0;
    let mut heat_wh = 0.0;
    let p_w = 150.0e3;
    let plan = [
        (-p_w, 1800),
        (0.0, 600),
        (p_w, 1800),
        (0.0, 600),
        (-p_w, 900),
    ];
    for (p, ticks) in plan {
        for _ in 0..ticks {
            let res = model.step_rack(&mut r, &cfg, p, 1.0);
            terminal_wh += res.p_dc_w / 3600.0;
            heat_wh += res.heat_w / 3600.0;
        }
    }
    let e1 = model.stored_energy_wh(&r, &cfg);
    let residual = (e0 - e1) - (terminal_wh + heat_wh);
    assert!(residual.abs() < 5.0, "residual {residual} Wh");
    assert!(heat_wh > 0.0);
}

/// The switch-off proof. With the transition rate at zero every rack's
/// state stays at its initial zero, and the tree, with that one field cut
/// out of its serialization, hashes to the golden digest the plant had
/// before this phase (M2 phase 4 PR1, `determinism.rs` history). Phase 2
/// added the estimator beside it; the proof runs with the perfect one and
/// cuts its three fields out as well, so the digest still reads phase 1's
/// claim.
#[test]
fn switched_off_the_plant_is_the_pre_phase_plant_byte_for_byte() {
    const PRE_PHASE_GOLDEN: u64 = 0xf3fb_8ad4_75c9_980f;
    let cfg = PlantConfig::gw01();
    let mut models = gw01_models(&cfg);
    models.cell = Box::new(Ecm1Rc::lfp_314ah_rack_without_hysteresis());
    models.bms = Box::new(BasicBms::with_perfect_estimator());
    let mut sim = Simulation::new(cfg, models, 42, START_UNIX_S);
    let weather = gw01_weather();
    for _ in 0..4 * 3600 {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
    }
    assert!(sim.state().racks().all(|r| r.hysteresis_v == 0.0));
    let json = serde_json::to_string(sim.state()).unwrap();
    let stripped = strip_estimator_fields(&json.replace("\"hysteresis_v\":0.0,", ""));
    assert_ne!(json, stripped, "the field was not in the serialization");
    let digest = fnv1a_64(stripped.as_bytes());
    assert_eq!(
        digest, PRE_PHASE_GOLDEN,
        "with hysteresis off the plant moved: {digest:#018x}"
    );
    // And the digest with the field in place is not the pre-phase one, so
    // the test cannot pass by the field going missing.
    assert_ne!(state_digest(sim.state()), PRE_PHASE_GOLDEN);
}

/// Cut `soc_reported`, `sensor_offset_a` and `sensor_gain` out of a
/// serialized tree: the three fields phase 2 added to every rack, which
/// sit together in field order.
fn strip_estimator_fields(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut rest = json;
    while let Some(start) = rest.find("\"soc_reported\":") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let gain = after.find("\"sensor_gain\":").expect("fields sit together");
        let end = gain + after[gain..].find(',').expect("a field follows") + 1;
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}
