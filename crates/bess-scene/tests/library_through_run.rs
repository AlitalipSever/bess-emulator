//! The iteration's gate: a library case played through the viewer's own
//! loop lands on the digest the headless run committed in `scenarios/`.
//! The browser and CI play one story, byte for byte.
//!
//! Every case up to two days long, through jumps with the awkward budget
//! `jump.rs` uses; three short ones again with live ticks and jumps
//! alternating across their event ticks. The ten-day and six-day cases run
//! in CI's `assert-library.sh` already and would cost minutes here.

use std::path::PathBuf;

use bess_core::checkpoint::state_digest;
use bess_core::PlantConfig;
use bess_models::{gw01_models, gw01_weather, HistoricalWeather};
use bess_scenario::library::CASES;
use bess_scenario::RunParams;
use bess_scene::alarms::Journal;
use bess_scene::viewer::jump::FastForward;
use bess_scene::viewer::run::Run;
use bess_scene::viewer::Session;
use bess_scene::ViewerCommand;
use serde_json::Value;

const TWO_DAYS_S: u64 = 2 * 86_400;

fn record(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(format!("{name}.digest.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("a record per case"))
        .expect("a record is JSON")
}

fn hex(v: u64) -> String {
    format!("{v:#018x}")
}

/// The case at its record's seed and start, as the headless run played it.
fn run_of(name: &str) -> (Run, u64) {
    let rec = record(name);
    let cfg = PlantConfig::gw01();
    let case = bess_scenario::library::find(name).expect("embedded");
    let resolved = case.load(&cfg).expect("loads").resolve(RunParams {
        seed: rec["seed"].as_u64().expect("seed"),
        start_unix_s: rec["start_unix_s"].as_i64().expect("start"),
        speed: 1.0,
    });
    let ticks = resolved.ticks.expect("a library case states its length");
    let models = gw01_models(&cfg);
    (Run::playing(resolved, cfg, models), ticks)
}

fn assert_on_record(name: &str, run: &Run, journal: &Journal) {
    let rec = record(name);
    let state = run.sim.state();
    assert_eq!(
        hex(state_digest(state)),
        rec["state_digest"],
        "{name}: state"
    );
    assert_eq!(
        state.event_log.count,
        rec["event_log"]["count"].as_u64().unwrap(),
        "{name}: event count"
    );
    assert_eq!(
        hex(state.event_log.digest),
        rec["event_log"]["digest"],
        "{name}: event digest"
    );
    // Every event the kernel logged reached the journal, once.
    let held = journal.len() as u64 + journal.dropped();
    assert_eq!(held, state.event_log.count, "{name}: journal");
}

fn jump(run: &mut Run, weather: HistoricalWeather, journal: &mut Journal, to: i64) {
    let Some(jump) = FastForward::toward(&run.sim, to) else {
        return;
    };
    let mut frames = 0;
    while !jump.advance(run, &weather, journal, 997) {
        frames += 1;
        assert!(frames < 1_000_000, "the jump never arrived");
    }
}

#[test]
fn every_short_case_jumped_lands_on_its_record() {
    let weather = gw01_weather();
    let mut played = 0;
    for case in CASES {
        let (mut run, ticks) = run_of(case.name);
        if ticks > TWO_DAYS_S {
            continue;
        }
        let mut journal = Journal::new();
        let end = run.end_unix_s().expect("a length");
        jump(&mut run, weather, &mut journal, end);
        assert_eq!(run.sim.state().tick, ticks);
        assert_on_record(case.name, &run, &journal);
        played += 1;
    }
    assert_eq!(played, 18, "eighteen cases are two days or shorter");
}

/// Live ticks and jumps alternate, and the jumps target the event ticks
/// themselves: an event on a jumped-over or landed-on tick fires once, on
/// its tick.
#[test]
fn live_ticks_and_jumps_mixed_still_land_on_the_record() {
    let weather = gw01_weather();
    for name in [
        "pcs-trip-and-reset",
        "protection-trip-staggered-return",
        "hvac-failure-both-units",
    ] {
        let (mut run, ticks) = run_of(name);
        let mut journal = Journal::new();
        let end = run.end_unix_s().expect("a length");
        while run.sim.state().tick < ticks {
            for _ in 0..13 {
                if run.sim.state().tick < ticks {
                    run.tick(&weather, &mut journal);
                }
            }
            let to = run.next_event_unix_s().unwrap_or(end).min(end);
            jump(&mut run, weather, &mut journal, to);
        }
        assert_on_record(name, &run, &journal);
    }
}

/// The panel's path end to end: start from the library, run to the end,
/// and the session's plant is the record's.
#[test]
fn a_case_started_and_run_to_the_end_through_the_session_is_the_record() {
    let mut session = Session::new();
    session.apply(ViewerCommand::StartScenario("pcs-trip-and-reset"));
    let (view, now) = session.story().expect("a story plays");
    assert_eq!((view.name.as_str(), now.fired), ("pcs-trip-and-reset", 0));
    session.apply(ViewerCommand::RunToNextEvent);
    while session.jumping() {
        session.advance(0.016);
    }
    // Landed on the first event's tick, which has not fired yet.
    let first = view_first_at(&session);
    assert_eq!(session.run.elapsed_s(), Some(first));
    assert_eq!(session.story().map(|(_, n)| n.fired), Some(0));
    session.apply(ViewerCommand::RunToEnd);
    while session.jumping() {
        session.advance(0.016);
    }
    assert_on_record("pcs-trip-and-reset", &session.run, &session.journal);
}

fn view_first_at(session: &Session) -> u64 {
    session.story().expect("a story").0.rows[0].at_s
}

#[test]
fn leaving_a_story_leaves_no_trace_of_it() {
    let mut session = Session::new();
    session
        .start_scenario("pcs-trip-and-reset")
        .expect("in the library");
    assert!(session.sim().state().meta.scenario.is_some());
    session.apply(ViewerCommand::RunToNextEvent);
    while session.jumping() {
        session.advance(0.016);
    }
    session.advance(0.25);
    assert!(!session.journal.is_empty());

    session.apply(ViewerCommand::RestartAt(1_767_225_600));
    assert_eq!(session.sim().state().meta.scenario, None);
    assert!(session.journal.is_empty());
    assert!(session.story().is_none());
}

#[test]
fn the_door_refuses_what_it_cannot_play_and_says_why() {
    let mut session = Session::new();
    let err = session.start_scenario("no-such-case").unwrap_err();
    assert!(
        err.contains("no library case named \"no-such-case\""),
        "{err}"
    );
    assert!(
        err.contains("pcs-trip-and-reset"),
        "the message lists the library"
    );

    let file = "schema: 1\nname: off-site\nevents:\n  - at: PT1M\n    target: block[25].pcs\n    fault: trip\n";
    let err = session.load_scenario_yaml(file).unwrap_err();
    assert_eq!(
        err,
        "events[0].target (line 4): block[25]: this site has block[0] to block[19]"
    );
    assert!(session.story().is_none(), "a refused file changes nothing");

    let ok = "schema: 1\nname: custom\nevents:\n  - at: PT1M\n    target: block[3].pcs\n    fault: trip\n";
    session.load_scenario_yaml(ok).expect("a valid file plays");
    let (view, _) = session.story().expect("a story");
    assert_eq!(view.name, "custom");
    assert_eq!(view.headline, "");
}

/// The door's two JSON answers: the catalogue, and the status in the REST
/// shape, counting what fired.
#[test]
fn the_door_answers_in_the_rest_shape() {
    let mut session = Session::new();
    let catalogue: Value = serde_json::from_str(&session.scenarios_json()).expect("JSON");
    assert_eq!(catalogue.as_array().map(Vec::len), Some(CASES.len()));
    assert_eq!(
        serde_json::from_str::<Value>(&session.scenario_status_json()).expect("JSON"),
        serde_json::json!({"loaded": false})
    );

    session
        .start_scenario("pcs-trip-and-reset")
        .expect("in the library");
    let status =
        |s: &Session| -> Value { serde_json::from_str(&s.scenario_status_json()).expect("JSON") };
    let before = status(&session);
    assert_eq!(before["loaded"], true);
    assert_eq!(before["name"], "pcs-trip-and-reset");
    assert_eq!(
        (&before["fired"], &before["tick"]),
        (&Value::from(0), &Value::from(0))
    );
    assert_eq!(before["next"]["at_s"], 4500);

    session.apply(ViewerCommand::RunToNextEvent);
    while session.jumping() {
        session.advance(0.016);
    }
    session.advance(0.25);
    let after = status(&session);
    assert_eq!(after["fired"], 1);
    assert_eq!(after["events"][0]["fired"], true);
    assert_eq!(after["next"]["fault"], "alarm_reset");
}
