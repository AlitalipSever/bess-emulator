//! Every rejection of the design's test plan, each checked for its reason
//! and its position. Events start on line 4 of the head below, so a
//! second event in these files starts on line 7.

use bess_core::PlantConfig;
use bess_scenario::{load, LoadError, Position, Reason};

const HEAD: &str = "schema: 1\nname: r\nevents:\n";

fn refuse(text: &str) -> LoadError {
    load(text.as_bytes(), &PlantConfig::gw01()).expect_err("loaded a file it should refuse")
}

fn refuse_events(events: &str) -> LoadError {
    refuse(&format!("{HEAD}{events}"))
}

fn event(index: usize, field: &'static str, line: u64) -> Position {
    Position::Event { index, field, line }
}

#[test]
fn top_level_fields() {
    let e = refuse("schema: 2\nname: r\nevents: []\n");
    assert_eq!(e.reason, Reason::UnsupportedSchema(2));
    assert_eq!(e.position, Position::Line { line: 1, column: 9 });
    assert_eq!(
        e.to_string(),
        "line 1, column 9: schema 2 is not supported; this engine reads schema 1"
    );

    let e = refuse("schema: 1\nname: Hot_Week\nevents: []\n");
    assert_eq!(e.reason, Reason::BadName("Hot_Week".to_owned()));

    let e = refuse("schema: 1\nname: r\nspeed: 5000\nevents: []\n");
    assert_eq!(e.reason, Reason::SpeedOutOfRange(5000.0));
    assert_eq!(e.position, Position::Line { line: 3, column: 8 });

    // M5 phase 2: a plant older than the models' data is refused; twenty
    // years is the edge and loads.
    let e = refuse("schema: 1\nname: r\nage_years: 21\nevents: []\n");
    assert_eq!(e.reason, Reason::AgeOutOfRange(21));
    assert_eq!(
        e.position,
        Position::Line {
            line: 3,
            column: 12
        }
    );
    let aged = load(
        b"schema: 1\nname: r\nage_years: 20\nevents: []\n",
        &PlantConfig::gw01(),
    )
    .expect("twenty years loads");
    assert_eq!(aged.age_years, Some(20));

    for (start, why) in [
        (
            "2024-07-08T00:00:00+02:00",
            "UTC only: end with Z, no offset",
        ),
        ("2024-07-08T00:00:00.5Z", "whole seconds only, no fraction"),
        ("2024-07-08Z", "the form is YYYY-MM-DDThh:mm:ssZ"),
        ("2024-02-30T00:00:00Z", "not a calendar date and time"),
    ] {
        let e = refuse(&format!("schema: 1\nname: r\nstart: {start}\nevents: []\n"));
        assert_eq!(
            e.reason,
            Reason::BadStart {
                value: start.to_owned(),
                why
            },
            "{start}"
        );
    }
}

#[test]
fn the_parser_refuses_unknown_and_duplicate_fields_with_a_line() {
    let e = refuse_events("  - at: PT1H\n    targt: site\n    fault: protection_trip\n");
    assert!(matches!(&e.reason, Reason::Syntax(m) if m.contains("unknown field `targt`")));
    assert_eq!(e.position, Position::Line { line: 5, column: 5 });

    let e = refuse("schema: 1\nname: r\nname: s\nevents: []\n");
    assert!(matches!(&e.reason, Reason::Syntax(m) if m.contains("duplicate")));
    assert_eq!(e.position, Position::Line { line: 3, column: 1 });
}

#[test]
fn the_one_of_rule() {
    let e =
        refuse_events("  - at: PT1H\n    target: site\n    surface: mqtt\n    fault: dropout\n");
    assert_eq!(e.reason, Reason::BothTargetAndSurface);
    assert_eq!(e.position, event(0, "surface", 4));

    let e = refuse_events("  - at: PT1H\n    fault: dropout\n");
    assert_eq!(e.reason, Reason::NeitherTargetNorSurface);
    assert_eq!(e.position, event(0, "target", 4));
}

#[test]
fn paths_and_durations_report_their_field() {
    let e = refuse_events("  - at: PT1H\n    target: block[25].pcs\n    fault: trip\n");
    assert!(matches!(e.reason, Reason::BadTarget(_)));
    assert_eq!(e.position, event(0, "target", 4));
    assert_eq!(
        e.to_string(),
        "events[0].target (line 4): block[25]: this site has block[0] to block[19]"
    );

    let e = refuse_events("  - at: P1M\n    target: site\n    fault: protection_trip\n");
    assert!(matches!(e.reason, Reason::BadDuration(_)));
    assert_eq!(e.position, event(0, "at", 4));
}

