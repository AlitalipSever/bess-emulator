//! Data faults on the read path: the tap (phase 5 design D1 to D3).
//!
//! Data faults corrupt the observation, never the state. The kernel steps
//! as if nothing happened; the surfaces read the snapshot through the tap
//! and publish what a lying device would. `Taps` is the shell's record of
//! the data events a scenario scheduled: each a window in ticks on one
//! surface, with its parameters, the points it selects, and, for a freeze,
//! the values captured when it opened. It is built from the resolved
//! schedule, so a resumed run rebuilds it from the file and the checkpoint
//! stays clean.
//!
//! What each surface does with a window (what a dropout does to a TCP
//! session, what NaN looks like in a payload) lives beside that surface,
//! in `mqtt/tap.rs` and `modbus/tap.rs`. This module holds what they
//! share: which windows are open at a tick, which points they select, and
//! the arithmetic of a value, a flap and a drift.

use std::sync::Arc;

use bess_core::TICK_SECONDS;
use bess_scenario::{DataFault, DataFaultKind, Due, Resolved, Selector, Surface};

use crate::map::{Class, Point, Space};

/// One scheduled data fault as a window in ticks.
#[derive(Debug, Clone)]
pub struct Window {
    /// The event's index in the file.
    pub index: usize,
    /// The surface it corrupts.
    pub surface: Surface,
    /// Which fault.
    pub kind: DataFaultKind,
    /// The first tick inside the window.
    pub start: u64,
    /// The first tick after it; `None` runs to the end of the run. A
    /// backfill burst is an instant: its end is its start, and the
    /// publisher replays once when the start has passed.
    pub end: Option<u64>,
    /// The points the fault selects, by index into the map.
    pub selected: Vec<bool>,
    /// `unit_error`'s power of ten.
    pub exponent: i32,
    /// `chatter`'s flaps per simulated minute.
    pub rate_per_min: f64,
    /// `timestamp_drift`'s seconds of device clock per hour.
    pub drift_s_per_h: f64,
    /// For a freeze: every point's value when the window opened, once it
    /// has.
    pub frozen: Option<Arc<Vec<f64>>>,
}

impl Window {
    /// Whether `tick` is inside the window.
    pub fn covers(&self, tick: u64) -> bool {
        tick >= self.start && self.end.is_none_or(|end| tick < end)
    }
}

/// The scenario's data faults, as the surfaces see them.
#[derive(Debug, Clone, Default)]
pub struct Taps {
    windows: Vec<Window>,
}

impl Taps {
    /// From a resolved schedule, against the map.
    pub fn new(resolved: &Resolved, points: &[Point]) -> Self {
        let windows = resolved
            .schedule
            .iter()
            .filter_map(|s| match &s.due {
                Due::Data(fault) => Some(window(s.index, s.tick, fault, points)),
                Due::Physical(_) => None,
            })
            .collect();
        Self { windows }
    }

    /// Every window, in schedule order.
    #[cfg(test)]
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Record what a freeze found when its event fired: `values` is every
    /// point's value at that tick, in map order.
    pub fn open(&mut self, index: usize, values: Vec<f64>) {
        let values = Arc::new(values);
        for w in &mut self.windows {
            if w.index == index && w.kind == DataFaultKind::Freeze {
                w.frozen = Some(Arc::clone(&values));
            }
        }
    }

    /// The windows of `surface` open at `tick`.
    pub fn open_at(&self, surface: Surface, tick: u64) -> impl Iterator<Item = &Window> {
        self.windows
            .iter()
            .filter(move |w| w.surface == surface && w.covers(tick))
    }

    /// Whether anything is open on `surface` at `tick`.
    pub fn any_open(&self, surface: Surface, tick: u64) -> bool {
        self.open_at(surface, tick).next().is_some()
    }

    /// Whether `surface` is in a dropout at `tick`.
    pub fn dropout(&self, surface: Surface, tick: u64) -> bool {
        self.open_at(surface, tick)
            .any(|w| w.kind == DataFaultKind::Dropout)
    }

