//! The state tree: the single source of truth for the whole plant.
//!
//! Every external surface (Modbus map, MQTT topics, browser UI, exports) is
//! a projection of this tree. Sign convention everywhere: active power is
//! positive when discharging (exporting to the grid), negative when charging.

use serde::{Deserialize, Serialize};

use crate::config::PlantConfig;
use crate::kernel::Weather;
use crate::rng::Rng;

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

/// The site's own consumption during the last tick, W, itemized by what
/// drew it.
///
/// The M1 gate asks for the nameplate-to-field gap to be explainable item by
/// item, which makes an unattributed lump of auxiliary power a design defect
/// rather than a simplification. Every consumer therefore reports here, and
/// `total_w` is what the substation meters as the house load.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct AuxPower {
    /// Container HVAC: compressors, heaters and fans.
    pub hvac_w: f64,
    /// Rack battery-management electronics.
    pub bms_w: f64,
    /// PCS units energized but not converting. A converting unit's own
    /// supply is already inside its conversion loss, so it is not counted
    /// twice here.
    pub pcs_standby_w: f64,
    /// Plant control, protection and SCADA.
    pub controls_w: f64,
    /// Fire and gas detection, security and access control, site and
    /// building lighting, small power. An enumerated row, not a remainder:
    /// nothing is assigned here because it did not fit elsewhere.
    pub lighting_and_safety_w: f64,
}

impl AuxPower {
    /// Total auxiliary draw, W.
    pub fn total_w(&self) -> f64 {
        self.hvac_w + self.bms_w + self.pcs_standby_w + self.controls_w + self.lighting_and_safety_w
    }
}

/// Auxiliary consumption accumulated per item, Wh.
///
/// The same five items as [`AuxPower`], integrated. `bess-bench` builds the
/// loss waterfall by reading these, rather than by re-deriving the physics
/// from a run it did not perform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuxEnergy {
    /// Container HVAC.
    pub hvac_wh: f64,
    /// Rack battery-management electronics.
    pub bms_wh: f64,
    /// PCS standby tare.
    pub pcs_standby_wh: f64,
    /// Plant control, protection and SCADA.
    pub controls_wh: f64,
    /// Fire and gas detection, security, lighting and small power.
    pub lighting_and_safety_wh: f64,
}

impl AuxEnergy {
    /// Add one tick of `power` lasting `hours`.
    pub fn accumulate(&mut self, power: &AuxPower, hours: f64) {
        self.hvac_wh += power.hvac_w * hours;
        self.bms_wh += power.bms_w * hours;
        self.pcs_standby_wh += power.pcs_standby_w * hours;
        self.controls_wh += power.controls_w * hours;
        self.lighting_and_safety_wh += power.lighting_and_safety_w * hours;
    }

    /// Total auxiliary energy, Wh.
    pub fn total_wh(&self) -> f64 {
        self.hvac_wh
            + self.bms_wh
            + self.pcs_standby_wh
            + self.controls_wh
            + self.lighting_and_safety_wh
    }
}

/// Cumulative energy accounting, Wh. All counters are monotonic; together
/// with the POI meters and the stored energy they close the site energy
/// balance, which CI enforces as an invariant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EnergyAccounting {
    /// Heat dissipated inside the racks.
    pub battery_loss_wh: f64,
    /// Conversion losses in the PCS units.
    pub pcs_loss_wh: f64,
    /// Transformer losses.
    pub transformer_loss_wh: f64,
    /// Auxiliary consumption as the substation meters it: the total that
    /// crossed into the house load.
    pub aux_wh: f64,
    /// The same energy attributed to the items that drew it. Accumulated on
    /// its own path, so `total_wh` matching `aux_wh` is a testable property
    /// rather than an arithmetic identity.
    pub aux_items: AuxEnergy,
}

/// One power block: a PCS and its containers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockState {
    /// Power conversion system of this block.
    pub pcs: PcsState,
    /// Battery containers, index 0..containers_per_block.
    pub containers: Vec<ContainerState>,
}

/// PCS operating state (full state machine arrives in M3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcsOpState {
    /// Energized, not converting.
    Standby,
    /// Converting power.
    Run,
    /// Tripped; requires a scenario or operator action to clear (M2+).
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
}

impl SiteState {
    /// Build the initial state tree for a configuration. Per-rack spreads
    /// (initial SoC, resistance, thermal position) are drawn from the seeded
    /// PRNG, so the whole tree is a pure function of `(cfg, seed, start)`.
    pub fn new(cfg: &PlantConfig, seed: u64, start_unix_s: i64) -> Self {
        let mut rng = Rng::from_seed(seed);
        let ambient_c = 12.0;
        let blocks = (0..cfg.blocks)
            .map(|_| BlockState {
                pcs: PcsState {
                    op_state: PcsOpState::Standby,
                    p_ac_setpoint_w: 0.0,
                    p_ac_w: 0.0,
                    p_dc_w: 0.0,
                    loss_w: 0.0,
                },
                containers: (0..cfg.containers_per_block)
                    .map(|_| ContainerState {
                        air_temp_c: 20.0,
                        hvac: HvacState {
                            mode: HvacMode::Off,
                            compressor_hold_s: 0.0,
                            electrical_w: 0.0,
                            thermal_w: 0.0,
                        },
                        racks: (0..cfg.racks_per_container)
                            .map(|_| RackState {
                                in_service: true,
                                soc: (cfg.initial_soc + rng.uniform(-0.005, 0.005)).clamp(0.0, 1.0),
                                soh: 1.0,
                                voltage_v: cfg.rack.nominal_v(),
                                current_a: 0.0,
                                cell_temp_c: 20.0,
                                polarization_v: 0.0,
                                resistance_scale: rng.uniform(0.97, 1.03),
                                temp_offset_c: rng.uniform(-1.5, 1.5),
                                alarm_bits: 0,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();

        Self {
            meta: SiteMeta {
                site_id: cfg.site_id.clone(),
                seed,
                start_unix_s,
            },
            tick: 0,
            rng,
            weather: Weather {
                ambient_c,
                irradiance_wm2: 0.0,
            },
            ems: EmsState {
                mode: EmsMode::FollowPlan,
                site_setpoint_w: 0.0,
                external_setpoint_w: 0.0,
                available_discharge_w: 0.0,
                available_charge_w: 0.0,
            },
            substation: SubstationState {
                hv_breaker: BreakerState::Closed,
                poi_active_power_w: 0.0,
                poi_reactive_power_var: 0.0,
                poi_voltage_kv: cfg.grid.poi_nominal_kv,
                frequency_hz: 50.0,
                transformer_loss_w: 0.0,
                aux_power_w: 0.0,
                import_wh: 0.0,
                export_wh: 0.0,
            },
            aux: AuxPower::default(),
            energy: EnergyAccounting::default(),
            blocks,
        }
    }

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
