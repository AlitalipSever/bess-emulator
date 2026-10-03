//! The signal map: one table drives every projection.
//!
//! Each `Point` names a signal, says how to read it from the state tree,
//! and how to encode it as Modbus registers. The Modbus banks, the MQTT
//! topics, and the published CSV reference are all generated from this one
//! table, so they cannot drift apart.
//!
//! Register conventions: 32-bit values span two registers, high word first.
//! `scale` converts physical units to register counts (register = physical
//! value x scale, rounded). Addresses are stable per COMPATIBILITY.md once
//! published: adding registers is a minor change, moving them is major.
//!
//! The table is cut by what a reader is asking about: the site's telemetry,
//! the control surface, each block's telemetry, and the alarm points, which
//! sit at both site and block addresses but share one publication rule.

mod alarms;
mod block;
mod control;
mod csv;
mod encode;
mod site;

use bess_core::config::PlantConfig;
use bess_core::state::SiteState;

pub use control::{HOLDING_MODE_ADDR, HOLDING_SETPOINT_ADDR};
pub use csv::dump_signal_map_csv;
pub use encode::write_banks;

/// Version of the published signal map, semver, independent of the crate
/// version per COMPATIBILITY.md.
///
/// The map published through crate v0.2.0 carried no version number at all;
/// it is recorded as 0.1.0 so the sequence has a beginning. 0.2.0 is M1's
/// addition of the thermal and auxiliary points: new addresses only, nothing
/// moved, nothing renamed. 0.3.0 is M2's, and the first major step: the rack
/// alarm bits behind `blockNN.alarm_bits` gained a layout, which
/// reinterprets a published point; the block and site alarm words, the event
/// counter, the spread and the derate count are additions.
pub const MAP_VERSION: &str = "0.3.0";

/// Register space a point lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// Input registers (function code 4), read-only telemetry.
    Input,
    /// Holding registers (function codes 3/6/16), the control surface.
    Holding,
}

/// Publication class: when MQTT publishes a point. Modbus refreshes every
/// register every tick whatever its class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Every simulated second.
    Fast,
    /// Every 10 simulated seconds.
    Medium,
    /// Every 60 simulated seconds.
    Slow,
    /// When the value changes, retained, never on a cadence: report by
    /// exception. Alarm words and the counts derived from them only move
    /// when an alarm edge happens, so a cadence would repeat them for
    /// nothing and could still miss an edge between two samples.
    Event,
}

impl Class {
    /// Minimum simulated seconds between publications, or `None` for a
    /// point published on change.
    pub fn period_s(self) -> Option<i64> {
        match self {
            Class::Fast => Some(1),
            Class::Medium => Some(10),
            Class::Slow => Some(60),
            Class::Event => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Class::Fast => "fast",
            Class::Medium => "medium",
            Class::Slow => "slow",
            Class::Event => "event",
        }
    }
}

/// Register encoding of a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// One unsigned register.
    U16,
    /// One signed register.
    I16,
    /// Two registers, unsigned, high word first.
    U32,
    /// Two registers, signed, high word first.
    I32,
}

impl Encoding {
    /// Number of registers the encoding occupies.
    pub fn words(self) -> u16 {
        match self {
            Encoding::U16 | Encoding::I16 => 1,
            Encoding::U32 | Encoding::I32 => 2,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Encoding::U16 => "u16",
            Encoding::I16 => "i16",
            Encoding::U32 => "u32",
            Encoding::I32 => "i32",
        }
    }
}

/// One signal: where it comes from and how every surface presents it.
pub struct Point {
    /// Dotted path, e.g. `site.poi.active_power_w` (MQTT topic uses `/`).
    pub name: String,
    /// Physical unit of the extracted value.
    pub unit: &'static str,
    /// Publication class.
    pub class: Class,
    /// `true` if the point is writable (holding space control surface).
    pub writable: bool,
    /// Register encoding.
    pub encoding: Encoding,
    /// Register counts per physical unit.
    pub scale: f64,
    /// First register address inside `space`.
    pub addr: u16,
    /// Register space.
    pub space: Space,
    /// Reads the physical value from the state tree.
    pub extract: Box<dyn Fn(&SiteState) -> f64 + Send + Sync>,
}

/// Size of the input register bank: the site, then both per-block ranges
/// for 20 power blocks.
pub const INPUT_BANK_LEN: usize = BLOCK_EXT_BASE as usize + 20 * BLOCK_STRIDE as usize;
/// Size of the holding register bank.
pub const HOLDING_BANK_LEN: usize = 3;

/// First register of block `b`'s original range in the input space.
const BLOCK_BASE: u16 = 1000;
/// First register of block `b`'s second range. The original stride filled
/// up in M1 (base+0 to 9 are taken), and moving published addresses is a
/// major change, so M2's block points open a second range rather than
/// renumbering the first. The mechanism M1 reserved for exactly this case.
const BLOCK_EXT_BASE: u16 = 2000;
/// Register stride between blocks, in both ranges.
const BLOCK_STRIDE: u16 = 10;

/// First register of block `b` in the original per-block range.
fn block_base(b: usize) -> u16 {
    BLOCK_BASE + b as u16 * BLOCK_STRIDE
}

/// First register of block `b` in the second per-block range.
fn block_ext_base(b: usize) -> u16 {
    BLOCK_EXT_BASE + b as u16 * BLOCK_STRIDE
}

/// MQTT and CSV name prefix of block `b`.
fn block_prefix(b: usize) -> String {
    format!("block{b:02}")
}

macro_rules! point {
    ($name:expr, $unit:expr, $class:expr, $enc:expr, $scale:expr, $addr:expr, $space:expr, $extract:expr) => {
        Point {
            name: $name.into(),
            unit: $unit,
            class: $class,
            writable: matches!($space, Space::Holding),
            encoding: $enc,
            scale: $scale,
            addr: $addr,
            space: $space,
            extract: Box::new($extract),
        }
    };
}
use point;

/// Build the full signal map for a plant configuration, in address order:
/// input space first, then holding.
pub fn build_points(cfg: &PlantConfig) -> Vec<Point> {
    let mut points = site::points();
    points.extend(control::points());
    points.extend(alarms::site_points());
    for b in 0..cfg.blocks {
        points.extend(block::points(b));
        points.extend(alarms::block_points(b));
    }
    points.sort_by_key(|p| (p.space == Space::Holding, p.addr));
    points
}

/// Project a state into a fresh input bank, for the tests of every part of
/// the table.
#[cfg(test)]
fn input_bank(points: &[Point], state: &SiteState) -> Vec<u16> {
    let mut input = vec![0u16; INPUT_BANK_LEN];
    let mut holding = vec![0u16; HOLDING_BANK_LEN];
    write_banks(points, state, &mut input, &mut holding);
    input
}

/// Read a two-register unsigned value, high word first.
#[cfg(test)]
fn read_u32(bank: &[u16], addr: usize) -> u32 {
    (u32::from(bank[addr]) << 16) | u32::from(bank[addr + 1])
}

#[cfg(test)]
mod tests;
