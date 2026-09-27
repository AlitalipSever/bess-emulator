//! The published record and the document generated from it.
//!
//! Two artifacts leave a run. The JSON record is the machine-readable one and
//! the one CI compares against; the CALIBRATION.md block is rendered from
//! that record, never from a second measurement. Keeping the document a pure
//! function of the record is what makes the check for a stale document a
//! string comparison rather than a float comparison, and float comparisons
//! across architectures are how generated documents become flaky.

use serde_json::Value;

use crate::run::Kpis;

mod render;

pub use render::render;

/// Opening marker of the generated block in CALIBRATION.md.
pub const BEGIN: &str = "<!-- bess-bench:begin m1-annual -->";
/// Closing marker of the generated block in CALIBRATION.md.
pub const END: &str = "<!-- bess-bench:end m1-annual -->";

/// How far a published figure may sit from a fresh measurement before the
/// record counts as stale. Both sides are rounded to the digits they publish,
/// so equality is the normal case; the tolerance absorbs a rounding boundary
/// landing differently on another architecture, and is far tighter than any
/// physics change would be.
const STALE_TOLERANCE: f64 = 1.0e-3;

/// Serialize a record the way it is committed: pretty, newline-terminated.
pub fn to_json(kpis: &Kpis) -> String {
    let mut text = serde_json::to_string_pretty(kpis).expect("KPIs serialize");
    text.push('\n');
    text
}

/// Serialize the study series the way it is committed.
pub fn series_to_json(series: &crate::series::StudySeries) -> String {
    let mut text = serde_json::to_string_pretty(series).expect("series serialize");
    text.push('\n');
    text
}

/// Parse a committed study series.
pub fn series_from_json(text: &str) -> Result<crate::series::StudySeries, String> {
    serde_json::from_str(text).map_err(|err| format!("cannot read the study series: {err}"))
}

/// Parse a committed record.
pub fn from_json(text: &str) -> Result<Kpis, String> {
    serde_json::from_str(text).map_err(|err| format!("cannot read the calibration record: {err}"))
}

/// Compare a fresh measurement against the committed record, field by field.
/// Returns one line per difference, empty when the record is current.
pub fn drift(measured: &Kpis, published: &Kpis) -> Vec<String> {
    let a = serde_json::to_value(measured).expect("KPIs serialize");
    let b = serde_json::to_value(published).expect("KPIs serialize");
    let mut out = Vec::new();
    compare(&mut out, "", &a, &b);
    out
}

/// Walk two serialized records together. Walking the serialization rather
/// than the struct means a field added later is compared without anyone
/// remembering to add it here.
fn compare(out: &mut Vec<String>, path: &str, measured: &Value, published: &Value) {
    match (measured, published) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, a_value) in a {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match b.get(key) {
                    Some(b_value) => compare(out, &child, a_value, b_value),
                    None => out.push(format!("{child}: missing from the published record")),
                }
            }
            // A map keyed by what happened (alarm counts) loses a key when
            // something stops happening; that is drift too.
            for key in b.keys().filter(|key| !a.contains_key(*key)) {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                out.push(format!("{child}: published but no longer measured"));
            }
        }
        // Measured quantities carry a tolerance; the integers do not. Seed,
        // start time, day count and tick count are the identity of the run
        // rather than results of it, and a tolerance on a Unix timestamp is
        // wide enough to swallow a run that started in a different month.
        (Value::Number(a), Value::Number(b)) if a.is_f64() && b.is_f64() => {
            let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) else {
                return;
            };
            let scale = a.abs().max(b.abs()).max(1.0e-3);
            if (a - b).abs() > STALE_TOLERANCE * scale {
                out.push(format!("{path}: measured {a}, published {b}"));
            }
        }
        (a, b) if a != b => out.push(format!("{path}: measured {a}, published {b}")),
        _ => {}
    }
}

/// Replace the generated block in a document, keeping everything else.
pub fn splice(document: &str, block: &str) -> Result<String, String> {
    let start = document
        .find(BEGIN)
        .ok_or_else(|| format!("no `{BEGIN}` marker in the document"))?;
    let end = document
        .find(END)
        .ok_or_else(|| format!("no `{END}` marker in the document"))?;
    if end < start {
        return Err("the generated-block markers are out of order".to_string());
    }
    let mut out = String::with_capacity(document.len() + block.len());
    out.push_str(&document[..start]);
    out.push_str(block);
    out.push_str(&document[end + END.len()..]);
    Ok(out)
}

/// Read the generated block out of a document.
pub fn extract(document: &str) -> Result<&str, String> {
    let start = document
        .find(BEGIN)
        .ok_or_else(|| format!("no `{BEGIN}` marker in the document"))?;
    let end = document
        .find(END)
        .ok_or_else(|| format!("no `{END}` marker in the document"))?;
    if end < start {
        return Err("the generated-block markers are out of order".to_string());
    }
    Ok(&document[start..end + END.len()])
}

