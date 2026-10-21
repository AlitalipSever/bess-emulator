//! The tick loop: orchestrates the causal chain across plant layers.
//!
//! Order within a tick mirrors the real control chain: EMS picks a site
//! target, the plant controller allocates it to blocks, each PCS converts,
//! racks deliver what their BMS limits allow, heat flows into the container
//! thermal model, and the substation propagates the result to the POI.

use serde::{Deserialize, Serialize};

use crate::alarms::BlockSiteThresholds;
use crate::config::PlantConfig;
use crate::state::{
    AuxPower, BlockMode, BreakerState, EmsMode, PcsOpState, ReactiveMode, SiteState,
};
use crate::traits::{AuxDemand, GridConditions, Models, PowerLimits};
use crate::TICK_SECONDS;

mod alarms;
mod block;
mod commands;
mod event;
mod fault;
mod gridcode;
mod limit;
mod reactive;
mod restart;
mod setpoint;
mod startup;
mod switchgear;

use block::{step_block, Scratch, TickContext};
pub use commands::CommandError;
pub use event::Event;
pub use fault::{FaultAction, FaultError};
pub use gridcode::{FcrDroop, GridCode, GridCodeState};
pub use limit::{ControlCommand, ExternalLimit};
pub use reactive::ReactiveControl;
use restart::dispatchable;
pub use restart::ReturnPolicy;
pub use setpoint::SetpointDynamics;
pub use startup::StartupSequence;
pub use switchgear::{BreakerSequence, Switch, SwitchgearTimes};

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
    /// Grid voltage at the POI, per unit of nominal (M3 phase 4; synthetic
    /// until M4 replays a series).
    pub grid_voltage_pu: f64,
}

/// A running simulation: configuration, model bundle, and the state tree.
pub struct Simulation {
    cfg: PlantConfig,
    models: Models,
    state: SiteState,
    events: Vec<Event>,
    thresholds: BlockSiteThresholds,
    return_policy: ReturnPolicy,
    startup: StartupSequence,
    dynamics: SetpointDynamics,
    switchgear: SwitchgearTimes,
    gridcode: GridCode,
    droop: FcrDroop,
    reactive: ReactiveControl,
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
            thresholds: BlockSiteThresholds::default(),
            return_policy: ReturnPolicy::default(),
            startup: StartupSequence::default(),
            dynamics: SetpointDynamics::default(),
            switchgear: SwitchgearTimes::default(),
            gridcode: GridCode::default(),
            droop: FcrDroop::default(),
            reactive: ReactiveControl::default(),
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

    /// Replace how the converters follow their setpoints. For research
    /// runs and the switch-off proof; the plant ships with the default.
    pub fn set_setpoint_dynamics(&mut self, dynamics: SetpointDynamics) {
        self.dynamics = dynamics;
    }

    /// Replace the connection code's frequency characteristics and the
    /// FCR droop's rules (M4 phase 1). Research runs and the switch-off
    /// proof.
    pub fn set_grid_code(&mut self, gridcode: GridCode, droop: FcrDroop) {
        self.gridcode = gridcode;
        self.droop = droop;
    }

    /// Arm the FCR droop block with a prequalified power, W, or disarm it
    /// with zero. Phase 3's products call this per block; until then it
    /// is the research switch and the tests' door.
    pub fn set_fcr_prequalified_w(&mut self, pq_w: f64) {
        self.state.ems.fcr_prequalified_w = pq_w.max(0.0);
    }

    /// A scenario's grid event: add `mhz` to the driver's frequency from
    /// the next tick on, until another call changes it. Zero clears it.
    pub fn set_grid_frequency_offset_mhz(&mut self, mhz: i32) {
        self.state.substation.frequency_offset_mhz = mhz;
    }

    /// Replace the reactive-power rules (settling, the Q(U) band and
    /// slope). Research runs.
    pub fn set_reactive_control(&mut self, control: ReactiveControl) {
        self.reactive = control;
    }

    /// Step 3b of the tick: the site's reactive target in the operator's
    /// mode, settled, shared evenly over the dispatchable blocks and
    /// bounded by each converter's circle beside the active power it just
    /// converted. Returns the converters' total, var; zero on a
    /// disconnected site.
    fn step_reactive(
        &mut self,
        p_ac_site_w: f64,
        voltage_pu: f64,
        connected: bool,
        dt_s: f64,
    ) -> f64 {
        let raw = if connected {
            self.reactive.target_var(
                self.state.ems.reactive_mode,
                p_ac_site_w,
                voltage_pu,
                self.cfg.grid.site_rated_w,
            )
        } else {
            0.0
        };
        let target = self.reactive.settle(&mut self.state.ems, raw, dt_s);
        let share = restart::share_w(&self.state.blocks, &self.state.ems, connected, target);
        let mut total = 0.0;
        for (block_idx, block) in self.state.blocks.iter_mut().enumerate() {
            block.pcs.q_var = if dispatchable(block, block_idx, &self.state.ems, connected) {
                self.models.pcs.reactive(&block.pcs, share)
            } else {
                0.0
            };
            total += block.pcs.q_var;
        }
        total
    }

