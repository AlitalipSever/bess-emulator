//! Every rule a parsed file must pass before it is a scenario. The
//! file-level rules live here: the schema version, the top-level fields,
//! the one-of rule, and the per-target order of design D12. A physical
//! event's (kind, fault) table (D3) is `validate/physical.rs`, a data
//! event's (surface, fault) matrix (D4) `validate/data.rs`; the parameter
//! check both use stays here, as does the one rule between data events: a
//! backfill burst needs a dropout on its surface that has ended by then.

use std::collections::HashMap;

use bess_core::state::ScenarioId;
use bess_core::PlantConfig;
use serde_saphyr::Spanned;

use crate::error::{LoadError, Position, Reason};
use crate::scenario::{DataFaultKind, Fault, Scenario, ScenarioEvent};
use crate::schema::target::Target;
use crate::schema::{duration, EventSpec, File};

mod data;
mod physical;

/// A rule broken by one event: the field at fault and why.
type Refusal = (&'static str, Reason);

/// Turn a parsed file into a scenario, or name the first rule it breaks.
pub(crate) fn validate(
    file: File,
    cfg: &PlantConfig,
    content_hash: u64,
) -> Result<Scenario, LoadError> {
    if file.schema.value != 1 {
        return Err(at(
            &file.schema,
            Reason::UnsupportedSchema(file.schema.value),
        ));
    }
    let name = &file.name.value;
    let name_ok = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !name_ok {
        return Err(at(&file.name, Reason::BadName(name.clone())));
    }
    let start_unix_s = match &file.start {
        Some(s) => Some(parse_start(&s.value).map_err(|why| {
            let value = s.value.clone();
            at(s, Reason::BadStart { value, why })
        })?),
        None => None,
    };
    if let Some(speed) = &file.speed {
        if !(1.0..=3600.0).contains(&speed.value) {
            return Err(at(speed, Reason::SpeedOutOfRange(speed.value)));
        }
    }
    let length_s = match &file.length {
        Some(length) => match duration::parse(&length.value) {
            Ok(0) => return Err(at(length, Reason::ZeroLength)),
            Ok(seconds) => Some(seconds),
            Err(e) => return Err(at(length, e.into())),
        },
        None => None,
    };

    let mut events = Vec::with_capacity(file.events.len());
    let mut last_on: HashMap<Target, (usize, u64)> = HashMap::new();
    for (index, spanned) in file.events.iter().enumerate() {
        let line = spanned.referenced.line();
        let refuse = |(field, reason): Refusal| LoadError {
            position: Position::Event { index, field, line },
            reason,
        };
        let event = event(index, &spanned.value, cfg).map_err(refuse)?;
        if let Some(length_s) = length_s.filter(|&length_s| event.at_s >= length_s) {
            return Err(refuse(("at", Reason::EventAfterLength { length_s })));
        }
        if let Fault::Physical { target, .. } = event.fault {
            if let Some(&(other, other_at)) = last_on.get(&target) {
                if event.at_s == other_at {
                    return Err(refuse(("at", Reason::SharedTick { other })));
                }
                if event.at_s < other_at {
                    return Err(refuse(("at", Reason::OutOfOrder { other })));
                }
            }
            last_on.insert(target, (index, event.at_s));
        }
        events.push(event);
    }
    for (index, spanned) in file.events.iter().enumerate() {
        if backfill_has_nothing_to_replay(index, &events) {
            return Err(LoadError {
                position: Position::Event {
                    index,
                    field: "at",
                    line: spanned.referenced.line(),
                },
                reason: Reason::BackfillWithoutDropout,
            });
        }
    }

    Ok(Scenario {
        id: ScenarioId {
            name: name.clone(),
            content_hash,
        },
        taxonomy: file.taxonomy,
        seed: file.seed,
        start_unix_s,
        speed: file.speed.map(|s| s.value),
        length_s,
        events,
    })
}

/// Whether `events[index]` is a backfill burst with no dropout on its
/// surface that has ended by its offset, wherever in the file the dropout
/// is listed.
fn backfill_has_nothing_to_replay(index: usize, events: &[ScenarioEvent]) -> bool {
    let Fault::Data(burst) = &events[index].fault else {
        return false;
    };
    if burst.fault != DataFaultKind::BackfillBurst {
        return false;
    }
    !events.iter().any(|e| match &e.fault {
        Fault::Data(d) => {
            d.surface == burst.surface
                && d.fault == DataFaultKind::Dropout
                && e.at_s + d.duration_s.unwrap_or(0) <= events[index].at_s
        }
        Fault::Physical { .. } => false,
    })
}

fn at<T>(spanned: &Spanned<T>, reason: Reason) -> LoadError {
    LoadError {
        position: Position::Line {
            line: spanned.referenced.line(),
            column: spanned.referenced.column(),
        },
        reason,
    }
}

fn event(index: usize, spec: &EventSpec, cfg: &PlantConfig) -> Result<ScenarioEvent, Refusal> {
    let at_s = duration::parse(&spec.at).map_err(|e| ("at", e.into()))?;
    let fault = match (&spec.target, spec.surface) {
        (Some(_), Some(_)) => return Err(("surface", Reason::BothTargetAndSurface)),
        (None, None) => return Err(("target", Reason::NeitherTargetNorSurface)),
        (Some(path), None) => physical::physical(spec, path, cfg)?,
        (None, Some(surface)) => Fault::Data(data::data(spec, surface, cfg)?),
    };
    Ok(ScenarioEvent { index, at_s, fault })
}

/// Refuse any parameter the fault does not take.
fn only_taken(spec: &EventSpec, fault: &'static str, takes: &[&str]) -> Result<(), Refusal> {
    let given = [
        ("rate_pct_per_month", spec.rate_pct_per_month.is_some()),
        ("points", spec.points.is_some()),
        ("exponent", spec.exponent.is_some()),
        ("mhz", spec.mhz.is_some()),
        ("pct", spec.pct.is_some()),
        ("revert", spec.revert.is_some()),
        ("rate_per_min", spec.rate_per_min.is_some()),
        ("drift_s_per_h", spec.drift_s_per_h.is_some()),
    ];
    match given
        .into_iter()
        .find(|&(p, present)| present && !takes.contains(&p))
    {
        Some((parameter, _)) => Err((parameter, Reason::ParameterNotTaken { fault, parameter })),
        None => Ok(()),
    }
}

/// `YYYY-MM-DDThh:mm:ssZ` to unix seconds.
fn parse_start(text: &str) -> Result<i64, &'static str> {
    use bess_data::calendar::{civil_from_days, days_from_civil};
    if text.contains(['.', ',']) {
        return Err("whole seconds only, no fraction");
    }
    if !text.ends_with('Z') {
        return Err("UTC only: end with Z, no offset");
    }
    let bytes = text.as_bytes();
    let shape = bytes.len() == 20
        && bytes.iter().enumerate().all(|(i, &c)| match i {
            4 | 7 => c == b'-',
            10 => c == b'T',
            13 | 16 => c == b':',
            19 => c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !shape {
        return Err("the form is YYYY-MM-DDThh:mm:ssZ");
    }
    let num = |r: std::ops::Range<usize>| text[r].parse::<i64>().expect("digits checked");
    let date = (num(0..4), num(5..7), num(8..10));
    let (hour, minute, second) = (num(11..13), num(14..16), num(17..19));
    let days = days_from_civil(date.0, date.1, date.2);
    let real_date = (1..=12).contains(&date.1) && civil_from_days(days) == date;
    if !real_date || hour > 23 || minute > 59 || second > 59 {
        return Err("not a calendar date and time");
    }
    Ok(days * 86_400 + hour * 3600 + minute * 60 + second)
}
