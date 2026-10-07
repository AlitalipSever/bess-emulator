//! Prepared plants for the surface tests, each started in a state that makes
//! one thing happen soon, so a test over real sockets waits seconds rather
//! than a simulated day.

use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::gw01_models;

/// 2026-07-14 14:00 UTC, the warmest afternoon of the replayed year.
const JULY_AFTERNOON_S: i64 = 1_767_225_600 + 194 * 86_400 + 14 * 3600;
/// 2026-01-01 00:00 UTC, the CLI's default start.
const NEW_YEAR_S: i64 = 1_767_225_600;

/// The block the hot plant fails.
pub const HOT_BLOCK: usize = 2;

fn plant(start_unix_s: i64, prepare: impl FnOnce(&mut SiteState)) -> Simulation {
    let cfg = PlantConfig::gw01();
    let mut state = SiteState::new(&cfg, 7, start_unix_s);
    prepare(&mut state);
    let models = gw01_models(&cfg);
    Simulation::from_state(cfg, models, state)
}

/// The kernel's causal chain plant (`bess-models/tests/alarm_chain.rs`): a
/// July afternoon, both HVAC units of one block down for hours, its racks at
/// 54 C, the site asked for full discharge. Derate, setpoint miss and site
/// limit follow at roughly 8, 21 and 31 simulated minutes.
pub fn hot_plant() -> Simulation {
    let mut sim = plant(JULY_AFTERNOON_S, |state| {
        for rack in state
            .blocks
            .iter_mut()
            .flat_map(|blk| blk.containers.iter_mut().flat_map(|c| c.racks.iter_mut()))
        {
            rack.soc = 0.9;
            rack.cell_temp_c = 30.0;
        }
        for hot in &mut state.blocks[HOT_BLOCK].containers {
            hot.air_temp_c = 45.0;
            for rack in &mut hot.racks {
                rack.cell_temp_c = 54.0;
            }
        }
    });
    sim.set_hvac_failed(HOT_BLOCK, 0, true);
    sim.set_hvac_failed(HOT_BLOCK, 1, true);
    sim.set_external_setpoint_w(Some(100.0e6));
    sim
}

/// One rack of block 1 at 65 C on a winter night: its over-temperature trip
/// latches on the first tick, and the rack needs simulated minutes to cool
/// back under 60 C, so a reset at real-time speed finds the cause present.
pub fn tripped_plant() -> Simulation {
    plant(NEW_YEAR_S, |state| {
        state.blocks[1].containers[0].racks[4].cell_temp_c = 65.0;
    })
}
