//! The published record and the document generated from it.
//!
//! Two artifacts leave a run. The JSON record is the machine-readable one and
//! the one CI compares against; the CALIBRATION.md block is rendered from
//! that record, never from a second measurement. Keeping the document a pure
//! function of the record is what makes the check for a stale document a
//! string comparison rather than a float comparison, and float comparisons
//! across architectures are how generated documents become flaky.

use std::fmt::Write as _;

use serde_json::Value;

use crate::bands::{self, Band};
use crate::run::Kpis;

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

/// Render the generated CALIBRATION.md block from a record.
pub fn render(kpis: &Kpis) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(BEGIN);
    out.push('\n');
    write_header(&mut out, kpis);
    write_gates(&mut out, kpis);
    write_energy(&mut out, kpis);
    write_waterfall(&mut out, kpis);
    write_thermal(&mut out, kpis);
    out.push_str(END);
    out
}

fn write_header(out: &mut String, kpis: &Kpis) {
    let run = &kpis.run;
    let sentence = format!(
        "Measured by `bess-bench` on kernel {}: {} on the internal dispatch \
         plan, seed {}, {} simulated days ({} ticks) of the replayed weather \
         year. Regenerate with `cargo run --release -p bess-bench -- --write`; \
         CI fails if this block is stale.",
        run.engine_version,
        run.site_id,
        run.seed,
        run.days,
        group(&run.ticks.to_string())
    );
    let _ = write!(out, "\n{}\n\n", wrap(&sentence, 76));
}

/// Greedy wrap, so the generated prose matches the hand-written prose around
/// it. A generated block that a human can tell apart at a glance invites
/// hand-editing, and hand-editing is what the staleness check exists to
/// catch.
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / width + 1);
    let mut column = 0;
    for word in text.split_whitespace() {
        if column == 0 {
            out.push_str(word);
            column = word.chars().count();
        } else if column + 1 + word.chars().count() > width {
            out.push('\n');
            out.push_str(word);
            column = word.chars().count();
        } else {
            out.push(' ');
            out.push_str(word);
            column += 1 + word.chars().count();
        }
    }
    out
}

fn write_gates(out: &mut String, kpis: &Kpis) {
    out.push_str("| Gate | Band | Measured | Verdict | Band drawn from |\n|---|---|---|---|---|\n");
    for gate in bands::GATES {
        let value = (gate.read)(kpis);
        let verdict = if gate.band.contains(value) {
            "inside"
        } else {
            "**outside**"
        };
        let _ = writeln!(
            out,
            "| {} | {} ({}) | {} | {verdict} | {} |",
            gate.name,
            band_text(gate.band),
            gate.bound.label(),
            percent(value),
            gate.source
        );
    }
    out.push('\n');
}

fn write_energy(out: &mut String, kpis: &Kpis) {
    let e = &kpis.energy;
    out.push_str("Energy at the point of interconnection over the run:\n\n");
    out.push_str("| Quantity | Value |\n|---|---|\n");
    let _ = writeln!(out, "| Imported | {} MWh |", thousands(e.import_mwh));
    let _ = writeln!(out, "| Exported | {} MWh |", thousands(e.export_mwh));
    let _ = writeln!(
        out,
        "| Round-trip efficiency | {:.4} |",
        e.round_trip_efficiency
    );
    let _ = writeln!(
        out,
        "| Round-trip efficiency, house load removed from import | {:.4} |",
        e.round_trip_efficiency_excluding_aux
    );
    let _ = writeln!(
        out,
        "| Equivalent full cycles, exported energy over nameplate energy | {:.1} |",
        e.equivalent_full_cycles
    );
    let _ = writeln!(
        out,
        "| Stored energy, end minus start | {:.1} MWh |",
        e.stored_delta_mwh
    );
    let _ = writeln!(
        out,
        "| Auxiliary share of import / of export | {} / {} |",
        percent(e.aux_share_of_import),
        percent(e.aux_share_of_export)
    );
    let _ = writeln!(
        out,
        "| Unexplained residual | {:.5}% of throughput, gate below {}% |",
        e.balance_residual_share * 100.0,
        bands::BALANCE_RESIDUAL_MAX * 100.0
    );
    out.push('\n');
    out.push_str(&wrap(
        "Three definitions the figures above depend on. The round-trip ratio \
         is uncorrected for the stored-energy endpoints, matching how the \
         fleet figure it is gated against is computed; the row below it takes \
         the house load back out of import arithmetically rather than by \
         re-running the plant without it. Cycles count exported energy \
         against nameplate energy, not against the smaller window the BMS \
         keeps the plant inside. Container air is read every tick; the \
         rack-level temperatures below are sampled once a minute, which is \
         far inside their thermal time constant.",
        76,
    ));
    out.push_str("\n\n");
}

