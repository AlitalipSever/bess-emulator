//! The alarm tree on the scraping surface: how many events the kernel has
//! emitted, and which alarms are active, on how many nodes.
//!
//! Both families read the state rather than tallying events in the shell.
//! The counter is the kernel's own log count, so it agrees with the Modbus
//! event counter and the MQTT sequence numbers; a by-severity counter would
//! have needed a second tally that disagreed with the log after any restart.
//! Severity lives on the gauges instead, where the state carries it.

use std::fmt::Write as _;

use bess_core::alarms::layout::{block, rack, site};
use bess_core::alarms::{has_bit, Severity};
use bess_core::SiteState;

use super::metric;
use crate::events::severity_name;

/// Event counter and active-alarm gauges.
pub(super) fn alarm_metrics(out: &mut String, s: &SiteState) {
    metric(
        out,
        "bess_events_total",
        "counter",
        "Events the kernel has emitted: alarm raises and clears, and PCS state changes. \
         The count the Modbus event counter wraps and the MQTT sequence numbers run on.",
        s.event_log.count as f64,
    );

    let _ = writeln!(
        out,
        "# HELP bess_alarms_active Nodes with the alarm active, per laid-out alarm bit.\n\
         # TYPE bess_alarms_active gauge"
    );
    let racks: Vec<u32> = s.racks().map(|r| r.alarm_bits).collect();
    let blocks: Vec<u32> = s.blocks.iter().map(|b| b.alarm_bits).collect();
    active_gauges(out, "rack", rack::NAMES, &racks);
    active_gauges(out, "block", block::NAMES, &blocks);
    active_gauges(out, "site", site::NAMES, &[s.alarm_bits]);
}

/// One `bess_alarms_active` sample per laid-out bit of a word: on how many
/// of `words` the bit is set.
fn active_gauges(out: &mut String, word: &str, names: &[(u8, &str)], words: &[u32]) {
    for &(bit, name) in names {
        let active = words.iter().filter(|&&w| has_bit(w, bit)).count();
        let _ = writeln!(
            out,
            "bess_alarms_active{{alarm=\"{word}.{name}\",severity=\"{}\"}} {active}",
            severity_name(Severity::of_bit(bit)),
        );
    }
}
