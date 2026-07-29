//! The tick loop: orchestrates the causal chain across plant layers.
//!
//! Order within a tick mirrors the real control chain: EMS picks a site
//! target, the plant controller allocates it to blocks, each PCS converts,
//! racks deliver what their BMS limits allow, heat flows into the container
//! thermal model, and the substation propagates the result to the POI.

use serde::{Deserialize, Serialize};

use crate::config::PlantConfig;
use crate::state::{BlockState, BreakerState, EmsMode, PcsOpState, SiteState};
use crate::traits::{Models, PowerLimits};
use crate::TICK_SECONDS;

/// Exogenous inputs for one tick. Compiled offline (bess-data) or generated
/// by a driver in the shell; the kernel treats them as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Inputs {
    /// Ambient air temperature, degrees Celsius.
    pub ambient_c: f64,
    /// Global horizontal irradiance, W/m2.
    pub irradiance_wm2: f64,
    /// Grid frequency at the POI, Hz.
    pub grid_frequency_hz: f64,
}

/// Discrete events emitted by the kernel (state transitions, alarms). M0
/// emits only PCS state transitions; the alarm tree arrives in M2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// A PCS changed operating state.
    PcsStateChanged {
        /// Block index.
        block: usize,
        /// Previous state.
        from: PcsOpState,
        /// New state.
        to: PcsOpState,
    },
}

/// What one block contributed during a tick.
struct BlockOutcome {
    /// AC power produced (positive) or consumed (negative), W.
    p_ac_w: f64,
    /// HVAC electrical draw of the block's containers, W.
    hvac_w: f64,
    /// Battery heat released, W.
    battery_heat_w: f64,
    /// AC-side capability given current BMS limits.
    ac_capability: PowerLimits,
    /// PCS operating-state transition, if one happened.
    pcs_transition: Option<(PcsOpState, PcsOpState)>,
}

/// A running simulation: configuration, model bundle, and the state tree.
pub struct Simulation {
    cfg: PlantConfig,
    models: Models,
    state: SiteState,
    events: Vec<Event>,
    /// Per-rack DC limits of the block currently being stepped; reused
    /// across blocks so the hot loop performs no allocation.
    rack_limits_scratch: Vec<PowerLimits>,
}

impl Simulation {
    /// Create a fresh simulation at tick 0.
    pub fn new(cfg: PlantConfig, models: Models, seed: u64, start_unix_s: i64) -> Self {
        let state = SiteState::new(&cfg, seed, start_unix_s);
        Self::from_state(cfg, models, state)
    }

    /// Resume a simulation from an existing state tree (checkpoint restore).
    pub fn from_state(cfg: PlantConfig, models: Models, state: SiteState) -> Self {
        let racks_per_block = cfg.racks_per_block();
        Self {
            cfg,
            models,
            state,
            events: Vec::with_capacity(16),
            rack_limits_scratch: vec![PowerLimits::default(); racks_per_block],
        }
    }

    /// Current state tree.
    pub fn state(&self) -> &SiteState {
        &self.state
    }

    /// Plant configuration.
    pub fn config(&self) -> &PlantConfig {
        &self.cfg
    }

    /// Unix timestamp (UTC seconds) of the current tick.
    pub fn unix_time_s(&self) -> i64 {
        self.state.unix_time_s()
    }

    /// Write or clear the external site setpoint (W, positive = discharge).
    /// `Some` switches the EMS to `External` mode, `None` returns it to the
    /// internal dispatch plan. This is the same path the Modbus/REST control
    /// surface uses.
    pub fn set_external_setpoint_w(&mut self, setpoint_w: Option<f64>) {
        if let Some(p) = setpoint_w {
            self.state.ems.mode = EmsMode::External;
            self.state.ems.external_setpoint_w = p;
        } else {
            self.state.ems.mode = EmsMode::FollowPlan;
            self.state.ems.external_setpoint_w = 0.0;
        }
    }

    /// Chemical energy currently stored across all racks, Wh.
    pub fn stored_energy_wh(&self) -> f64 {
        self.state
            .racks()
            .map(|r| self.models.cell.stored_energy_wh(r, &self.cfg.rack))
            .sum()
    }

