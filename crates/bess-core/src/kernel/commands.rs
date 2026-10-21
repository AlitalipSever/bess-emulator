//! The between-ticks door: physical changes an operator, a test or the
//! phase 4 scenario player makes to the plant.
//!
//! Every command applies between ticks. Its consequences emerge from the
//! next tick's models; what it says right away (a PCS transition, a
//! latched raise) waits in `SiteState::pending_events`, so a checkpoint
//! taken in the gap loses nothing. A command naming a node the site does
//! not have is refused before anything changes. The operator reset, which
//! belongs with the alarm vocabulary, lives in `kernel/alarms.rs`.

use crate::alarms::layout::site as s;
use crate::alarms::{has_bit, with_bit, AlarmNode, Severity};
use crate::state::{BlockMode, BreakerState, PcsOpState};

use super::{restart, Event, Simulation};

/// Why a command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// The command names a block or rack the site does not have.
    #[error("no such node on this site: {0:?}")]
    NoSuchNode(AlarmNode),
    /// The command names a container the site does not have. Containers
    /// carry no alarm word, so they get a variant of their own.
    #[error("no such container on this site: block {block}, container {container}")]
    NoSuchContainer {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
    },
    /// The substation's interlocks refuse the switching operation; the
    /// message says which one (M3 phase 4).
    #[error("interlocked: {0}")]
    Interlocked(&'static str),
}

impl Simulation {
    /// Fail (`true`) or repair (`false`) one container's HVAC unit, from
    /// the next tick on. The first physical fault the kernel accepts; the
    /// phase 4 scenario player reaches it through its fault actions.
    /// `block.hvac_failure` mirrors it on the next tick.
    pub fn set_hvac_failed(
        &mut self,
        block: usize,
        container: usize,
        failed: bool,
    ) -> Result<(), CommandError> {
        let c = self
            .state
            .blocks
            .get_mut(block)
            .and_then(|b| b.containers.get_mut(container))
            .ok_or(CommandError::NoSuchContainer { block, container })?;
        c.hvac.failed = failed;
        Ok(())
    }

    /// Close (`true`) or open (`false`) one rack's DC contactor, from the
    /// next tick on. An isolated rack is asked for nothing and reports no
    /// limits; its electronics and its container's cooling keep running.
    /// `rack.isolated` and `site.partial_availability` say so on the next
    /// tick.
    pub fn set_rack_in_service(
        &mut self,
        block: usize,
        container: usize,
        rack: usize,
        in_service: bool,
    ) -> Result<(), CommandError> {
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
        r.in_service = in_service;
        Ok(())
    }

    /// Put a block into maintenance or back into service, from the next
    /// tick on. Entering maintenance opens the converter's contactors: its
    /// PCS goes to `Off` (unless it is in fault, which only a reset
    /// clears) and stops paying its standby tare; its containers keep
    /// their HVAC and its racks keep balancing. Leaving maintenance hands
    /// the block back to the plant controller, whose clearance starts the
    /// converter's startup sequence. `block.maintenance` mirrors the mode
    /// on the next tick.
    pub fn set_block_mode(&mut self, block: usize, mode: BlockMode) -> Result<(), CommandError> {
        let b = self
            .state
            .blocks
            .get_mut(block)
            .ok_or(CommandError::NoSuchNode(AlarmNode::Block { block }))?;
        b.mode = mode;
        let from = b.pcs.op_state;
        if mode == BlockMode::Maintenance && from != PcsOpState::Fault && from != PcsOpState::Off {
            b.pcs.op_state = PcsOpState::Off;
            b.pcs.state_elapsed_s = 0.0;
            self.state.pending_events.push(Event::PcsStateChanged {
                block,
                from,
                to: PcsOpState::Off,
            });
        }
        Ok(())
    }

    /// Trip one block's PCS to `Fault`. The transition waits for the next
    /// tick, whose block evaluation raises `block.pcs_fault`; the other
    /// blocks pick up its share. Only an operator reset brings it back.
    /// Returns `false` when the PCS was already in fault.
    pub fn trip_pcs(&mut self, block: usize) -> Result<bool, CommandError> {
        let pcs = &mut self
            .state
            .blocks
            .get_mut(block)
            .ok_or(CommandError::NoSuchNode(AlarmNode::Block { block }))?
            .pcs;
        if pcs.op_state == PcsOpState::Fault {
            return Ok(false);
        }
        let from = pcs.op_state;
        pcs.op_state = PcsOpState::Fault;
        self.state.pending_events.push(Event::PcsStateChanged {
            block,
            from,
            to: PcsOpState::Fault,
        });
        Ok(true)
    }

    /// Protection trips the site: the HV breaker opens, every PCS drops to
    /// `Fault`, `site.protection_trip` latches, and the return sequencer
    /// starts over from no block released. The raise goes out first on the
    /// next tick, then the PCS transitions; that tick's site evaluation adds
    /// `site.hv_breaker_open`, which mirrors the breaker. The house load
    /// carries on through station service.
    ///
    /// Returns `false`, changing nothing, on a site that is already tripped.
    /// The way back is `reset_alarms(ResetScope::Site)`.
    pub fn trip_protection(&mut self) -> bool {
        if has_bit(self.state.alarm_bits, s::PROTECTION_TRIP) {
            return false;
        }
        let state = &mut self.state;
        state.alarm_bits = with_bit(state.alarm_bits, s::PROTECTION_TRIP, true);
        state.pending_events.push(Event::AlarmRaised {
            node: AlarmNode::Site,
            bit: s::PROTECTION_TRIP,
            severity: Severity::of_bit(s::PROTECTION_TRIP),
        });
        state.substation.hv_breaker = BreakerState::Open;
        restart::start_over(&mut state.ems);
        for (block, b) in state.blocks.iter_mut().enumerate() {
            let from = b.pcs.op_state;
            if from != PcsOpState::Fault {
                b.pcs.op_state = PcsOpState::Fault;
                state.pending_events.push(Event::PcsStateChanged {
                    block,
                    from,
                    to: PcsOpState::Fault,
                });
            }
        }
        true
    }
}