    /// Chemical energy currently stored across all racks, Wh.
    pub fn stored_energy_wh(&self) -> f64 {
        self.state
            .racks()
            .map(|r| self.models.cell.stored_energy_wh(r, &self.cfg.rack))
            .sum()
    }

    /// Walk every cleared converter one tick along its startup sequence
    /// before the blocks are stepped: the clearance is the return's, the
    /// walk is the converter's, and a block is dispatchable only once the
    /// walk has reached a connected state.
    fn start_converters(&mut self, connected: bool, dt_s: f64) {
        for (block_idx, block) in self.state.blocks.iter_mut().enumerate() {
            let cleared = restart::cleared(block, block_idx, &self.state.ems, connected);
            self.startup
                .advance(&mut block.pcs, block_idx, cleared, dt_s, &mut self.events);
        }
    }

    /// The site target this tick, clamped to the site rating: the external
    /// setpoint or the plan's, and nothing while the breaker is open.
    fn site_target_w(&self, connected: bool) -> f64 {
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
        raw_target.clamp(-rated, rated)
    }

    /// The external limit's cap on the site target (M4 phase 2 PR1):
    /// the target held inside the cap in both directions, the binding
    /// noted for the site word, and the revert timer counted down.
    fn apply_external_limit(&mut self, target_w: f64, dt_s: f64) -> f64 {
        let cap_w = self
            .state
            .ems
            .external_limit
            .cap_w(self.cfg.grid.site_rated_w);
        let capped = target_w.clamp(-cap_w, cap_w);
        self.state.ems.limit_binding = target_w.abs() > cap_w;
        self.state.ems.external_limit.tick(dt_s);
        capped
    }

    /// Choose how the plant sets its reactive power (M4 phase 1 PR2):
    /// the operator's mode, in force from the next tick. The settings
    /// the mode carries are kept for the holding bank to read back.
    pub fn set_reactive_mode(&mut self, mode: ReactiveMode) {
        let ems = &mut self.state.ems;
        match mode {
            ReactiveMode::FixedCosPhi {
                cos_phi,
                under_excited,
            } => {
                ems.reactive_cos_phi = cos_phi.clamp(0.0, 1.0);
                ems.reactive_under_excited = under_excited;
            }
            ReactiveMode::FixedQ { var } => ems.reactive_q_var = var,
            ReactiveMode::QofU => {}
        }
        ems.reactive_mode = mode;
    }

    /// The frequency the plant sees this tick (the driver's plus a
    /// scenario's offset) and what the grid asks on top of the EMS target
    /// at it: the LFSM cut plus the FCR droop's output, W, the fast term
    /// the converters do not ramp. Nothing on a disconnected site.
    fn grid_response(&mut self, inputs: &Inputs, connected: bool, dt_s: f64) -> (f64, f64) {
        let frequency_hz = inputs.grid_frequency_hz
            + f64::from(self.state.substation.frequency_offset_mhz) / 1000.0;
        if !connected {
            self.state.ems.lfsm_cut_w = 0.0;
            self.state.ems.fcr_output_w = 0.0;
            return (frequency_hz, 0.0);
        }
        let rated = self.cfg.grid.site_rated_w;
        self.state.ems.lfsm_cut_w = self.gridcode.lfsm_w(frequency_hz, rated);
        let fcr_w = self.droop.advance(&mut self.state.ems, frequency_hz, dt_s);
        (frequency_hz, self.state.ems.lfsm_cut_w + fcr_w)
    }

    /// Step 4 of the tick: the auxiliary inventory, what the site consumes
    /// to run itself, breaker open or closed. A tripped site keeps its
    /// house load on the station-service feed, and the substation meters
    /// it there.
    fn step_aux(&mut self, hvac_aux_w: f64, wh: f64) -> AuxPower {
        let aux = self.models.aux.step_site(AuxDemand {
            hvac_w: hvac_aux_w,
            racks: self.cfg.total_racks(),
            // Standby and Fault both mean energized and not converting; see
            // AuxDemand for why a tripped unit still pays. A maintained unit
            // is locked out and switched off, so it does not.
            pcs_not_converting: self
                .state
                .blocks
                .iter()
                .filter(|b| b.pcs.op_state != PcsOpState::Run && b.mode == BlockMode::InService)
                .count(),
        });
        self.state.aux = aux;
        self.state.energy.aux_items.accumulate(&aux, wh);
        aux
    }

