//! The generated CALIBRATION.md block: a pure function of the record.

use std::fmt::Write as _;

use super::{BEGIN, END};
use crate::bands::{self, Band};
use crate::run::Kpis;

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
    write_alarms(&mut out, kpis);
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

fn write_alarms(out: &mut String, kpis: &Kpis) {
    let a = &kpis.alarms;
    let _ = write!(
        out,
        "Alarms raised over the run: {} trips (gate: none), {} warnings.\n\n",
        thousands_int(a.trips_raised),
        thousands_int(a.warnings_raised)
    );
    if a.raised_by_alarm.is_empty() {
        return;
    }
    out.push_str("| Alarm | Raised |\n|---|---|\n");
    for (name, count) in &a.raised_by_alarm {
        let _ = writeln!(out, "| `{name}` | {} |", thousands_int(*count));
    }
    out.push('\n');
}

/// A count with thousands separated.
fn thousands_int(value: u64) -> String {
    group(&value.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

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