#[test]
fn faults_must_fit_their_target_and_surface() {
    let e =
        refuse_events("  - at: PT1H\n    target: block[2].container[0].hvac\n    fault: trip\n");
    assert!(
        matches!(&e.reason, Reason::FaultNotForTarget { kind: "hvac", fault, .. } if fault == "trip")
    );
    assert_eq!(e.position, event(0, "fault", 4));

    let e = refuse_events("  - at: PT1H\n    target: block[2].pcs\n    fault: alarm_reset\n");
    assert!(matches!(
        e.reason,
        Reason::FaultNotForTarget { kind: "pcs", .. }
    ));

    let e = refuse_events(
        "  - at: PT1H\n    surface: modbus\n    fault: nan_burst\n    duration: PT1M\n",
    );
    assert_eq!(
        e.reason,
        Reason::FaultNotForSurface {
            fault: "nan_burst",
            surface: "modbus",
            why: "a u16 register cannot say NaN"
        }
    );

    let e = refuse_events("  - at: PT1H\n    surface: mqtt\n    fault: blackout\n");
    assert_eq!(e.reason, Reason::UnknownDataFault("blackout".to_owned()));
}

#[test]
fn durations_and_parameters() {
    let e = refuse_events(
        "  - at: PT1H\n    surface: mqtt\n    fault: backfill_burst\n    duration: PT1M\n",
    );
    assert_eq!(e.reason, Reason::DurationForbidden("backfill_burst"));
    assert_eq!(e.position, event(0, "duration", 4));

    let e = refuse_events("  - at: PT1H\n    surface: mqtt\n    fault: dropout\n");
    assert_eq!(e.reason, Reason::DurationRequired("dropout"));

    for fault in ["dropout", "unit_error"] {
        let e = refuse_events(&format!(
            "  - at: PT1H\n    surface: mqtt\n    fault: {fault}\n    duration: PT0S\n"
        ));
        assert_eq!(e.reason, Reason::ZeroDuration(fault));
        assert_eq!(e.position, event(0, "duration", 4));
    }

    let e = refuse_events(
        "  - at: PT1H\n    target: site\n    fault: protection_trip\n    duration: PT1M\n",
    );
    assert_eq!(e.reason, Reason::DurationForbidden("protection_trip"));

    let leak =
        "  - at: PT1H\n    target: block[0].container[0].rack[0]\n    fault: self_discharge\n";
    let out_of_range = |e: LoadError| match e.reason {
        Reason::ParameterOutOfRange {
            parameter, value, ..
        } => (parameter, value),
        other => panic!("expected a range refusal, got {other:?}"),
    };
    let e = refuse_events(&format!("{leak}    rate_pct_per_month: 150\n"));
    assert_eq!(e.position, event(0, "rate_pct_per_month", 4));
    assert_eq!(out_of_range(e), ("rate_pct_per_month", 150.0));
    let e = refuse_events(&format!("{leak}    rate_pct_per_month: [30, 10]\n"));
    assert_eq!(out_of_range(e), ("rate_pct_per_month", 10.0));

    let e = refuse_events(leak);
    assert_eq!(
        e.reason,
        Reason::ParameterMissing {
            fault: "self_discharge",
            parameter: "rate_pct_per_month"
        }
    );

    let e = refuse_events("  - at: PT1H\n    surface: mqtt\n    fault: dropout\n    duration: PT1M\n    exponent: 3\n");
    assert_eq!(
        e.reason,
        Reason::ParameterNotTaken {
            fault: "dropout",
            parameter: "exponent"
        }
    );

    let e = refuse_events("  - at: PT1H\n    surface: mqtt\n    fault: chatter\n    duration: PT1M\n    rate_per_min: 120\n");
    assert!(matches!(
        e.reason,
        Reason::ParameterOutOfRange {
            parameter: "rate_per_min",
            ..
        }
    ));

    let e =
        refuse_events("  - at: PT1H\n    surface: mqtt\n    fault: unit_error\n    exponent: 0\n");
    assert!(matches!(
        e.reason,
        Reason::ParameterOutOfRange {
            parameter: "exponent",
            ..
        }
    ));
}