    /// Advance the plant by one tick. Returns the events emitted.
    pub fn step(&mut self, inputs: &Inputs) -> &[Event] {
        let dt_s = TICK_SECONDS as f64;
        let wh = dt_s / 3600.0;
        self.events.clear();
        // A reset between ticks left its clears in the tree; they go out
        // first, ahead of anything this tick raises again.
        self.events.append(&mut self.state.pending_events);

        self.state.weather = inputs.weather;

        // 0. The switchgear: a breaker on its closing sequence closes when
        //    its time is up, and the site is connected from this tick on.
        self.switchgear.advance(
            &mut self.state.substation,
            &mut self.state.ems,
            dt_s,
            &mut self.events,
        );

        // 1. EMS: site active-power target, then what the grid asks on
        //    top of it: the LFSM cut and the FCR droop, a fast term the
        //    converters do not ramp. The frequency is the driver's plus a
        //    scenario's offset; outside the connection band the plant
        //    trips at the end of the tick.
        let connected = self.state.substation.hv_breaker == BreakerState::Closed;
        let target_w = self.site_target_w(connected);
        let target_w = self.apply_external_limit(target_w, dt_s);
        self.state.ems.site_setpoint_w = target_w;
        let (frequency_hz, fast_site_w) = self.grid_response(inputs, connected, dt_s);

        // 2. Allocate the target evenly over the dispatchable blocks: in
        //    service, not in fault, released by the return sequencer. The
        //    same predicate decides availability below, so what the plant
        //    is asked and what it reports can never disagree.
        if connected {
            restart::release(
                &mut self.state.ems,
                self.state.blocks.len(),
                &self.return_policy,
                dt_s,
            );
        }
        // The converters' startup sequences walk before the share is cut,
        // so a block that reaches a connected state this tick takes its
        // share this tick.
        self.start_converters(connected, dt_s);
        let share_w = restart::share_w(&self.state.blocks, &self.state.ems, connected, target_w);
        let fast_share_w =
            restart::share_w(&self.state.blocks, &self.state.ems, connected, fast_site_w);

        // 3. Per block: BMS limits -> PCS conversion -> racks -> thermal.
        let mut p_ac_site_w = 0.0;
        let mut hvac_aux_w = 0.0;
        let mut avail = PowerLimits::default();
        let ctx = TickContext {
            models: &self.models,
            cfg: &self.cfg,
            thresholds: &self.thresholds,
            dynamics: &self.dynamics,
            share_w,
            fast_share_w,
            weather: inputs.weather,
            dt_s,
        };
        for (block_idx, block) in self.state.blocks.iter_mut().enumerate() {
            let in_set = dispatchable(block, block_idx, &self.state.ems, connected);
            let outcome = step_block(
                &ctx,
                &mut self.scratch,
                block,
                block_idx,
                in_set,
                &mut self.events,
            );
            p_ac_site_w += outcome.p_ac_w;
            hvac_aux_w += outcome.hvac_w;
            avail.max_discharge_w += outcome.ac_capability.max_discharge_w;
            avail.max_charge_w += outcome.ac_capability.max_charge_w;
            self.state.energy.battery_loss_wh += outcome.battery_heat_w * wh;
            self.state.energy.pcs_loss_wh += block.pcs.loss_w * wh;
        }
        self.state.ems.available_discharge_w = avail.max_discharge_w;
        self.state.ems.available_charge_w = avail.max_charge_w;
        let q_ac_site_var =
            self.step_reactive(p_ac_site_w, inputs.grid_voltage_pu, connected, dt_s);

        let aux = self.step_aux(hvac_aux_w, wh);

        // 5. Substation: losses, POI measurements, meters.
        self.models.grid.step(
            &mut self.state.substation,
            p_ac_site_w,
            q_ac_site_var,
            aux.total_w(),
            GridConditions {
                frequency_hz,
                voltage_pu: inputs.grid_voltage_pu,
                ambient_c: inputs.weather.ambient_c,
            },
            dt_s,
        );
        // The fiscal meter reads the same POI power the SCADA meters just
        // booked, and closes a period when this tick ends on a quarter-hour.
        // An open breaker leaves the POI at zero, so it stands still through
        // a trip without a special case.
        let tick_end_s = self.state.unix_time_s() + TICK_SECONDS as i64;
        let poi_w = self.state.substation.poi_active_power_w;
        self.state
            .substation
            .revenue
            .observe(poi_w, dt_s, tick_end_s);
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
        self.close_tick_alarms();

        self.state.tick += 1;
        &self.events
    }
}