    /// Advance the plant by one tick. Returns the events emitted.
    pub fn step(&mut self, inputs: &Inputs) -> &[Event] {
        let dt_s = TICK_SECONDS as f64;
        let wh = dt_s / 3600.0;
        self.events.clear();

        self.state.weather.ambient_c = inputs.ambient_c;
        self.state.weather.irradiance_wm2 = inputs.irradiance_wm2;

        // 1. EMS: site active-power target.
        let connected = self.state.substation.hv_breaker == BreakerState::Closed;
        let raw_target = if connected {
            match self.state.ems.mode {
                EmsMode::External => self.state.ems.external_setpoint_w,
                EmsMode::FollowPlan => self
                    .models
                    .ems
                    .site_target_w(self.state.unix_time_s(), &self.state),
            }
        } else {
            0.0
        };
        let rated = self.cfg.grid.site_rated_w;
        let target_w = raw_target.clamp(-rated, rated);
        self.state.ems.site_setpoint_w = target_w;

        // 2. Allocate the target evenly over non-faulted blocks.
        let active_blocks = self
            .state
            .blocks
            .iter()
            .filter(|b| b.pcs.op_state != PcsOpState::Fault)
            .count();
        let share_w = if active_blocks == 0 {
            0.0
        } else {
            target_w / active_blocks as f64
        };

        // 3. Per block: BMS limits -> PCS conversion -> racks -> thermal.
        let mut p_ac_site_w = 0.0;
        let mut hvac_aux_w = 0.0;
        let mut avail = PowerLimits::default();
        for (block_idx, block) in self.state.blocks.iter_mut().enumerate() {
            let outcome = step_block(
                &self.models,
                &self.cfg,
                &mut self.rack_limits_scratch,
                block,
                share_w,
                inputs.ambient_c,
                dt_s,
            );
            p_ac_site_w += outcome.p_ac_w;
            hvac_aux_w += outcome.hvac_w;
            avail.max_discharge_w += outcome.ac_capability.max_discharge_w;
            avail.max_charge_w += outcome.ac_capability.max_charge_w;
            self.state.energy.battery_loss_wh += outcome.battery_heat_w * wh;
            self.state.energy.pcs_loss_wh += block.pcs.loss_w * wh;
            if let Some((from, to)) = outcome.pcs_transition {
                self.events.push(Event::PcsStateChanged {
                    block: block_idx,
                    from,
                    to,
                });
            }
        }
        self.state.ems.available_discharge_w = avail.max_discharge_w;
        self.state.ems.available_charge_w = avail.max_charge_w;

        // 4. Substation: losses, auxiliaries, POI measurements, meters.
        self.models.grid.step(
            &mut self.state.substation,
            p_ac_site_w,
            hvac_aux_w,
            inputs.grid_frequency_hz,
            dt_s,
        );
        self.state.energy.transformer_loss_wh += self.state.substation.transformer_loss_w * wh;
        self.state.energy.aux_wh += self.state.substation.aux_power_w * wh;

        self.state.tick += 1;
        &self.events
    }
}

/// Step one power block: aggregate BMS limits, convert the AC share to a DC
/// request, distribute it over in-service racks, advance the electrical and
/// thermal models, and finalize the PCS.
fn step_block(
    models: &Models,
    cfg: &PlantConfig,
    rack_limits: &mut [PowerLimits],
    block: &mut BlockState,
    share_w: f64,
    ambient_c: f64,
    dt_s: f64,
) -> BlockOutcome {
    // DC capability of this block, rack by rack.
    let mut block_limits = PowerLimits::default();
    let mut in_service = 0usize;
    {
        let mut i = 0;
        for container in &block.containers {
            for rack in &container.racks {
                let lim = models.bms.rack_limits(rack, &cfg.rack);
                rack_limits[i] = lim;
                block_limits.max_charge_w += lim.max_charge_w;
                block_limits.max_discharge_w += lim.max_discharge_w;
                in_service += usize::from(rack.in_service);
                i += 1;
            }
        }
    }
    let ac_capability = models.pcs.ac_capability_w(&block_limits);

    let faulted = block.pcs.op_state == PcsOpState::Fault;
    let block_target_w = if faulted { 0.0 } else { share_w };
    block.pcs.p_ac_setpoint_w = block_target_w;
    let dc_request_w = models.pcs.dc_request_w(block_target_w, &block_limits);

    // Distribute the DC request evenly over in-service racks. A rack that
    // cannot take its share leaves the remainder undelivered (no
    // redistribution pass in M0).
    let per_rack_w = if in_service == 0 {
        0.0
    } else {
        dc_request_w / in_service as f64
    };

    let mut p_dc_block_w = 0.0;
    let mut battery_heat_w = 0.0;
    let mut hvac_w = 0.0;
    let mut i = 0;
    for container in &mut block.containers {
        let mut container_heat_w = 0.0;
        for rack in &mut container.racks {
            let lim = rack_limits[i];
            i += 1;
            let request_w = if rack.in_service {
                per_rack_w.clamp(-lim.max_charge_w, lim.max_discharge_w)
            } else {
                0.0
            };
            let res = models.cell.step_rack(rack, &cfg.rack, request_w, dt_s);
            debug_assert!(
                (-1.0e-9..=1.0 + 1.0e-9).contains(&rack.soc),
                "SoC out of bounds: {}",
                rack.soc
            );
            p_dc_block_w += res.p_dc_w;
            container_heat_w += res.heat_w;
        }
        battery_heat_w += container_heat_w;
        hvac_w += models
            .thermal
            .step_container(container, container_heat_w, ambient_c, dt_s);
    }

    let op_state_before = block.pcs.op_state;
    let p_ac_w = models.pcs.finalize(&mut block.pcs, p_dc_block_w);
    let pcs_transition = if block.pcs.op_state == op_state_before {
        None
    } else {
        Some((op_state_before, block.pcs.op_state))
    };

    BlockOutcome {
        p_ac_w,
        hvac_w,
        battery_heat_w,
        ac_capability,
        pcs_transition,
    }
}
