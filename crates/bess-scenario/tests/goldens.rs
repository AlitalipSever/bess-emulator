//! The design's example, parsed field for field; the two other fixtures
//! load.

use bess_core::alarms::ResetScope;
use bess_core::digest::fnv1a_64;
use bess_core::kernel::FaultAction;
use bess_core::PlantConfig;
use bess_scenario::{
    load, DataFault, DataFaultKind, Fault, PendingAction, Rate, Surface, Target, Taxonomy,
};

const EXAMPLE: &[u8] = include_bytes!("fixtures/hvac-failure-hot-week.yaml");

#[test]
fn the_example_parses_to_what_the_design_says() {
    let s = load(EXAMPLE, &PlantConfig::gw01()).unwrap();
    assert_eq!(s.id.name, "hvac-failure-hot-week");
    assert_eq!(s.id.content_hash, fnv1a_64(EXAMPLE));
    assert_eq!(s.taxonomy, Some(Taxonomy::Bos));
    assert_eq!(s.seed, Some(42));
    // 2024-07-08 00:00:00 UTC.
    assert_eq!(s.start_unix_s, Some(1_720_396_800));
    assert_eq!(s.speed, Some(60.0));

    let at: Vec<u64> = s.events.iter().map(|e| e.at_s).collect();
    assert_eq!(
        at,
        [50_400, 136_800, 172_800, 799_200, 51_600, 52_800],
        "PT14H, PT38H, P2D, P9DT6H, PT14H20M, PT14H40M"
    );
    let hvac = Target::Hvac {
        block: 2,
        container: 0,
    };
    let rack = Target::Rack {
        block: 2,
        container: 1,
        rack: 5,
    };
    let faults: Vec<&Fault> = s.events.iter().map(|e| &e.fault).collect();
    assert_eq!(
        faults,
        [
            &Fault::Physical {
                target: hvac,
                action: PendingAction::Ready(FaultAction::HvacFailure {
                    block: 2,
                    container: 0
                }),
            },
            &Fault::Physical {
                target: hvac,
                action: PendingAction::Ready(FaultAction::HvacRepair {
                    block: 2,
                    container: 0
                }),
            },
            &Fault::Physical {
                target: rack,
                action: PendingAction::SelfDischarge {
                    block: 2,
                    container: 1,
                    rack: 5,
                    rate_pct_per_month: Rate::Range(10.0, 30.0),
                },
            },
            &Fault::Physical {
                target: rack,
                action: PendingAction::Ready(FaultAction::RackIsolate {
                    block: 2,
                    container: 1,
                    rack: 5
                }),
            },
            &Fault::Data(DataFault {
                surface: Surface::Mqtt,
                fault: DataFaultKind::Dropout,
                duration_s: Some(1200),
                points: None,
                exponent: None,
                rate_per_min: None,
                drift_s_per_h: None,
            }),
            &Fault::Data(DataFault {
                surface: Surface::Mqtt,
                fault: DataFaultKind::BackfillBurst,
                duration_s: None,
                points: None,
                exponent: None,
                rate_per_min: None,
                drift_s_per_h: None,
            }),
        ]
    );
}

#[test]
fn an_empty_scenario_and_the_twin_load() {
    let cfg = PlantConfig::gw01();
    let empty = load(include_bytes!("fixtures/empty.yaml"), &cfg).unwrap();
    assert_eq!(empty.events, []);
    assert_eq!(
        (empty.seed, empty.start_unix_s, empty.speed),
        (None, None, None)
    );

    let twin = load(include_bytes!("fixtures/hot-afternoon.yaml"), &cfg).unwrap();
    assert_eq!(twin.seed, Some(7));
    assert_eq!(
        twin.start_unix_s,
        Some(1_767_225_600 + 194 * 86_400 + 14 * 3600)
    );
    assert_eq!(twin.events.len(), 2);
}

/// Parameter defaults land where the design puts them, and a reset reaches
/// the scope its target names.
#[test]
fn defaults_and_scopes() {
    let yaml = br"
schema: 1
name: defaults
events:
  - at: PT1M
    surface: modbus
    fault: unit_error
  - at: PT2M
    surface: mqtt
    fault: freeze
    duration: PT5M
    points: block02.pcs
  - at: PT3M
    target: block[4]
    fault: alarm_reset
";
    let s = load(yaml, &PlantConfig::gw01()).unwrap();
    let Fault::Data(unit) = &s.events[0].fault else {
        panic!("not a data fault")
    };
    assert_eq!(
        (unit.points.as_deref(), unit.exponent, unit.duration_s),
        (Some("*"), Some(3), None)
    );
    let Fault::Data(freeze) = &s.events[1].fault else {
        panic!("not a data fault")
    };
    assert_eq!(freeze.points.as_deref(), Some("block02.pcs"));
    assert_eq!(
        s.events[2].fault,
        Fault::Physical {
            target: Target::Block { block: 4 },
            action: PendingAction::Ready(FaultAction::AlarmReset(ResetScope::Block(4))),
        }
    );
}
