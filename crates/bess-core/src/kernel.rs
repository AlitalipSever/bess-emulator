//! The tick loop: orchestrates the causal chain across plant layers.
//!
//! Order within a tick mirrors the real control chain: EMS picks a site
//! target, the plant controller allocates it to blocks, each PCS converts,
//! racks deliver what their BMS limits allow, heat flows into the container
//! thermal model, and the substation propagates the result to the POI.

use serde::{Deserialize, Serialize};

use crate::alarms::{AlarmNode, BlockSiteThresholds, Severity};
use crate::config::PlantConfig;
use crate::state::{AuxPower, BreakerState, EmsMode, PcsOpState, SiteState};
use crate::traits::{AuxDemand, Models, PowerLimits};
use crate::TICK_SECONDS;

mod alarms;
mod block;

use block::{step_block, Scratch, TickContext};

/// The weather one tick applies.
///
/// A separate struct so a model can be handed exactly the exogenous
/// quantities it has business knowing: the thermal layer sees the sky, not
/// the grid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    /// Ambient air temperature, degrees Celsius.
    pub ambient_c: f64,
    /// Global horizontal irradiance, W/m2.
    pub irradiance_wm2: f64,
}

/// Exogenous inputs for one tick. Compiled offline (bess-data) or generated
/// by a driver in the shell; the kernel treats them as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Inputs {
    /// Ambient conditions at the site.
    pub weather: Weather,
    /// Grid frequency at the POI, Hz.
    pub grid_frequency_hz: f64,
}

/// Discrete events emitted by the kernel: state transitions and alarm
/// edges. Facts about a tick, emitted exactly once; the shells fan them out.
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
    /// An alarm bit went from clear to set.
    AlarmRaised {
        /// Which word.
        node: AlarmNode,
        /// Bit position, per `alarms::layout`.
        bit: u8,
        /// Read off the bit position.
        severity: Severity,
    },
    /// An alarm bit went from set to clear.
    AlarmCleared {
        /// Which word.
        node: AlarmNode,
        /// Bit position, per `alarms::layout`.
        bit: u8,
        /// Read off the bit position.
        severity: Severity,
    },
}

/// A running simulation: configuration, model bundle, and the state tree.
pub struct Simulation {
    cfg: PlantConfig,
    models: Models,
    state: SiteState,
    events: Vec<Event>,
    /// Events that happened between ticks (an operator reset), already
    /// recorded in the log and handed out with the next tick's.
    pending: Vec<Event>,
    thresholds: BlockSiteThresholds,
    scratch: Scratch,
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
        let racks_per_container = cfg.racks_per_container;
        Self {
            cfg,
            models,
            state,
            events: Vec::with_capacity(16),
            pending: Vec::new(),
            thresholds: BlockSiteThresholds::default(),
            scratch: Scratch {
                rack_limits: vec![PowerLimits::default(); racks_per_block],
                bms_heat: vec![0.0; racks_per_block],
                rack_heat: vec![0.0; racks_per_container],
            },
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

    /// Fail (`true`) or repair (`false`) one container's HVAC unit, from
    /// the next tick on. The first physical fault the kernel accepts; the
    /// phase 4 scenario player reaches it through its fault actions.
    pub fn set_hvac_failed(&mut self, block: usize, container: usize, failed: bool) {
        self.state.blocks[block].containers[container].hvac.failed = failed;
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
        // A reset between ticks already recorded its clears; they go out
        // first, ahead of anything this tick raises again.
        self.events.append(&mut self.pending);
        let recorded = self.events.len();

        self.state.weather = inputs.weather;

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
        let ctx = TickContext {
            models: &self.models,
            cfg: &self.cfg,
            thresholds: &self.thresholds,
            share_w,
            weather: inputs.weather,
            dt_s,
        };
        for (block_idx, block) in self.state.blocks.iter_mut().enumerate() {
            let outcome = step_block(&ctx, &mut self.scratch, block, block_idx, &mut self.events);
            p_ac_site_w += outcome.p_ac_w;
            hvac_aux_w += outcome.hvac_w;
            avail.max_discharge_w += outcome.ac_capability.max_discharge_w;
            avail.max_charge_w += outcome.ac_capability.max_charge_w;
            self.state.energy.battery_loss_wh += outcome.battery_heat_w * wh;
            self.state.energy.pcs_loss_wh += block.pcs.loss_w * wh;
        }
        self.state.ems.available_discharge_w = avail.max_discharge_w;
        self.state.ems.available_charge_w = avail.max_charge_w;

        // 4. Auxiliary inventory: what the site consumes to run itself. A
        //    dark site draws nothing at the POI, which is what the substation
        //    meters too, so the itemized and metered totals agree by
        //    construction. What losing supply does to a container that still
        //    has heat in it is an M3 question, arriving with the breaker
        //    state machine; M0 never opens the breaker.
        let aux = if connected {
            self.models.aux.step_site(AuxDemand {
                hvac_w: hvac_aux_w,
                racks: self.cfg.total_racks(),
                // Standby and Fault both mean energized and not converting;
                // see AuxDemand for why a tripped unit still pays.
                pcs_not_converting: self
                    .state
                    .blocks
                    .iter()
                    .filter(|b| b.pcs.op_state != PcsOpState::Run)
                    .count(),
            })
        } else {
            AuxPower::default()
        };
        self.state.aux = aux;
        self.state.energy.aux_items.accumulate(&aux, wh);

        // 5. Substation: losses, POI measurements, meters.
        self.models.grid.step(
            &mut self.state.substation,
            p_ac_site_w,
            aux.total_w(),
            inputs.grid_frequency_hz,
            dt_s,
        );
        self.state.energy.transformer_loss_wh += self.state.substation.transformer_loss_w * wh;
        self.state.energy.aux_wh += self.state.substation.aux_power_w * wh;

        // The waterfall identity, checked by every debug run rather than by
        // one test file: what the substation metered as house load equals
        // what the items say they drew. The two totals are accumulated on
        // different paths, so a category added to one and forgotten in the
        // other shows up here on the tick it happens, not in a report months
        // later. Relative because the two sum in a different order.
        debug_assert!(
            {
                let metered = self.state.energy.aux_wh;
                let itemized = self.state.energy.aux_items.total_wh();
                (metered - itemized).abs() <= 1.0e-9 * metered.abs().max(1.0)
            },
            "auxiliary accounting drifted: metered {} Wh, itemized {} Wh",
            self.state.energy.aux_wh,
            self.state.energy.aux_items.total_wh()
        );

        // 6. Site word, from the plant as this tick left it, then the log.
        self.close_tick_alarms(recorded);

        self.state.tick += 1;
        &self.events
    }
}
