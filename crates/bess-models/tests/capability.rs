//! Capability accounting, as a property: whatever mix of isolated racks,
//! maintained blocks, faulted converters, unreleased blocks and breaker
//! position the plant is in, the availability it reports is the AC
//! capability of its dispatchable blocks, built from their in-service
//! racks, and zero while it is disconnected.
//!
//! No property-testing crate: the cases come from the kernel's own seeded
//! PRNG, so a failure names the seed that reproduces it.

use bess_core::rng::Rng;
use bess_core::state::{BlockMode, BreakerState, PcsOpState};
use bess_core::traits::PowerLimits;
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};

/// 2026-04-15 10:00 UTC: mild cells, nowhere near a derating knee.
const SPRING_MORNING_S: i64 = 1_767_225_600 + 104 * 86_400 + 10 * 3600;
const CASES: u64 = 40;

fn reduced() -> PlantConfig {
    let mut cfg = PlantConfig::gw01();
    cfg.blocks = 6;
    cfg
}

/// One random plant condition, applied to a fresh tree.
fn shake(state: &mut SiteState, rng: &mut Rng) {
    for blk in &mut state.blocks {
        if rng.next_f64() < 0.25 {
            blk.mode = BlockMode::Maintenance;
        }
        if rng.next_f64() < 0.2 {
            blk.pcs.op_state = PcsOpState::Fault;
        }
        for rack in blk.containers.iter_mut().flat_map(|c| c.racks.iter_mut()) {
            if rng.next_f64() < 0.2 {
                rack.in_service = false;
            }
        }
    }
    let blocks = state.blocks.len();
    state.ems.blocks_released = (rng.next_u64() % (blocks as u64 + 1)) as usize;
    // Hold the sequencer still, so the released count is the case's.
    state.ems.next_release_s = 1.0e9;
    if rng.next_f64() < 0.2 {
        state.substation.hv_breaker = BreakerState::Open;
    }
}

/// The architecture's predicate, computed from the outside.
fn expected(sim: &Simulation, models: &bess_core::Models) -> PowerLimits {
    let st = sim.state();
    let cfg = sim.config();
    let mut sum = PowerLimits::default();
    if st.substation.hv_breaker == BreakerState::Open {
        return sum;
    }
    for (k, blk) in st.blocks.iter().enumerate() {
        let in_set = blk.mode == BlockMode::InService
            && blk.pcs.op_state != PcsOpState::Fault
            && k < st.ems.blocks_released;
        if !in_set {
            continue;
        }
        let mut dc = PowerLimits::default();
        for rack in blk.containers.iter().flat_map(|c| c.racks.iter()) {
            let lim = models.bms.rack_limits(rack, &cfg.rack);
            dc.max_discharge_w += lim.max_discharge_w;
            dc.max_charge_w += lim.max_charge_w;
        }
        // At the DC bus voltage the kernel recorded for the block.
        let ac = models.pcs.ac_capability_w(&blk.pcs, &dc);
        sum.max_discharge_w += ac.max_discharge_w;
        sum.max_charge_w += ac.max_charge_w;
    }
    sum
}

#[test]
fn availability_is_the_dispatchable_sets_capability() {
    let weather = gw01_weather();
    let (mut dark, mut partial) = (0, 0);
    for seed in 0..CASES {
        let cfg = reduced();
        let mut rng = Rng::from_seed(seed);
        let mut state = SiteState::new(&cfg, seed, SPRING_MORNING_S);
        shake(&mut state, &mut rng);
        let reference = gw01_models(&cfg);
        let mut sim = Simulation::from_state(cfg.clone(), gw01_models(&cfg), state);
        // Idle, so the racks the predicate reads after the tick are the
        // racks the kernel read during it.
        sim.set_external_setpoint_w(Some(0.0));
        for _ in 0..2 {
            sim.step(&weather.inputs_at(sim.unix_time_s()));
        }
        let want = expected(&sim, &reference);
        let st = sim.state();
        if want.max_discharge_w == 0.0 {
            dark += 1;
        } else if want.max_discharge_w < 6.0 * cfg.pcs_rated_w - 1.0 {
            partial += 1;
        }
        for (got, want, side) in [
            (
                st.ems.available_discharge_w,
                want.max_discharge_w,
                "discharge",
            ),
            (st.ems.available_charge_w, want.max_charge_w, "charge"),
        ] {
            assert!(
                (got - want).abs() <= 1.0e-6 * want.abs().max(1.0),
                "seed {seed}: available {side} {got} W, predicate says {want} W"
            );
        }
    }
    // The cases have to exercise the predicate, not just agree with it.
    assert!(dark >= 3, "only {dark} cases with nothing available");
    assert!(partial >= 10, "only {partial} partly available cases");
}