fn write_waterfall(out: &mut String, kpis: &Kpis) {
    let import = kpis.energy.import_mwh.max(1.0e-9);
    out.push_str("Where the energy went, each category on its own meter:\n\n");
    out.push_str("| Category | MWh | Share of import |\n|---|---|---|\n");
    for (name, value) in kpis.losses.waterfall() {
        let _ = writeln!(
            out,
            "| {name} | {} | {} |",
            thousands(value),
            percent(value / import)
        );
    }
    let total = kpis.losses.total_mwh();
    let _ = writeln!(
        out,
        "| **Total** | **{}** | **{}** |\n",
        thousands(total),
        percent(total / import)
    );
}

fn write_thermal(out: &mut String, kpis: &Kpis) {
    let t = &kpis.thermal;
    let h = &kpis.hvac;
    out.push_str("Temperatures and HVAC over the run:\n\n");
    out.push_str("| Reading | Value |\n|---|---|\n");
    let _ = writeln!(
        out,
        "| Ambient, coldest to warmest | {:.1} to {:.1} C |",
        t.ambient_min_c, t.ambient_max_c
    );
    let _ = writeln!(
        out,
        "| Container air, coldest to warmest | {:.1} to {:.1} C |",
        t.container_air_min_c, t.container_air_max_c
    );
    let _ = writeln!(
        out,
        "| Cells, coldest to warmest | {:.1} to {:.1} C |",
        t.cell_min_c, t.cell_max_c
    );
    let _ = writeln!(
        out,
        "| Duty, one unit / both units / heating | {} / {} / {} |",
        percent(h.stage1_duty),
        percent(h.stage2_duty),
        percent(h.heat_duty)
    );
    let _ = writeln!(
        out,
        "| Cooling starts per container | {} |\n",
        thousands(h.compressor_starts_per_container)
    );
}

/// A band as it is published: percentages, since both gates are shares.
fn band_text(band: Band) -> String {
    format!("{} to {}", percent(band.lo), percent(band.hi))
}

/// A share as a percentage, at two decimals.
fn percent(share: f64) -> String {
    format!("{:.2}%", share * 100.0)
}

/// A megawatt-hour figure with thousands separated, at one decimal.
fn thousands(value: f64) -> String {
    let text = format!("{value:.1}");
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), "0"));
    format!("{}.{fraction}", group(whole))
}

/// Separate thousands with a space. Digit groups only; a leading sign is
/// carried through untouched.
fn group(digits: &str) -> String {
    let (sign, digits) = match digits.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", digits),
    };
    let mut out = String::with_capacity(sign.len() + digits.len() + 4);
    out.push_str(sign);
    for (idx, ch) in digits.chars().enumerate() {
        if idx > 0 && (digits.len() - idx) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{EnergyKpis, HvacKpis, LossKpis, RunRecord, ThermalKpis};

    fn kpis() -> Kpis {
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
        }
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
                    found
                        .iter()
                        .any(|line| line.starts_with(&format!("{section}.{field}:"))),
                    "moving {section}.{field} was not reported: {found:?}"
                );
                checked += 1;
            }
        }
        // Every field of every section: 6 run, 9 energy, 8 loss, 6 thermal,
        // 4 HVAC. Pinned so a field added without a thought about whether it
        // belongs in the published record fails here.
        assert_eq!(checked, 33, "the record changed shape");
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

    #[test]
    fn generated_prose_wraps_like_the_prose_around_it() {
        let wrapped = wrap("one two three four five six seven eight nine ten", 20);
        assert_eq!(
            wrapped,
            "one two three four\nfive six seven eight\nnine ten"
        );
        assert!(wrapped.lines().all(|line| line.len() <= 20));
        assert_eq!(wrap("supercalifragilistic", 5), "supercalifragilistic");
    }

    #[test]
    fn thousands_separate_without_losing_the_sign() {
        assert_eq!(thousands(38_123.46), "38 123.5");
        assert_eq!(thousands(-3.44), "-3.4");
        assert_eq!(thousands(900.0), "900.0");
        assert_eq!(thousands(1_234_567.0), "1 234 567.0");
        assert_eq!(group("31536000"), "31 536 000");
        assert_eq!(group("7"), "7");
    }
}
