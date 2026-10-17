//! The scenario engine's types, in the panel's terms: the library as the
//! picker lists it, and a resolved case as the strip shows it.

use bess_core::kernel::FaultAction;
use bess_core::PlantConfig;
use bess_scenario::library::CASES;
use bess_scenario::{Due, Fault, Resolved, Target};

use crate::panels::scenario::{CaseListing, StoryRow, StoryView};

/// Every embedded case that loads against `cfg`, in the library's order.
/// A case that does not load is left out and logged: the library's own test
/// loads every case against GW-01, so this only happens on another site.
pub fn listing(cfg: &PlantConfig) -> Vec<CaseListing> {
    CASES
        .iter()
        .filter_map(|case| match case.load(cfg) {
            Ok(scenario) => Some(CaseListing {
                name: case.name,
                taxonomy: scenario.taxonomy.map_or("other", |t| t.name()),
                length_s: scenario.length_s,
                headline: case.headline(),
                events: scenario.events.len(),
                wire_events: scenario
                    .events
                    .iter()
                    .filter(|e| matches!(e.fault, Fault::Data(_)))
                    .count(),
            }),
            Err(err) => {
                log::warn!("library case {} does not load here: {err}", case.name);
                None
            }
        })
        .collect()
}

/// A resolved case as the strip shows it.
pub fn story_view(
    resolved: &Resolved,
    headline: String,
    taxonomy: Option<&'static str>,
) -> StoryView {
    StoryView {
        name: resolved.id.name.clone(),
        headline,
        taxonomy,
        length_s: resolved.ticks.map(|t| t * bess_core::TICK_SECONDS),
        rows: resolved
            .schedule
            .iter()
            .map(|s| StoryRow {
                at_s: s.at_s,
                what: what(&s.due),
                wire_only: matches!(s.due, Due::Data(_)),
            })
            .collect(),
    }
}

/// Target and fault as the file spells them; a drawn rate where there is
/// one, a surface where the fault acts on one.
fn what(due: &Due) -> String {
    match due {
        Due::Physical(action) => {
            let what = format!("{} {}", Target::of(action), action.name());
            match action {
                FaultAction::RackSelfDischarge {
                    excess_ppm_per_day, ..
                } => format!("{what} ({excess_ppm_per_day} ppm/day)"),
                _ => what,
            }
        }
        Due::Data(fault) => format!("{} {}", fault.surface.name(), fault.fault.name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_scenario::RunParams;

    #[test]
    fn the_listing_carries_every_case_and_marks_the_wire_ones() {
        let cfg = PlantConfig::gw01();
        let cases = listing(&cfg);
        assert_eq!(cases.len(), CASES.len());
        let chatter = cases
            .iter()
            .find(|c| c.name == "alarm-chatter-storm")
            .expect("listed");
        assert_eq!(chatter.taxonomy, "controls");
        assert_eq!(chatter.wire_events, chatter.events);
        let both = cases
            .iter()
            .find(|c| c.name == "hvac-failure-both-units")
            .expect("listed");
        assert_eq!((both.wire_events, both.length_s), (0, Some(6 * 3600)));
    }

    #[test]
    fn a_story_row_reads_as_the_file_does() {
        let cfg = PlantConfig::gw01();
        let case = bess_scenario::library::find("pcs-trip-and-reset").expect("in the library");
        let scenario = case.load(&cfg).expect("loads");
        let resolved = scenario.resolve(RunParams {
            seed: 42,
            start_unix_s: 0,
            speed: 60.0,
        });
        let view = story_view(&resolved, case.headline(), Some("bos"));
        assert_eq!(view.name, "pcs-trip-and-reset");
        assert_eq!(view.length_s, Some(3 * 3600));
        assert_ne!(view.rows, []);
        assert!(view.rows.iter().all(|r| !r.wire_only));
        assert!(
            view.rows[0].what.ends_with(" trip"),
            "{}",
            view.rows[0].what
        );
    }
}
