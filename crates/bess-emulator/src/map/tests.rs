//! The table held to its contract: addresses, names, the version, and the
//! site points whose values a wiring error could swap.

use std::path::Path;

use bess_core::config::PlantConfig;
use bess_core::state::{AuxPower, SiteState};

use super::*;

#[test]
fn addresses_do_not_overlap_and_fit_the_banks() {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let mut input_used = vec![false; INPUT_BANK_LEN];
    let mut holding_used = vec![false; HOLDING_BANK_LEN];
    for p in &points {
        let used = match p.space {
            Space::Input => &mut input_used,
            Space::Holding => &mut holding_used,
        };
        for w in 0..p.encoding.words() {
            let a = (p.addr + w) as usize;
            assert!(a < used.len(), "{}: address {a} out of bank", p.name);
            assert!(!used[a], "{}: address {a} overlaps", p.name);
            used[a] = true;
        }
    }
}

#[test]
fn names_are_unique() {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let mut names: Vec<&str> = points.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), points.len());
}

#[test]
fn banks_reflect_state_values() {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let state = SiteState::new(&cfg, 1, 0);
    let mut input = vec![0u16; INPUT_BANK_LEN];
    let mut holding = vec![0u16; HOLDING_BANK_LEN];
    write_banks(&points, &state, &mut input, &mut holding);
    // Frequency register: 50.0 Hz x1000.
    assert_eq!(input[5], 50_000);
    // POI voltage: 110 kV x100.
    assert_eq!(input[4], 11_000);
    // Site SoC: 50% x100 within spread tolerance.
    assert!((4_900..=5_100).contains(&input[6]), "soc reg {}", input[6]);
}

/// A house load with five deliberately different values, so a register
/// carrying the wrong item cannot hide behind a correct total.
fn itemized_state(cfg: &PlantConfig) -> SiteState {
    let mut state = SiteState::new(cfg, 1, 0);
    state.aux = AuxPower {
        hvac_w: 41_234.6,
        bms_w: 17_232.4,
        pcs_standby_w: 6_795.5,
        controls_w: 15_000.3,
        lighting_and_safety_w: 9_999.7,
    };
    state.substation.aux_power_w = state.aux.total_w();
    state
}

/// Each item register carries its own item.
///
/// The sum test below cannot see this: swapping two items leaves the
/// total untouched, and the inventory would publish wrong values under
/// right names. Addressability is the whole point of itemizing, so every
/// address is pinned to its own quantity here.
#[test]
fn every_house_load_item_register_carries_its_own_item() {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let state = itemized_state(&cfg);
    let input = input_bank(&points, &state);

    for (addr, name, watts) in [
        (32, "hvac", state.aux.hvac_w),
        (34, "bms", state.aux.bms_w),
        (36, "pcs_standby", state.aux.pcs_standby_w),
        (38, "controls", state.aux.controls_w),
        (40, "lighting_and_safety", state.aux.lighting_and_safety_w),
    ] {
        let expected = watts.round() as u32;
        let read = read_u32(&input, addr);
        assert_eq!(read, expected, "site.aux.{name} at {addr} reads {read} W");
    }
}

/// The auxiliary identity the kernel guards internally has to survive the
/// projection: whoever reads the five item registers and adds them up
/// must land on the metered total register. Deliberately awkward values,
/// so the assertion exercises rounding rather than round numbers.
#[test]
fn the_house_load_items_add_up_to_the_metered_total() {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let state = itemized_state(&cfg);
    let input = input_bank(&points, &state);

    let total = read_u32(&input, 22);
    let items = read_u32(&input, 32)
        + read_u32(&input, 34)
        + read_u32(&input, 36)
        + read_u32(&input, 38)
        + read_u32(&input, 40);
    // Five items and the total each round to the nearest watt, so the
    // sums can differ by at most 3 W. Anything larger is a wiring error.
    assert!(
        total.abs_diff(items) <= 3,
        "items sum to {items} W, total register reads {total} W"
    );
}

/// FNV-1a over one byte run, the construction the state digest uses.
/// Written out here rather than borrowed from a hasher, because a pinned
/// constant has to mean the same thing on every toolchain forever.
fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    let mut h = hash;
    for byte in bytes {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Fingerprint of the published contract: every field of every point that
/// a consumer can depend on, in table order.
fn map_digest(points: &[Point]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325;
    for p in points {
        h = fnv1a(h, p.name.as_bytes());
        h = fnv1a(h, p.unit.as_bytes());
        h = fnv1a(h, p.class.as_str().as_bytes());
        h = fnv1a(h, p.encoding.as_str().as_bytes());
        h = fnv1a(h, &p.scale.to_bits().to_le_bytes());
        h = fnv1a(h, &p.addr.to_le_bytes());
        h = fnv1a(
            h,
            &[u8::from(p.space == Space::Holding), u8::from(p.writable)],
        );
    }
    h
}

/// Digest of the map at [`MAP_VERSION`] 0.3.0.
const MAP_DIGEST: u64 = 0x2959_ef69_5ecb_0a15;

/// A version number nobody is forced to move is decoration.
///
/// CI compares the committed CSV against a fresh dump, and the dump
/// regenerates the version line along with the rows, so adding a point
/// and forgetting the version would pass green. This digest is what makes
/// the version deliberate: a changed contract fails here, and whoever
/// updates the constant has to decide what the change was worth. Minor
/// for points added at unused addresses, major for anything that moves,
/// renames, or reinterprets a point that was already published.
#[test]
fn the_published_contract_is_the_one_that_was_versioned() {
    let points = build_points(&PlantConfig::gw01());
    let digest = map_digest(&points);
    assert_eq!(
        digest, MAP_DIGEST,
        "the signal map changed: {digest:#018x}. Update MAP_DIGEST, and \
         move MAP_VERSION with it: minor for additions at unused \
         addresses, major for anything else."
    );

    // And the document that publishes the contract has to name the same
    // version this binary serves.
    let doc = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../COMPATIBILITY.md"),
    )
    .expect("COMPATIBILITY.md");
    assert!(
        doc.contains(&format!("signal-map-version: {MAP_VERSION}")),
        "COMPATIBILITY.md does not document signal map version {MAP_VERSION}"
    );
}
