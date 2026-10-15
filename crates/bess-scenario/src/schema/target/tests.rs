//! Every row of the table resolves and spells itself back; every
//! refusal names what the site has.

use bess_core::alarms::ResetScope;
use bess_core::kernel::FaultAction;
use bess_core::PlantConfig;

use super::Target;

fn parse(path: &str) -> Result<Target, String> {
    Target::parse(path, &PlantConfig::gw01()).map_err(|e| e.to_string())
}

/// A parsed path spells itself back exactly, and every action names the
/// target its file row had.
#[test]
fn every_target_spells_itself_back() {
    for path in [
        "site",
        "block[19]",
        "block[0].pcs",
        "block[2].container[1].hvac",
        "block[2].container[1].rack[11]",
    ] {
        assert_eq!(parse(path).unwrap().to_string(), path);
    }
    let rack = Target::Rack {
        block: 2,
        container: 1,
        rack: 5,
    };
    let block = Target::Block { block: 2 };
    let hvac = Target::Hvac {
        block: 2,
        container: 1,
    };
    let scope = ResetScope::Rack {
        block: 2,
        container: 1,
        rack: 5,
    };
    let cases = [
        (
            FaultAction::HvacFailure {
                block: 2,
                container: 1,
            },
            hvac,
        ),
        (
            FaultAction::HvacRepair {
                block: 2,
                container: 1,
            },
            hvac,
        ),
        (FaultAction::PcsTrip { block: 2 }, Target::Pcs { block: 2 }),
        (FaultAction::ProtectionTrip, Target::Site),
        (
            FaultAction::RackSelfDischarge {
                block: 2,
                container: 1,
                rack: 5,
                excess_ppm_per_day: 10,
            },
            rack,
        ),
        (
            FaultAction::RackIsolate {
                block: 2,
                container: 1,
                rack: 5,
            },
            rack,
        ),
        (
            FaultAction::RackRestore {
                block: 2,
                container: 1,
                rack: 5,
            },
            rack,
        ),
        (FaultAction::BlockMaintenanceEnter { block: 2 }, block),
        (FaultAction::BlockMaintenanceExit { block: 2 }, block),
        (FaultAction::AlarmReset(ResetScope::Site), Target::Site),
        (FaultAction::AlarmReset(ResetScope::Block(2)), block),
        (FaultAction::AlarmReset(scope), rack),
    ];
    for (action, target) in cases {
        assert_eq!(Target::of(&action), target, "{action:?}");
    }
}

#[test]
fn every_row_of_the_table_resolves() {
    assert_eq!(parse("site"), Ok(Target::Site));
    assert_eq!(parse("block[19]"), Ok(Target::Block { block: 19 }));
    assert_eq!(parse("block[0].pcs"), Ok(Target::Pcs { block: 0 }));
    assert_eq!(
        parse("block[2].container[1].hvac"),
        Ok(Target::Hvac {
            block: 2,
            container: 1
        })
    );
    assert_eq!(
        parse("block[2].container[1].rack[11]"),
        Ok(Target::Rack {
            block: 2,
            container: 1,
            rack: 11
        })
    );
}

#[test]
fn refusals_say_what_the_site_has() {
    let cases = [
        (
            "block[25]",
            "block[25]: this site has block[0] to block[19]",
        ),
        (
            "block[2].container[0].fan",
            "block[2].container[0].fan: a container has `hvac` and `rack[0]` to `rack[11]`",
        ),
        (
            "block[2].container[0].rack[12]",
            "block[2].container[0].rack[12]: a container has `hvac` and `rack[0]` to `rack[11]`",
        ),
        (
            "block[2].container[2].hvac",
            "block[2].container[2]: a block has `pcs` and `container[0]` to `container[1]`",
        ),
        (
            "block[2].container[0]",
            "block[2].container[0]: a container is not a target; name its `hvac` or a `rack[R]`",
        ),
        (
            "block02.pcs",
            "block02: a path starts at `site` or `block[N]`, N from 0 to 19",
        ),
        (
            "block[02]",
            "block[02]: indices are plain zero-based numbers, as in block[2]",
        ),
        (
            "block[2].pcs.fan",
            "block[2].pcs.fan: a PCS has no parts a scenario addresses",
        ),
        (
            "block[2].container[0].hvac.fan",
            "block[2].container[0].hvac.fan: hvac has no parts a scenario addresses",
        ),
        (
            "",
            ": a path starts at `site` or `block[N]`, N from 0 to 19",
        ),
    ];
    for (path, message) in cases {
        assert_eq!(parse(path), Err(message.to_owned()), "{path}");
    }
}
