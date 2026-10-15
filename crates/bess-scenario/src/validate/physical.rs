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
    let takes: &[&str] = if fault == "self_discharge" {
        &["rate_pct_per_month"]
    } else {
        &[]
    };
    only_taken(spec, fault, takes)?;
    Ok(Fault::Physical { target, action })
}

fn not_for(spec: &EventSpec, target: Target) -> Refusal {
    let allowed = match target {
        Target::Site => "protection_trip or alarm_reset",
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
