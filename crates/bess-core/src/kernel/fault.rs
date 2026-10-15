//! Every physical fault the kernel accepts, under one name, and the one
//! door that applies any of them.
//!
//! `FaultAction` is the vocabulary the scenario engine resolves a file to;
//! `Simulation::apply` is a dispatcher over the between-ticks commands, not
//! a second implementation of them. The one action no command covers is a
//! rack's excess self-discharge, which sets a rate in the rack and leaves
//! the consequences to the BMS model's spread law.

use serde::{Deserialize, Serialize};

use crate::alarms::{AlarmNode, ResetError, ResetScope};
use crate::state::BlockMode;

use super::{CommandError, Event, Simulation};

/// Seconds in a day, for the self-discharge rate's unit.
const DAY_S: f64 = 86_400.0;

/// A physical fault, or the operator action that answers one, addressed
/// by tree indices. Integer-only, so `Event` keeps `Eq` and the event
/// digest never hashes a float's formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FaultAction {
    /// One container's HVAC unit fails.
    HvacFailure {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
    },
    /// One container's HVAC unit is repaired.
    HvacRepair {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
    },
    /// One block's PCS trips to fault.
    PcsTrip {
        /// Block index.
        block: usize,
    },
    /// Protection trips the site: breaker open, every PCS in fault.
    ProtectionTrip,
    /// One rack's string starts leaking: an excess self-discharge rate in
    /// parts per million of SoC per day (1 %/month is 333). Zero heals it,
    /// which only tests do; a scenario has no repair for a leaking cell.
    RackSelfDischarge {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
        /// Excess self-discharge, ppm of SoC per day.
        excess_ppm_per_day: u32,
    },
    /// One rack's DC contactor opens.
    RackIsolate {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
    },
    /// One rack's DC contactor closes again.
    RackRestore {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
    },
    /// A block is locked out for maintenance.
    BlockMaintenanceEnter {
        /// Block index.
        block: usize,
    },
    /// A block returns to service.
    BlockMaintenanceExit {
        /// Block index.
        block: usize,
    },
    /// The operator reset, over a scope.
    AlarmReset(ResetScope),
}

impl FaultAction {
    /// The fault's name as a scenario file spells it (`failure`,
    /// `self_discharge`, `alarm_reset`), shared by the parser and the
    /// surfaces so the two cannot drift.
    pub fn name(&self) -> &'static str {
        match self {
            Self::HvacFailure { .. } => "failure",
            Self::HvacRepair { .. } => "repair",
            Self::PcsTrip { .. } => "trip",
            Self::ProtectionTrip => "protection_trip",
            Self::RackSelfDischarge { .. } => "self_discharge",
            Self::RackIsolate { .. } => "isolate",
            Self::RackRestore { .. } => "restore",
            Self::BlockMaintenanceEnter { .. } => "maintenance_enter",
            Self::BlockMaintenanceExit { .. } => "maintenance_exit",
            Self::AlarmReset(_) => "alarm_reset",
        }
    }
}

/// Why a fault action was refused: the command it dispatches to refused
/// it, always before anything changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FaultError {
    /// Refused by a between-ticks command.
    #[error(transparent)]
    Command(#[from] CommandError),
    /// Refused by the operator reset.
    #[error(transparent)]
    Reset(#[from] ResetError),
}

impl Simulation {
    /// Apply a fault action between ticks. `Ok(true)` when the plant
    /// changed, `Ok(false)` when it was already in the requested state (a
    /// second trip of a tripped PCS, a repair of a working unit, a reset
    /// with nothing latched). A node the site does not have is refused
    /// before anything changes and leaves no mark.
    ///
    /// Every accepted action, no-op included, goes out with the next tick as
    /// `Event::FaultApplied`, ahead of the events the action itself left
    /// pending (a trip's raise, the PCS transitions): the log reads cause,
    /// then effect.
    pub fn apply(&mut self, action: FaultAction) -> Result<bool, FaultError> {
        let mark = self.state.pending_events.len();
        let changed = self.dispatch(action)?;
        self.state
            .pending_events
            .insert(mark, Event::FaultApplied(action));
        Ok(changed)
    }

    fn dispatch(&mut self, action: FaultAction) -> Result<bool, FaultError> {
        Ok(match action {
            FaultAction::HvacFailure { block, container } => {
                self.hvac_to(block, container, true)?
            }
            FaultAction::HvacRepair { block, container } => {
                self.hvac_to(block, container, false)?
            }
            FaultAction::PcsTrip { block } => self.trip_pcs(block)?,
            FaultAction::ProtectionTrip => self.trip_protection(),
            FaultAction::RackSelfDischarge {
                block,
                container,
                rack,
                excess_ppm_per_day,
            } => {
                let r = self
                    .state
                    .blocks
                    .get_mut(block)
                    .and_then(|b| b.containers.get_mut(container))
                    .and_then(|c| c.racks.get_mut(rack))
                    .ok_or(CommandError::NoSuchNode(AlarmNode::Rack {
                        block,
                        container,
                        rack,
                    }))?;
                let rate = f64::from(excess_ppm_per_day) * 1.0e-6 / DAY_S;
                let changed = r.self_discharge_excess_per_s.to_bits() != rate.to_bits();
                r.self_discharge_excess_per_s = rate;
                changed
            }
            FaultAction::RackIsolate {
                block,
                container,
                rack,
            } => self.rack_to(block, container, rack, false)?,
            FaultAction::RackRestore {
                block,
                container,
                rack,
            } => self.rack_to(block, container, rack, true)?,
            FaultAction::BlockMaintenanceEnter { block } => {
                self.mode_to(block, BlockMode::Maintenance)?
            }
            FaultAction::BlockMaintenanceExit { block } => {
                self.mode_to(block, BlockMode::InService)?
            }
            FaultAction::AlarmReset(scope) => {
                // What a reset still finds present re-raises on the next
                // tick, which the log shows; the list itself is the HMI's.
                let before = self.state.pending_events.len();
                self.reset_alarms(scope)?;
                self.state.pending_events.len() > before
            }
        })
    }

    fn hvac_to(
        &mut self,
        block: usize,
        container: usize,
        failed: bool,
    ) -> Result<bool, CommandError> {
        let was = self
            .state
            .blocks
            .get(block)
            .and_then(|b| b.containers.get(container))
            .map(|c| c.hvac.failed);
        self.set_hvac_failed(block, container, failed)?;
        Ok(was != Some(failed))
    }

    fn rack_to(
        &mut self,
        block: usize,
        container: usize,
        rack: usize,
        in_service: bool,
    ) -> Result<bool, CommandError> {
        let was = self
            .state
            .blocks
            .get(block)
            .and_then(|b| b.containers.get(container))
            .and_then(|c| c.racks.get(rack))
            .map(|r| r.in_service);
        self.set_rack_in_service(block, container, rack, in_service)?;
        Ok(was != Some(in_service))
    }

    fn mode_to(&mut self, block: usize, mode: BlockMode) -> Result<bool, CommandError> {
        let was = self.state.blocks.get(block).map(|b| b.mode);
        self.set_block_mode(block, mode)?;
        Ok(was != Some(mode))
    }
}
