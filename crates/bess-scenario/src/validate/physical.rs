//! Physical events: the (kind, fault) table of design D3, and the one
//! parameter a physical fault takes, a leak's rate.

use bess_core::kernel::FaultAction;
use bess_core::PlantConfig;

use super::{only_taken, Refusal};
use crate::error::Reason;
use crate::scenario::{Fault, PendingAction};
use crate::schema::target::Target;
use crate::schema::{EventSpec, Rate};

/// The (kind, fault) table of design D3, then the parameters.
pub(super) fn physical(spec: &EventSpec, path: &str, cfg: &PlantConfig) -> Result<Fault, Refusal> {
    use FaultAction as A;
    use PendingAction::Ready;
    let target = Target::parse(path, cfg).map_err(|e| ("target", e.into()))?;
    let action = match (target, spec.fault.as_str()) {
        (Target::Hvac { block, container }, "failure") => {
            Ready(A::HvacFailure { block, container })
        }
        (Target::Hvac { block, container }, "repair") => Ready(A::HvacRepair { block, container }),
        (Target::Pcs { block }, "trip") => Ready(A::PcsTrip { block }),
        (Target::Site, "protection_trip") => Ready(A::ProtectionTrip),
        (Target::Substation, "breaker_open") => Ready(A::HvBreakerOpen),
        (Target::Substation, "breaker_close") => Ready(A::HvBreakerClose),
        (Target::Substation, "disconnectors_close") => Ready(A::Disconnectors { closed: true }),
        (Target::Substation, "disconnectors_open") => Ready(A::Disconnectors { closed: false }),
        (Target::Substation, "earthing_close") => Ready(A::EarthingSwitch { closed: true }),
        (Target::Substation, "earthing_open") => Ready(A::EarthingSwitch { closed: false }),
        (Target::Grid, "frequency_offset") => {
            let mhz = spec.mhz.ok_or((
                "mhz",
                Reason::ParameterMissing {
                    fault: "frequency_offset",
                    parameter: "mhz",
                },
            ))?;
            if !(-3000..=3000).contains(&mhz) {
                return Err((
                    "mhz",
                    Reason::ParameterOutOfRange {
                        parameter: "mhz",
                        value: f64::from(mhz),
                        range: "-3000 to 3000 (millihertz; 0 ends the event)",
                    },
                ));
            }
            Ready(A::GridFrequencyOffset { mhz })
        }
        (Target::Block { block }, "maintenance_enter") => Ready(A::BlockMaintenanceEnter { block }),
        (Target::Block { block }, "maintenance_exit") => Ready(A::BlockMaintenanceExit { block }),
        (
            Target::Rack {
                block,
                container,
                rack,
            },
            fault,
        ) if fault != "alarm_reset" => match fault {
            "isolate" => Ready(A::RackIsolate {
                block,
                container,
                rack,
            }),
            "restore" => Ready(A::RackRestore {
                block,
                container,
                rack,
            }),
            "self_discharge" => {
                let rate = spec.rate_pct_per_month.ok_or((
                    "rate_pct_per_month",
                    Reason::ParameterMissing {
                        fault: "self_discharge",
                        parameter: "rate_pct_per_month",
                    },
                ))?;
                check_rate(rate)?;
                PendingAction::SelfDischarge {
                    block,
                    container,
                    rack,
                    rate_pct_per_month: rate,
                }
            }
            _ => return Err(not_for(spec, target)),
        },
        (_, "alarm_reset") => match target.reset_scope() {
            Some(scope) => Ready(A::AlarmReset(scope)),
            None => return Err(not_for(spec, target)),
        },
        _ => return Err(not_for(spec, target)),
    };
    let fault = match action {
        Ready(a) => a.name(),
        PendingAction::SelfDischarge { .. } => "self_discharge",
    };
    if spec.duration.is_some() {
        return Err(("duration", Reason::DurationForbidden(fault)));
    }
    let takes: &[&str] = match fault {
        "self_discharge" => &["rate_pct_per_month"],
        "frequency_offset" => &["mhz"],
        _ => &[],
    };
    only_taken(spec, fault, takes)?;
    Ok(Fault::Physical { target, action })
}

fn not_for(spec: &EventSpec, target: Target) -> Refusal {
    let allowed = match target {
        Target::Site => "protection_trip or alarm_reset",
        Target::Substation => {
            "breaker_open, breaker_close, disconnectors_open, disconnectors_close, \
             earthing_open or earthing_close"
        }
        Target::Grid => "frequency_offset",
        Target::Block { .. } => "maintenance_enter, maintenance_exit or alarm_reset",
        Target::Pcs { .. } => "trip",
        Target::Hvac { .. } => "failure or repair",
        Target::Rack { .. } => "self_discharge, isolate, restore or alarm_reset",
    };
    let reason = Reason::FaultNotForTarget {
        fault: spec.fault.clone(),
        kind: target.kind(),
        allowed,
    };
    ("fault", reason)
}

fn check_rate(rate: Rate) -> Result<(), Refusal> {
    let out = |value| {
        (
            "rate_pct_per_month",
            Reason::ParameterOutOfRange {
                parameter: "rate_pct_per_month",
                value,
                range: "0.1 to 100 (a range as [lo, hi] with lo below hi)",
            },
        )
    };
    let ok = |v: f64| (0.1..=100.0).contains(&v);
    match rate {
        Rate::Fixed(v) if !ok(v) => Err(out(v)),
        Rate::Range(lo, _) if !ok(lo) => Err(out(lo)),
        Rate::Range(lo, hi) if !ok(hi) || hi <= lo => Err(out(hi)),
        _ => Ok(()),
    }
}
