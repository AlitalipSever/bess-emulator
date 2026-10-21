//! The staggered return: which blocks the plant controller has cleared to
//! run, how it clears them one by one after a protection trip, and the one
//! predicate that decides both allocation and reported availability.
//!
//! The sequencing is the plant controller's, not any layer model's: no
//! trait owns the start order of a site. Relay internals, precharge and
//! synchronization stay out (M3); what is modeled is the observable order.

use crate::state::{BlockMode, BlockState, EmsState};

/// How the plant controller brings blocks back after a trip.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnPolicy {
    /// Seconds between one block's release and the next.
    pub spacing_s: f64,
}

impl Default for ReturnPolicy {
    /// 30 s per block. One GW-01 block is 5 % of the site, so a block every
    /// 30 s averages the 10 % of rated power per minute that the German
    /// connection codes allow a plant reconnecting after a protective
    /// disconnection. An estimate anchored to that gradient, not a cited
    /// per-block figure: no public source pins one (CALIBRATION.md).
    fn default() -> Self {
        Self { spacing_s: 30.0 }
    }
}

/// Whether block `index` is cleared to run: the site is connected, the
/// block in service and already released. What the converter does with
/// the clearance is the startup sequence's business (`startup.rs`).
pub(super) fn cleared(block: &BlockState, index: usize, ems: &EmsState, connected: bool) -> bool {
    connected && block.mode == BlockMode::InService && index < ems.blocks_released
}

/// Whether block `index` takes a share of the site target and counts
/// towards availability: cleared, and its converter connected (through
/// its startup sequence, not in fault). Allocation, availability and the
/// site's partial-availability bit all ask this one question.
pub(super) fn dispatchable(
    block: &BlockState,
    index: usize,
    ems: &EmsState,
    connected: bool,
) -> bool {
    cleared(block, index, ems, connected) && block.pcs.op_state.is_connected()
}

/// Start the return over: no block released, the first one due on the
/// next tick of a connected site. A protection trip and the site reset
/// that ends it both begin here.
pub(super) fn start_over(ems: &mut EmsState) {
    ems.blocks_released = 0;
    ems.next_release_s = 0.0;
}

/// Each dispatchable block's even share of the site target, W; zero when
/// no block is dispatchable.
pub(super) fn share_w(
    blocks: &[BlockState],
    ems: &EmsState,
    connected: bool,
    target_w: f64,
) -> f64 {
    let active = blocks
        .iter()
        .enumerate()
        .filter(|(k, b)| dispatchable(b, *k, ems, connected))
        .count();
    if active == 0 {
        0.0
    } else {
        target_w / active as f64
    }
}

/// Advance the return by one tick of a connected site: release the next
/// block when its time has come, then count down to the one after. A site
/// that is not returning (everything released) is left alone.
///
/// Released positions, not power: a released block that is maintained or
/// faulted stays outside the dispatchable set until that clears.
pub(super) fn release(ems: &mut EmsState, blocks: usize, policy: &ReturnPolicy, dt_s: f64) {
    if ems.blocks_released >= blocks {
        return;
    }
    if ems.next_release_s <= 0.0 {
        ems.blocks_released += 1;
        ems.next_release_s = policy.spacing_s;
    }
    if ems.blocks_released < blocks {
        ems.next_release_s -= dt_s;
    } else {
        ems.next_release_s = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::EmsMode;

    fn returning() -> EmsState {
        EmsState {
            mode: EmsMode::FollowPlan,
            site_setpoint_w: 0.0,
            external_setpoint_w: 0.0,
            available_discharge_w: 0.0,
            available_charge_w: 0.0,
            blocks_released: 0,
            next_release_s: 0.0,
            fcr_prequalified_w: 0.0,
            fcr_output_w: 0.0,
            lfsm_cut_w: 0.0,
            reactive_mode: crate::state::ReactiveMode::unity(),
            q_site_target_var: 0.0,
            reactive_cos_phi: 1.0,
            reactive_under_excited: false,
            reactive_q_var: 0.0,
            external_limit: crate::kernel::ExternalLimit::none(),
            limit_binding: false,
            awards: crate::kernel::Awards::NONE,
            awards_pinned: false,
            reservoir: crate::kernel::Reservoir::default(),
        }
    }

    /// The tick on which each block is released, counting the first tick
    /// after the reset as 0.
    fn release_ticks(blocks: usize, spacing_s: f64) -> Vec<u64> {
        let policy = ReturnPolicy { spacing_s };
        let mut ems = returning();
        let mut ticks = Vec::new();
        for tick in 0..10_000u64 {
            let before = ems.blocks_released;
            release(&mut ems, blocks, &policy, 1.0);
            if ems.blocks_released > before {
                ticks.push(tick);
            }
        }
        ticks
    }

    #[test]
    fn block_k_is_released_k_spacings_after_the_first() {
        let ticks = release_ticks(20, 30.0);
        let want: Vec<u64> = (0..20).map(|k| k * 30).collect();
        assert_eq!(ticks, want);
        assert_eq!(ticks.last(), Some(&570));
    }

    #[test]
    fn a_site_that_is_not_returning_is_left_alone() {
        let policy = ReturnPolicy::default();
        let mut ems = returning();
        ems.blocks_released = 20;
        release(&mut ems, 20, &policy, 1.0);
        assert_eq!(ems.blocks_released, 20);
        assert!(ems.next_release_s.abs() < f64::EPSILON);
    }

    #[test]
    fn the_default_averages_ten_percent_of_gw01_per_minute() {
        let spacing_s = ReturnPolicy::default().spacing_s;
        let block_share = 1.0 / 20.0;
        let per_minute = block_share * 60.0 / spacing_s;
        assert!((per_minute - 0.10).abs() < 1.0e-12);
    }
}
