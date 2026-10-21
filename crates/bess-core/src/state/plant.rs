//! The plant hardware in the tree: blocks, PCS, containers, HVAC, racks.

use serde::{Deserialize, Serialize};

/// One power block: a PCS and its containers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockState {
    /// Operating mode: in service, or out of the plant for maintenance.
    pub mode: BlockMode,
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

/// What the operator has made of a block. Room for more modes later: the M3
/// electrical work will want at least a local/remote notion, which is why
/// this is an enum and its register a value, not a bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockMode {
    /// Part of the plant.
    InService,
    /// Locked out for service: asked for nothing, its converter switched
    /// off, its containers still cooled.
    Maintenance,
}

/// PCS operating state: the converter's state machine (M3 phase 2).
///
/// Three groups. Disconnected: `Off` (contactors open: after a reset,
/// under maintenance) and the startup sequence `Precharge`,
/// `ContactorClose`, `Synchronize` it walks when cleared. Connected:
/// `Ramp` (the first converting tick after synchronization; phase 2 PR2
/// gives it the ramp's duration), `Run` and `Standby`, between which an
/// idle converter flips by power as it did since M0. And `Fault`,
/// reached from any state by a trip and left only through a reset.
/// Published on Modbus with SunSpec's operating-state numbering
/// (`sunspec_code`), by name on MQTT and REST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcsOpState {
    /// Contactors open, controls energized: after a reset, under
    /// maintenance, before clearance.
    Off,
    /// Charging the DC link through the precharge resistor.
    Precharge,
    /// Closing and checking the main contactors.
    ContactorClose,
    /// Synchronizing to the grid before the gate drives enable.
    Synchronize,
    /// The first converting tick after synchronization.
    Ramp,
    /// Connected, not converting.
    Standby,
    /// Converting power.
    Run,
    /// Tripped; leaves only through an operator reset
    /// (`Simulation::reset_alarms`) or a scenario action.
    Fault,
}

impl PcsOpState {
    /// Connected to the grid: takes a share of the site target and counts
    /// toward availability.
    pub fn is_connected(self) -> bool {
        matches!(self, Self::Ramp | Self::Standby | Self::Run)
    }

    /// Inside the startup sequence.
    pub fn is_starting(self) -> bool {
        matches!(
            self,
            Self::Precharge | Self::ContactorClose | Self::Synchronize
        )
    }

    /// Converting power right now.
    pub fn is_converting(self) -> bool {
        matches!(self, Self::Ramp | Self::Run)
    }

    /// SunSpec operating-state code (models 701/702 `InvSt`): 1 off,
    /// 3 starting, 4 running, 7 fault, 8 standby. The three startup states
    /// share `starting` on the wire; MQTT and REST carry the finer name.
    pub fn sunspec_code(self) -> u16 {
        match self {
            Self::Off => 1,
            Self::Precharge | Self::ContactorClose | Self::Synchronize => 3,
            Self::Ramp | Self::Run => 4,
            Self::Fault => 7,
            Self::Standby => 8,
        }
    }
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
    /// DC bus voltage the converter sees this tick, V: the mean terminal
    /// voltage of the block's in-service racks as the last tick left them
    /// (M3 phase 1). Derived every tick, kept in the tree so a checkpoint
    /// says what the converter converted at.
    pub dc_voltage_v: f64,
    /// Seconds spent in the current startup state (`is_starting`); zero
    /// elsewhere. State, so a resume continues a precharge where it was.
    pub state_elapsed_s: f64,
    /// The setpoint after the ramp limit, W: where the follower is headed
    /// this tick (M3 phase 2 PR2). Zero while the converter is not
    /// following.
    pub setpoint_ramped_w: f64,
    /// The setpoint the converter is actually pursuing, W: the ramped
    /// target after the first-order lag. What `dc_request_w` converts and
    /// what the setpoint-miss alarm reads, so a legitimate ramp is not a
    /// miss. Published as `blockNN.pcs.setpoint_followed_kw`.
    pub setpoint_followed_w: f64,
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
    /// Hysteresis state of the open-circuit voltage, V, summed over the
    /// string: positive after charging, negative after discharging, held
    /// at rest. Added to the mean OCV curve it gives the rested voltage,
    /// which is why a rested LFP rack cannot tell its SoC from its voltage.
    /// Bounded by the envelope in `bess-models` (M2.5 phase 1).
    pub hysteresis_v: f64,
    /// State of charge as the rack's BMS reports it, 0..1: the estimator's
    /// output, not the truth. Drifts with the current sensor's error and
    /// snaps back toward `soc` at the knees of the OCV curve (M2.5 phase 2).
    /// The published SoC points carry this; `soc` stays internal.
    pub soc_reported: f64,
    /// Offset error of the rack current sensor, A, drawn once per rack at
    /// commissioning from the seeded PRNG inside the sensor's class bound.
    /// The estimator counts `current_a * sensor_gain + sensor_offset_a`.
    pub sensor_offset_a: f64,
    /// Gain error of the rack current sensor, dimensionless multiplier
    /// around 1, drawn once per rack at commissioning.
    pub sensor_gain: f64,
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
    /// How much faster than its neighbours the string's weakest cell
    /// self-discharges, SoC per second: a soft internal short. Zero on a
    /// healthy rack; set only by a fault (`FaultAction::RackSelfDischarge`),
    /// and read by the spread law, which adds it to the healthy drift.
    pub self_discharge_excess_per_s: f64,
}