/// A file that states how long it is cannot be shorter than its events,
/// and cannot be nothing long.
#[test]
fn the_length_bounds_the_events() {
    let e = refuse("schema: 1\nname: r\nlength: PT0S\nevents: []\n");
    assert_eq!(e.reason, Reason::ZeroLength);
    assert_eq!(e.position, Position::Line { line: 3, column: 9 });
    let e = refuse("schema: 1\nname: r\nlength: P1M\nevents: []\n");
    assert!(matches!(e.reason, Reason::BadDuration(_)));

    let text = "schema: 1\nname: r\nlength: PT1H\nevents:\n  - at: PT1H\n    target: site\n    fault: protection_trip\n";
    let e = refuse(text);
    assert_eq!(e.reason, Reason::EventAfterLength { length_s: 3600 });
    assert_eq!(e.position, event(0, "at", 5));
    let s = load(
        text.replace("at: PT1H", "at: PT59M59S").as_bytes(),
        &PlantConfig::gw01(),
    )
    .unwrap();
    assert_eq!(s.length_s, Some(3600));
}

/// A selector is a tree path the surfaces publish points under, or `*`;
/// a backfill burst needs a dropout on its surface that has ended by then,
/// wherever in the file the dropout is listed.
#[test]
fn points_and_backfill_follow_the_phase_5_rules() {
    let e = refuse_events(
        "  - at: PT1H\n    surface: mqtt\n    fault: freeze\n    duration: PT1M\n    points: block[2].container[0].hvac\n",
    );
    assert!(matches!(e.reason, Reason::BadPoints(_)), "{e}");
    assert_eq!(e.position, event(0, "points", 4));
    assert!(e
        .to_string()
        .contains("points block[2].container[0].hvac: points are selected with"));
    let e = refuse_events(
        "  - at: PT1H\n    surface: modbus\n    fault: unit_error\n    points: block[25]\n",
    );
    assert!(matches!(e.reason, Reason::BadPoints(_)), "{e}");

    let dropout = "  - at: PT1H\n    surface: mqtt\n    fault: dropout\n    duration: PT20M\n";
    let burst = |at: &str, surface: &str| {
        format!("  - at: {at}\n    surface: {surface}\n    fault: backfill_burst\n")
    };
    let e = refuse_events(&burst("PT2H", "mqtt"));
    assert_eq!(e.reason, Reason::BackfillWithoutDropout);
    assert_eq!(e.position, event(0, "at", 4));
    // During the dropout, and on the other surface, it has nothing either.
    let e = refuse_events(&format!("{dropout}{}", burst("PT1H10M", "mqtt")));
    assert_eq!(e.reason, Reason::BackfillWithoutDropout);
    assert_eq!(e.position, event(1, "at", 8));
    // At the dropout's end it replays, and the dropout may be listed after.
    for text in [
        format!("{dropout}{}", burst("PT1H20M", "mqtt")),
        format!("{}{dropout}", burst("PT2H", "mqtt")),
    ] {
        assert!(load(format!("{HEAD}{text}").as_bytes(), &PlantConfig::gw01()).is_ok());
    }
}

#[test]
fn one_target_one_action_per_tick_in_time_order() {
    let hvac = "    target: block[2].container[0].hvac\n";
    let e = refuse_events(&format!(
        "  - at: PT1H\n{hvac}    fault: failure\n  - at: PT60M\n{hvac}    fault: repair\n"
    ));
    assert_eq!(e.reason, Reason::SharedTick { other: 0 });
    assert_eq!(e.position, event(1, "at", 7));

    let e = refuse_events(&format!(
        "  - at: PT2H\n{hvac}    fault: repair\n  - at: PT1H\n{hvac}    fault: failure\n"
    ));
    assert_eq!(e.reason, Reason::OutOfOrder { other: 0 });
    assert_eq!(
        e.to_string(),
        "events[1].at (line 7): events[0] acts on the same target later; \
         list each target's events in time order"
    );

    // Different targets may share a tick and come in any order.
    let ok = format!(
        "{HEAD}  - at: PT2H\n{hvac}    fault: failure\n  - at: PT1H\n    target: block[2].pcs\n    fault: trip\n  - at: PT1H\n    target: site\n    fault: protection_trip\n"
    );
    assert!(load(ok.as_bytes(), &PlantConfig::gw01()).is_ok());
}
