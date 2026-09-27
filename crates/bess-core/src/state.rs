//! The state tree: the single source of truth for the whole plant.
//!
//! Every external surface (Modbus map, MQTT topics, browser UI, exports) is
//! a projection of this tree. Sign convention everywhere: active power is
//! positive when discharging (exporting to the grid), negative when charging.

use serde::{Deserialize, Serialize};

use crate::alarms::EventLog;
use crate::kernel::{Event, Weather};
use crate::rng::Rng;

mod energy;
mod init;
mod plant;

pub use energy::{AuxEnergy, AuxPower, EnergyAccounting};
pub use plant::{BlockState, ContainerState, HvacMode, HvacState, PcsOpState, PcsState, RackState};

/// Root of the state tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteState {
    /// Run identity: site id, seed, simulation start time.
    pub meta: SiteMeta,
    /// Ticks elapsed since simulation start.
    pub tick: u64,
    /// Kernel PRNG; serialized so a resumed run continues the same stream.
    pub rng: Rng,
    /// Ambient conditions currently applied. The tick inputs are copied here
    /// verbatim, so the tree is self-describing: a checkpoint says what
    /// weather produced it. Same type as the input, because it is the same
    /// quantity; a plant whose own sensors read something other than the
    /// exogenous truth is an M2 fault, not a second struct.
    pub weather: Weather,
    /// Plant controller state.
    pub ems: EmsState,
    /// 110 kV substation and point of interconnection.
    pub substation: SubstationState,
    /// What the site consumed for itself during the last tick, itemized.
    pub aux: AuxPower,
    /// Cumulative loss and consumption meters for energy accounting.
    pub energy: EnergyAccounting,
    /// Power blocks, index 0..blocks.
    pub blocks: Vec<BlockState>,
    /// Site alarm word, laid out in `alarms::layout::site`. Kept in a u32
    /// like the rack word; the documented layout is the low 16 bits.
    pub alarm_bits: u32,
    /// Count and digest of every event handed out since tick 0.
    pub event_log: EventLog,
    /// Events that happened between ticks (an operator reset) and go out
    /// with the next one. Empty except in that gap; part of the tree so a
    /// checkpoint taken inside the gap loses nothing.
    pub pending_events: Vec<Event>,
}

/// Run identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteMeta {
    /// Site identifier, e.g. "GW-01".
    pub site_id: String,
    /// PRNG seed this run started from.
    pub seed: u64,
    /// Unix timestamp (UTC seconds) of tick 0.
    pub start_unix_s: i64,
}

/// Plant controller (EMS) operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmsMode {
    /// Follow the internal dispatch plan.
    FollowPlan,
    /// Follow a setpoint written by an external controller (the control
    /// surface a dispatch application under test drives).
    External,
}

/// Plant controller state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmsState {
    /// Operating mode.
    pub mode: EmsMode,
    /// Site active-power target currently pursued, W (positive = discharge).
    pub site_setpoint_w: f64,
    /// Last setpoint written by an external controller, W. Applied while in
    /// `External` mode.
    pub external_setpoint_w: f64,
    /// Dischargeable AC power available right now, W.
    pub available_discharge_w: f64,
    /// Chargeable AC power available right now, W.
    pub available_charge_w: f64,
}

/// High-voltage breaker position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakerState {
    /// Breaker open: site disconnected from the grid.
    Open,
    /// Breaker closed: site connected.
    Closed,
}

/// Substation and point-of-interconnection state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubstationState {
    /// Main HV breaker position.
    pub hv_breaker: BreakerState,
    /// Active power at the POI, W (positive = export).
    pub poi_active_power_w: f64,
    /// Reactive power at the POI, var.
    pub poi_reactive_power_var: f64,
    /// POI voltage, kV.
    pub poi_voltage_kv: f64,
    /// Grid frequency, Hz (replayed or synthetic, passed in as input).
    pub frequency_hz: f64,
    /// Transformer losses during the last tick, W.
    pub transformer_loss_w: f64,
    /// Total auxiliary consumption during the last tick (HVAC + station), W.
    pub aux_power_w: f64,
    /// Monotonic import meter at the POI, Wh.
    pub import_wh: f64,
    /// Monotonic export meter at the POI, Wh.
    pub export_wh: f64,
}

impl SiteState {
    /// Unix timestamp (UTC seconds) of the current tick.
    pub fn unix_time_s(&self) -> i64 {
        self.meta.start_unix_s + self.tick as i64
    }

    /// Mean state of charge over in-service racks (0..1, unweighted; all
    /// racks share one topology).
    pub fn average_soc(&self) -> f64 {
        let mut sum = 0.0;
        let mut n = 0u32;
        for rack in self.racks() {
            if rack.in_service {
                sum += rack.soc;
                n += 1;
            }
        }
        if n == 0 {
            0.0
        } else {
            sum / f64::from(n)
        }
    }

    /// Iterator over every rack on site.
    pub fn racks(&self) -> impl Iterator<Item = &RackState> {
        self.blocks
            .iter()
            .flat_map(|b| b.containers.iter())
            .flat_map(|c| c.racks.iter())
    }

    /// Minimum and maximum representative cell temperature on site.
    pub fn cell_temp_min_max_c(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for rack in self.racks() {
            min = min.min(rack.cell_temp_c);
            max = max.max(rack.cell_temp_c);
        }
        if min > max {
            (0.0, 0.0)
        } else {
            (min, max)
        }
    }
}

impl BlockState {
    /// Mean state of charge over this block's in-service racks.
    pub fn average_soc(&self) -> f64 {
        let mut sum = 0.0;
        let mut n = 0u32;
        for rack in self.containers.iter().flat_map(|c| c.racks.iter()) {
            if rack.in_service {
                sum += rack.soc;
                n += 1;
            }
        }
        if n == 0 {
            0.0
        } else {
            sum / f64::from(n)
        }
    }

    /// Minimum and maximum representative cell temperature in this block.
    pub fn cell_temp_min_max_c(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for rack in self.containers.iter().flat_map(|c| c.racks.iter()) {
            min = min.min(rack.cell_temp_c);
            max = max.max(rack.cell_temp_c);
        }
        if min > max {
            (0.0, 0.0)
        } else {
            (min, max)
        }
    }
}
