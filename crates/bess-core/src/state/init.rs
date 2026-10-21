//! The tree at tick 0: a pure function of configuration, seed and start.

use super::{
    AuxPower, BlockMode, BlockState, BreakerState, ContainerState, EmsMode, EmsState,
    EnergyAccounting, HvacMode, HvacState, PcsOpState, PcsState, RackState, RevenueMeter, SiteMeta,
    SiteState, SubstationState,
};
use crate::alarms::EventLog;
use crate::config::PlantConfig;
use crate::kernel::Weather;
use crate::rng::Rng;

/// Salt that separates the commissioning-spread stream from the main one.
const SPREAD_STREAM: u64 = 0x5b12_ead0_ba1a_4ce5;

/// Salt that separates the current-sensor stream from the others.
const SENSOR_STREAM: u64 = 0x7a3c_91e0_5d4b_28f1;

/// Bound on the rack current sensor's offset, A: a shunt-based pack sensor
/// of the 500 A class publishes 75 mA (CALIBRATION.md, M2.5 reported SoC).
/// Drawn uniformly inside the bound; the bound is a specification limit,
/// not a distribution, and is labeled so there.
const SENSOR_OFFSET_BOUND_A: f64 = 0.075;

/// Bound on the rack current sensor's gain error, relative: 0.4 % of
/// reading, the same sensor's total accuracy over temperature.
const SENSOR_GAIN_BOUND: f64 = 0.004;

/// SoC spread a rack leaves the factory with, drawn uniformly per rack.
/// Strings are top-balanced at commissioning; what remains is the spread a
/// balancing run leaves behind, well under one percent.
const COMMISSIONING_DSOC: (f64, f64) = (0.002, 0.006);

impl SiteState {
    /// Build the initial state tree for a configuration. Per-rack spreads
    /// (initial SoC, resistance, thermal position) are drawn from the seeded
    /// PRNG, so the whole tree is a pure function of `(cfg, seed, start)`.
    pub fn new(cfg: &PlantConfig, seed: u64, start_unix_s: i64) -> Self {
        let mut rng = Rng::from_seed(seed);
        // The commissioning spread draws from a stream of its own, so adding
        // it left every draw that predates it where it was.
        let mut spread_rng = Rng::from_seed(seed ^ SPREAD_STREAM);
        let mut sensor_rng = Rng::from_seed(seed ^ SENSOR_STREAM);
        let ambient_c = 12.0;
        let blocks = (0..cfg.blocks)
            .map(|_| BlockState {
                mode: BlockMode::InService,
                pcs: PcsState {
                    op_state: PcsOpState::Standby,
                    p_ac_setpoint_w: 0.0,
                    p_ac_w: 0.0,
                    p_dc_w: 0.0,
                    loss_w: 0.0,
                    dc_voltage_v: cfg.rack.nominal_v(),
                    state_elapsed_s: 0.0,
                },
                containers: (0..cfg.containers_per_block)
                    .map(|_| ContainerState {
                        air_temp_c: 20.0,
                        hvac: HvacState {
                            mode: HvacMode::Off,
                            compressor_hold_s: 0.0,
                            electrical_w: 0.0,
                            thermal_w: 0.0,
                            failed: false,
                        },
                        racks: (0..cfg.racks_per_container)
                            .map(|_| new_rack(cfg, &mut rng, &mut spread_rng, &mut sensor_rng))
                            .collect(),
                    })
                    .collect(),
                alarm_bits: 0,
                setpoint_miss_s: 0.0,
            })
            .collect();

        Self {
            meta: SiteMeta {
                site_id: cfg.site_id.clone(),
                seed,
                start_unix_s,
                scenario: None,
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
                blocks_released: cfg.blocks,
                next_release_s: 0.0,
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
                station_service_wh: 0.0,
                revenue: RevenueMeter::new(start_unix_s),
            },
            aux: AuxPower::default(),
            energy: EnergyAccounting::default(),
            blocks,
            alarm_bits: 0,
            event_log: EventLog::default(),
            pending_events: Vec::new(),
        }
    }
}

/// One rack at commissioning: its draws from the three streams, in the
/// order the streams were added, so no earlier draw moves.
fn new_rack(
    cfg: &PlantConfig,
    rng: &mut Rng,
    spread_rng: &mut Rng,
    sensor_rng: &mut Rng,
) -> RackState {
    let soc = (cfg.initial_soc + rng.uniform(-0.005, 0.005)).clamp(0.0, 1.0);
    RackState {
        in_service: true,
        soc,
        soh: 1.0,
        voltage_v: cfg.rack.nominal_v(),
        current_a: 0.0,
        cell_temp_c: 20.0,
        polarization_v: 0.0,
        hysteresis_v: 0.0,
        // The BMS is told the truth at commissioning
        // and walks away from it from there.
        soc_reported: soc,
        sensor_offset_a: sensor_rng.uniform(-SENSOR_OFFSET_BOUND_A, SENSOR_OFFSET_BOUND_A),
        sensor_gain: 1.0 + sensor_rng.uniform(-SENSOR_GAIN_BOUND, SENSOR_GAIN_BOUND),
        resistance_scale: rng.uniform(0.97, 1.03),
        temp_offset_c: rng.uniform(-1.5, 1.5),
        alarm_bits: 0,
        cell_dsoc: spread_rng.uniform(COMMISSIONING_DSOC.0, COMMISSIONING_DSOC.1),
        cell_dv_v: 0.0,
        balancing_active: false,
        self_discharge_excess_per_s: 0.0,
    }
}