/// A record shaped like the committed one, shared by the tests of this
/// module and of `render`.
#[cfg(test)]
pub(crate) fn fixture() -> Kpis {
    use crate::alarms::AlarmKpis;
    use crate::run::{EnergyKpis, HvacKpis, LossKpis, RunRecord, ThermalKpis};
    let mut raised_by_alarm = std::collections::BTreeMap::new();
    raised_by_alarm.insert("block.setpoint_not_met".to_string(), 730);
    raised_by_alarm.insert("site.power_limited".to_string(), 730);
    Kpis {
        run: RunRecord {
            site_id: "GW-01".to_string(),
            engine_version: "0.2.0".to_string(),
            seed: 7,
            start_unix_s: 1_767_225_600,
            days: 365,
            ticks: 31_536_000,
        },
        energy: EnergyKpis {
            import_mwh: 38_000.0,
            export_mwh: 33_000.0,
            round_trip_efficiency: 0.8684,
            round_trip_efficiency_excluding_aux: 0.8901,
            equivalent_full_cycles: 164.0,
            stored_delta_mwh: -3.4,
            aux_share_of_import: 0.038,
            aux_share_of_export: 0.043,
            balance_residual_share: 0.000_001,
        },
        losses: LossKpis {
            battery_mwh: 900.0,
            pcs_mwh: 1_100.0,
            transformer_mwh: 1_500.0,
            aux_hvac_mwh: 700.0,
            aux_bms_mwh: 302.0,
            aux_pcs_standby_mwh: 55.0,
            aux_controls_mwh: 131.0,
            aux_lighting_and_safety_mwh: 87.0,
        },
        thermal: ThermalKpis {
            ambient_min_c: -12.0,
            ambient_max_c: 35.0,
            container_air_min_c: 10.0,
            container_air_max_c: 29.0,
            cell_min_c: 11.0,
            cell_max_c: 38.0,
        },
        hvac: HvacKpis {
            stage1_duty: 0.1,
            stage2_duty: 0.01,
            heat_duty: 0.002,
            compressor_starts_per_container: 4_000.0,
        },
        alarms: AlarmKpis {
            trips_raised: 0,
            warnings_raised: 1_460,
            raised_by_alarm,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kpis() -> Kpis {
        fixture()
    }

    #[test]
    fn a_record_that_matches_the_measurement_reports_no_drift() {
        assert!(drift(&kpis(), &kpis()).is_empty());
    }

    #[test]
    fn every_published_figure_is_watched_for_drift() {
        // Walk the serialized record and move each number in turn. A field
        // the comparison forgets is a figure that can rot in the document
        // without CI noticing, so each one has to be caught.
        let base = serde_json::to_value(kpis()).expect("serialize");
        let mut checked = 0;
        for (section, fields) in base.as_object().expect("object") {
            for (field, value) in fields.as_object().expect("section object") {
                let mut moved = base.clone();
                moved[section][field] = disturb(value);
                let stale: Kpis = serde_json::from_value(moved).expect("still a record");
                let found = drift(&kpis(), &stale);
                assert!(
                    found.iter().any(|line| {
                        line.starts_with(&format!("{section}.{field}:"))
                            || line.starts_with(&format!("{section}.{field}."))
                    }),
                    "moving {section}.{field} was not reported: {found:?}"
                );
                checked += 1;
            }
        }
        // Every field of every section: 6 run, 9 energy, 8 loss, 6 thermal,
        // 4 HVAC, 3 alarms. Pinned so a field added without a thought about
        // whether it belongs in the published record fails here.
        assert_eq!(checked, 36, "the record changed shape");
    }

    /// Move a value far enough to be drift, without changing its JSON type:
    /// a record that no longer deserializes would prove nothing.
    fn disturb(value: &Value) -> Value {
        if let Some(number) = value.as_f64() {
            if value.is_f64() {
                return serde_json::json!(number * 1.5 + 1.0);
            }
        }
        if let Some(number) = value.as_u64() {
            return serde_json::json!(number + 1);
        }
        if let Some(number) = value.as_i64() {
            return serde_json::json!(number + 1);
        }
        if let Some(text) = value.as_str() {
            return serde_json::json!(format!("{text}-moved"));
        }
        if let Some(map) = value.as_object() {
            let mut moved = map.clone();
            moved.insert("moved.alarm".to_string(), serde_json::json!(1));
            return Value::Object(moved);
        }
        panic!("unexpected value in the record: {value}");
    }

    #[test]
    fn a_stale_document_is_a_string_comparison() {
        let block = render(&kpis());
        let document = format!("before\n\n{block}\n\nafter\n");
        assert_eq!(extract(&document).expect("block"), block);
        let spliced = splice(&document, &block).expect("splice");
        assert_eq!(spliced, document);
        assert!(document.contains("| Imported | 38 000.0 MWh |"));
    }

    #[test]
    fn splicing_replaces_only_the_generated_block() {
        let document = format!("keep me\n{BEGIN}\nold\n{END}\nkeep me too\n");
        let spliced = splice(&document, &format!("{BEGIN}\nnew\n{END}")).expect("splice");
        assert_eq!(spliced, "keep me\n<!-- bess-bench:begin m1-annual -->\nnew\n<!-- bess-bench:end m1-annual -->\nkeep me too\n");
    }
}