    /// What point `index` reads on `surface` at `tick` when the truth is
    /// `value`: frozen where a freeze that selects it has captured, then
    /// scaled by every unit error that selects it, then NaN if a burst
    /// does.
    pub fn value(&self, surface: Surface, index: usize, tick: u64, value: f64) -> f64 {
        let mut read = value;
        for w in self.open_at(surface, tick).filter(|w| w.selected[index]) {
            match w.kind {
                DataFaultKind::Freeze => {
                    if let Some(frozen) = &w.frozen {
                        read = frozen[index];
                    }
                }
                DataFaultKind::UnitError => read *= 10f64.powi(w.exponent),
                DataFaultKind::NanBurst => read = f64::NAN,
                _ => {}
            }
        }
        read
    }

    /// Whether a chatter window that selects point `index` has its bit
    /// flipped at `tick`: a flap is one period of `60 / rate` simulated
    /// seconds, flipped through its second half. Two windows cancel where
    /// both are flipped, as two edges would.
    pub fn flipped(&self, surface: Surface, index: usize, tick: u64) -> bool {
        self.open_at(surface, tick)
            .filter(|w| w.kind == DataFaultKind::Chatter && w.selected[index])
            .fold(false, |flipped, w| {
                let flaps = (tick - w.start) as f64 * w.rate_per_min / 60.0;
                flipped ^ (flaps.fract() >= 0.5)
            })
    }

    /// How far the device clock has drifted from the plant's at `tick`,
    /// seconds, summed over the drifts open on `surface`.
    pub fn drift_s(&self, surface: Surface, tick: u64) -> f64 {
        // A fold from 0.0, not `sum`, whose empty result is -0.0: a clock
        // that is not drifting reads exactly the plant's time.
        self.open_at(surface, tick)
            .filter(|w| w.kind == DataFaultKind::TimestampDrift)
            .fold(0.0, |drift, w| {
                drift + w.drift_s_per_h * (tick - w.start) as f64 / 3600.0
            })
    }

    /// The backfill bursts of `surface` whose instant has passed by `tick`.
    pub fn backfills_due(&self, surface: Surface, tick: u64) -> impl Iterator<Item = &Window> {
        self.windows.iter().filter(move |w| {
            w.surface == surface && w.kind == DataFaultKind::BackfillBurst && w.start <= tick
        })
    }
}

fn window(index: usize, start: u64, fault: &DataFault, points: &[Point]) -> Window {
    use DataFaultKind as K;
    let end = match fault.fault {
        K::BackfillBurst => Some(start),
        _ => fault.duration_s.map(|d| start + d / TICK_SECONDS),
    };
    let selected = points
        .iter()
        .map(|p| selects(fault.fault, fault.points, p))
        .collect();
    Window {
        index,
        surface: fault.surface,
        kind: fault.fault,
        start,
        end,
        selected,
        exponent: fault.exponent.unwrap_or(0),
        rate_per_min: fault.rate_per_min.unwrap_or(0.0),
        drift_s_per_h: fault.drift_s_per_h.unwrap_or(0.0),
        frozen: None,
    }
}

/// Whether a fault of `kind` with selector `points` reaches `point`. The
/// control registers are never touched: they are the operator's words, not
/// the device's. A freeze holds every selected point; a unit error and a
/// NaN burst corrupt the cadence points, since a word is not a quantity; a
/// chatter flaps the alarm words and nothing else.
fn selects(kind: DataFaultKind, points: Option<Selector>, point: &Point) -> bool {
    use DataFaultKind as K;
    if point.space == Space::Holding {
        return false;
    }
    let named = match points {
        Some(Selector::All) => true,
        Some(Selector::Site) => point.name.starts_with("site."),
        Some(Selector::Block { block }) => point.name.starts_with(&format!("block{block:02}.")),
        Some(Selector::Pcs { block }) => point.name.starts_with(&format!("block{block:02}.pcs.")),
        None => false,
    };
    named
        && match kind {
            K::Freeze => true,
            K::UnitError | K::NanBurst => point.class != Class::Event,
            K::Chatter => point.class == Class::Event && point.name.ends_with("alarm_bits"),
            K::Dropout | K::TimestampDrift | K::BackfillBurst => false,
        }
}

#[cfg(test)]
mod tests;
