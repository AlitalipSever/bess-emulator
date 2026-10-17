//! The loop the shell runs, for the integration tests: before each tick,
//! hand what is due to the kernel, then step. Included by path from the
//! tests that drive a plant.

use bess_core::kernel::Event;
use bess_core::Simulation;
use bess_models::gw01_weather;
use bess_scenario::Player;

/// Play `ticks` ticks; the events each tick emitted, with the tick.
pub fn play(sim: &mut Simulation, player: &mut Player, ticks: u64) -> Vec<(u64, Event)> {
    let weather = gw01_weather();
    let mut log = Vec::new();
    for _ in 0..ticks {
        let tick = sim.state().tick;
        let fired = player.fire_due(sim);
        assert!(fired.refused.is_none(), "validated against this site");
        let events = sim.step(&weather.inputs_at(sim.unix_time_s()));
        log.extend(events.iter().map(|e| (tick, *e)));
    }
    log
}
