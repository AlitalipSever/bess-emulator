//! The file format, v1, as serde reads it.
//!
//! Field for field what a scenario author writes, `deny_unknown_fields` at
//! every level: a misspelled parameter is a scenario that does not do what
//! its author thinks, and the parser saying so is the cheapest review there
//! is. Durations and target paths stay strings here and are parsed by
//! validation, so a bad one reports its event and field. The one-of rule
//! and the fault matrices are validation too: serde's untagged enums can
//! only say "did not match any variant", which an author cannot act on.

use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_saphyr::Spanned;

pub mod duration;
pub mod selector;
pub mod target;

/// A whole scenario file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct File {
    pub schema: Spanned<u32>,
    pub name: Spanned<String>,
    #[serde(default)]
    pub taxonomy: Option<Taxonomy>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub start: Option<Spanned<String>>,
    #[serde(default)]
    pub speed: Option<Spanned<f64>>,
    #[serde(default)]
    pub length: Option<Spanned<String>>,
    pub events: Vec<Spanned<EventSpec>>,
}

/// One event as written: either a physical fault on a `target` or a data
/// fault on a `surface`, with whatever parameters its fault takes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventSpec {
    pub at: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub surface: Option<Surface>,
    pub fault: String,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub rate_pct_per_month: Option<Rate>,
    #[serde(default)]
    pub points: Option<String>,
    #[serde(default)]
    pub exponent: Option<i32>,
    /// `frequency_offset`: millihertz added to the driver's frequency.
    #[serde(default)]
    pub mhz: Option<i32>,
    #[serde(default)]
    pub rate_per_min: Option<f64>,
    #[serde(default)]
    pub drift_s_per_h: Option<f64>,
}

/// The failure category a scenario illustrates; phase 5's library requires
/// it and its gate reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Taxonomy {
    /// Balance of system: HVAC, PCS, protection.
    Bos,
    /// Controls and communication.
    Controls,
    /// The cells themselves.
    Cell,
    /// Calendar effects: seasons, DST days.
    Calendar,
    /// Planned outages.
    Maintenance,
    /// The grid's own events: frequency excursions (M4 phase 1).
    Grid,
}

impl Taxonomy {
    /// The tag as a file spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Bos => "bos",
            Self::Controls => "controls",
            Self::Cell => "cell",
            Self::Calendar => "calendar",
            Self::Maintenance => "maintenance",
            Self::Grid => "grid",
        }
    }
}

/// The protocol surface a data fault corrupts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The MQTT telemetry stream.
    Mqtt,
    /// The Modbus register bank.
    Modbus,
}

impl Surface {
    /// The name as the file spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Modbus => "modbus",
        }
    }
}

/// A rate given as one number or as a `[lo, hi]` range drawn at load.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Rate {
    /// One value.
    Fixed(f64),
    /// Drawn uniformly from `[lo, hi)` when the scenario is resolved.
    Range(f64, f64),
}

impl<'de> Deserialize<'de> for Rate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct RateVisitor;
        impl<'de> Visitor<'de> for RateVisitor {
            type Value = Rate;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number or a [lo, hi] range")
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Rate, E> {
                Ok(Rate::Fixed(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Rate, E> {
                Ok(Rate::Fixed(v as f64))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Rate, E> {
                Ok(Rate::Fixed(v as f64))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Rate, A::Error> {
                let lo = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let hi = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                if seq.next_element::<f64>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &self));
                }
                Ok(Rate::Range(lo, hi))
            }
        }
        d.deserialize_any(RateVisitor)
    }
}
