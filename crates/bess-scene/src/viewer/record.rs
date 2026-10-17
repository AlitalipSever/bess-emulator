//! One tick of the plant, with its events kept in the journal.
//!
//! Both ways the viewer moves the plant, against the wall clock and in a
//! jump, step through here, so the journal cannot miss the events of one of
//! them. The numbering is the kernel's: the log's count before the tick plus
//! the event's place in it, the same `seq` the native surfaces publish, and
//! the time is that of the snapshot the tick leaves behind.

use bess_core::{Simulation, TICK_SECONDS};
use bess_models::HistoricalWeather;

use crate::alarms::Journal;

/// Step the plant once on the replayed weather and journal what it emitted.
/// Allocates nothing: the events are copied into the ring straight from the
/// kernel's own buffer.
pub fn step(sim: &mut Simulation, weather: &HistoricalWeather, journal: &mut Journal) {
    let count_before = sim.state().event_log.count;
    let ts_after = sim.unix_time_s() + TICK_SECONDS as i64;
    let inputs = weather.inputs_at(sim.unix_time_s());
    let events = sim.step(&inputs);
    if !events.is_empty() {
        journal.record(events, count_before + events.len() as u64, ts_after);
    }
}

#[cfg(test)]
mod tests {
    use super::step;
    use crate::alarms::Journal;
    use bess_core::kernel::FaultAction;
    use bess_core::{PlantConfig, Simulation};
    use bess_models::{gw01_models, gw01_weather};

    /// The journal's numbers and times are the ones the kernel and the
    /// shells use: count after the tick, simulation time after it.
    #[test]
    fn a_recorded_tick_carries_the_kernels_numbering_and_time() {
        let cfg = PlantConfig::gw01();
        let models = gw01_models(&cfg);
        let mut sim = Simulation::new(cfg, models, 7, 1_767_225_600);
        let weather = gw01_weather();
        let mut journal = Journal::new();

        sim.apply(FaultAction::PcsTrip { block: 3 })
            .expect("block 3 exists");
        step(&mut sim, &weather, &mut journal);

        let count = sim.state().event_log.count;
        assert!(count >= 2, "a trip is its mark and the bit it raises");
        assert_eq!(journal.len() as u64, count);
        let newest = journal.newest(0).expect("an entry");
        assert_eq!(newest.seq, count);
        assert_eq!(newest.unix_time_s, sim.unix_time_s());
        let oldest = journal.newest(journal.len() - 1).expect("an entry");
        assert_eq!(oldest.seq, 1);
        assert_eq!(
            oldest.event,
            bess_core::kernel::Event::FaultApplied(FaultAction::PcsTrip { block: 3 })
        );
    }
}
