//! The plant hardware in the tree: blocks, PCS, containers, HVAC, racks.

use serde::{Deserialize, Serialize};

/// One power block: a PCS and its containers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockState {
    /// Power conversion system of this block.
    pub pcs: PcsState,
    /// Battery containers, index 0..containers_per_block.
    pub containers: Vec<ContainerState>,
    /// Block alarm word, laid out in `alarms::layout::block`.
    pub alarm_bits: u32,
    /// How long the PCS has been missing its setpoint by more than the
    /// raise threshold, s. State because the deadband has to survive a
    /// checkpoint mid-miss.
    pub setpoint_miss_s: f64,
}

/// PCS operating state (full state machine arrives in M3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcsOpState {
    /// Energized, not converting.
    Standby,
    /// Converting power.
    Run,
    /// Tripped; leaves only through an operator reset
    /// (`Simulation::reset_alarms`) or a scenario action.
    Fault,
}

/// Power conversion system state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcsState {
    /// Operating state.
    pub op_state: PcsOpState,
    /// AC-side setpoint received from the plant controller, W.
    pub p_ac_setpoint_w: f64,
    /// AC-side power actually converted, W (positive = discharge).
    pub p_ac_w: f64,
    /// DC-side power, W (positive = discharge).
    pub p_dc_w: f64,
    /// Conversion loss during the last tick, W.
    pub loss_w: f64,
}

/// One battery container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerState {
    /// Bulk air temperature inside the container, degrees Celsius.
    pub air_temp_c: f64,
    /// HVAC unit state.
    pub hvac: HvacState,
    /// Racks, index 0..racks_per_container.
    pub racks: Vec<RackState>,
}

/// What the container HVAC unit is doing.
///
/// Cooling is staged because the reference container carries more than one
/// unit; heating is a single electric mode, since that is what container
/// datasheets fit (see CALIBRATION.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HvacMode {
    /// Controls energized, no compressor and no heater.
    Off,
    /// One cooling unit running.
    Cool1,
    /// Both cooling units running.
    Cool2,
    /// Electric heating.
    Heat,
}

/// Container HVAC state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvacState {
    /// Operating mode.
    pub mode: HvacMode,
    /// Seconds of compressor protection left: how long before a compressor
    /// may start or stop again. It guards the compressors and nothing else,
    /// so electric heating starts and stops on its band regardless. Part of
    /// the state because a resumed run has to continue mid-cycle rather than
    /// restart the timer.
    pub compressor_hold_s: f64,
    /// Electrical power drawn, W.
    pub electrical_w: f64,
    /// Heat currently being moved, W (thermal). Positive when cooling
    /// removes heat from the container, negative when heating adds it.
    pub thermal_w: f64,
    /// The unit has failed: no compressor and no heater run whatever the
    /// air does, until it is repaired. Set by a fault, never by the model.
    pub failed: bool,
}

/// One battery rack (one series string).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RackState {
    /// Whether the rack is connected to the DC bus.
    pub in_service: bool,
    /// State of charge, 0..1.
    pub soc: f64,
    /// State of health, 0..1 (aging arrives in M5; 1.0 until then).
    pub soh: f64,
    /// Terminal voltage, V.
    pub voltage_v: f64,
    /// Current, A (positive = discharge).
    pub current_a: f64,
    /// Representative cell temperature, degrees Celsius.
    pub cell_temp_c: f64,
    /// Polarization voltage of the RC branch in the equivalent circuit, V.
    pub polarization_v: f64,
    /// Per-rack manufacturing spread multiplier on internal resistance
    /// (drawn once at initialization from the seeded PRNG).
    pub resistance_scale: f64,
    /// Fixed thermal offset of this rack relative to container air, K
    /// (position in the airflow; drawn once at initialization).
    pub temp_offset_c: f64,
    /// Active alarm bits (alarm tree arrives in M2; 0 until then).
    pub alarm_bits: u32,
    /// SoC spread between the highest and lowest cell of the string, 0..1.
    /// The BMS's dynamic state: throughput widens it, balancing narrows it.
    /// One scalar per rack, never per-cell state: SCADA publishes a min and a
    /// max, and that is the truth this is accountable to.
    pub cell_dsoc: f64,
    /// Highest minus lowest cell voltage, V: `cell_dsoc` read through the
    /// OCV curve at the rack's SoC. Tiny on the LFP plateau, large at the
    /// top knee, which is why real BMSs balance there.
    pub cell_dv_v: f64,
    /// Whether the bleed resistors are on. Status, not an alarm.
    pub balancing_active: bool,
}
