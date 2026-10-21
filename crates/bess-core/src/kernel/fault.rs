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

use super::{Awards, CommandError, Event, Product, Simulation};

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
    /// The operator opens the HV breaker (M3 phase 4).
    HvBreakerOpen,
    /// The operator closes the HV breaker, through its sequence.
    HvBreakerClose,
    /// The operator moves the line disconnectors.
    Disconnectors {
        /// Closed (`true`) or open.
        closed: bool,
    },
    /// The operator moves the earthing switch.
    EarthingSwitch {
        /// Closed (`true`) or open.
        closed: bool,
    },
    /// A grid event: the frequency the plant sees is the driver's plus
    /// this offset until the next such event (M4 phase 1). Zero ends it.
    GridFrequencyOffset {
        /// Millihertz added to the driver's frequency.
        mhz: i32,
    },
    /// A grid operator's order: cap the exchange at the POI (M4 phase 2
    /// PR1), in tenths of a percent of the rating, reverting after
    /// `revert_s` seconds when that is positive.
    ExternalLimit {
        /// The cap, tenths of a percent of the rating (0 to 1000).
        pct_x10: u16,
        /// Seconds until the cap clears itself; 0 keeps it.
        revert_s: u32,
    },
    /// The order lifted.
    ExternalLimitClear,
    /// A balancing award pinned by hand (M4 phase 3): the plant holds
    /// `kw` of `product` from the next tick until the awards are cleared,
    /// the EMS strategy's awards set aside meanwhile.
    Award {
        /// The product.
        product: Product,
        /// Awarded power, kW.
        kw: u32,
    },
    /// The pinned awards cleared; the strategy's are back.
    AwardsClear,
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
            Self::HvBreakerOpen => "breaker_open",
            Self::HvBreakerClose => "breaker_close",
            Self::Disconnectors { closed: true } => "disconnectors_close",
            Self::Disconnectors { closed: false } => "disconnectors_open",
            Self::EarthingSwitch { closed: true } => "earthing_close",
            Self::EarthingSwitch { closed: false } => "earthing_open",
            Self::GridFrequencyOffset { .. } => "frequency_offset",
            Self::ExternalLimit { .. } => "limit",
            Self::ExternalLimitClear => "limit_clear",
            Self::Award { .. } => "award",
            Self::AwardsClear => "award_clear",
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
            FaultAction::HvBreakerOpen => self.open_hv_breaker(),
            FaultAction::HvBreakerClose => self.close_hv_breaker()?,
            FaultAction::Disconnectors { closed } => self.set_disconnectors(closed)?,
            FaultAction::EarthingSwitch { closed } => self.set_earthing_switch(closed)?,
            FaultAction::GridFrequencyOffset { mhz } => {
                let changed = self.state.substation.frequency_offset_mhz != mhz;
                self.set_grid_frequency_offset_mhz(mhz);
                changed
            }
            FaultAction::ExternalLimit { pct_x10, revert_s } => {
                let before = self.state.ems.external_limit;
                self.set_external_limit(f64::from(pct_x10) / 10.0, f64::from(revert_s))?;
                before != self.state.ems.external_limit
            }
            FaultAction::ExternalLimitClear => self.clear_external_limit(),
            FaultAction::Award { product, kw } => {
                let mut awards = if self.state.ems.awards_pinned {
                    self.state.ems.awards
                } else {
                    Awards::NONE
                };
                let before = awards;
                awards.set_power_w(product, f64::from(kw) * 1000.0);
                self.pin_awards(awards);
                before != awards
            }
            FaultAction::AwardsClear => {
                let was = self.state.ems.awards_pinned;
                self.unpin_awards();
                was
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
