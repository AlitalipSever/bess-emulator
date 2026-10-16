//! Data events: the (surface, fault) matrix of design D4, the duration
//! rule, and the parameters with their defaults. Checked for presence and
//! range only; phase 5 gives them meaning.

use bess_core::PlantConfig;

use super::{only_taken, Refusal};
use crate::error::Reason;
use crate::scenario::{DataFault, DataFaultKind};
use crate::schema::selector::Selector;
use crate::schema::{duration, EventSpec, Surface};

/// The (surface, fault) matrix of design D4, the duration rule and the
/// parameters; defaults applied.
pub(super) fn data(
    spec: &EventSpec,
    surface: Surface,
    cfg: &PlantConfig,
) -> Result<DataFault, Refusal> {
    use DataFaultKind as K;
    let kind = K::from_name(&spec.fault)
        .ok_or_else(|| ("fault", Reason::UnknownDataFault(spec.fault.clone())))?;
    let name = kind.name();
    let modbus_lacks = match kind {
        K::TimestampDrift => Some("Modbus has no wire timestamp"),
        K::NanBurst => Some("a u16 register cannot say NaN"),
        K::BackfillBurst => Some("a register bank has no replay"),
        _ => None,
    };
    if let (Surface::Modbus, Some(why)) = (surface, modbus_lacks) {
        let reason = Reason::FaultNotForSurface {
            fault: name,
            surface: surface.name(),
            why,
        };
        return Err(("fault", reason));
    }

    let duration_s = match (&spec.duration, kind) {
        (Some(_), K::BackfillBurst) => return Err(("duration", Reason::DurationForbidden(name))),
        (None, K::UnitError | K::BackfillBurst) => None,
        (None, _) => return Err(("duration", Reason::DurationRequired(name))),
        (Some(d), _) => match duration::parse(d).map_err(|e| ("duration", e.into()))? {
            // A fault that lasts no time is a row the author did not mean;
            // refusing it now keeps phase 5's tap from having to decide.
            0 => return Err(("duration", Reason::ZeroDuration(name))),
            seconds => Some(seconds),
        },
    };

    let takes: &[&str] = match kind {
        K::Dropout | K::BackfillBurst => &[],
        K::Freeze | K::NanBurst => &["points"],
        K::UnitError => &["points", "exponent"],
        K::Chatter => &["points", "rate_per_min"],
        K::TimestampDrift => &["drift_s_per_h"],
    };
    only_taken(spec, name, takes)?;
    let required = |parameter: &'static str, value: Option<f64>| {
        value.ok_or((
            parameter,
            Reason::ParameterMissing {
                fault: name,
                parameter,
            },
        ))
    };
    let out = |parameter, value, range| {
        (
            parameter,
            Reason::ParameterOutOfRange {
                parameter,
                value,
                range,
            },
        )
    };

    let points = match (&spec.points, takes.contains(&"points")) {
        (Some(text), true) => {
            Some(Selector::parse(text, cfg).map_err(|e| ("points", Reason::BadPoints(e)))?)
        }
        (None, true) => Some(Selector::All),
        (_, false) => None,
    };
    let mut fault = DataFault {
        surface,
        fault: kind,
        duration_s,
        points,
        exponent: None,
        rate_per_min: None,
        drift_s_per_h: None,
    };
    match kind {
        K::UnitError => {
            let exponent = spec.exponent.unwrap_or(3);
            if exponent == 0 {
                return Err(out("exponent", 0.0, "the nonzero integers"));
            }
            fault.exponent = Some(exponent);
        }
        K::Chatter => {
            let rate = required("rate_per_min", spec.rate_per_min)?;
            if !(0.1..=60.0).contains(&rate) {
                return Err(out("rate_per_min", rate, "0.1 to 60"));
            }
            fault.rate_per_min = Some(rate);
        }
        K::TimestampDrift => {
            let drift = required("drift_s_per_h", spec.drift_s_per_h)?;
            if drift == 0.0 || !drift.is_finite() {
                return Err(out("drift_s_per_h", drift, "the nonzero finite numbers"));
            }
            fault.drift_s_per_h = Some(drift);
        }
        _ => {}
    }
    Ok(fault)
}
