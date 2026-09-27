//! The kernel's half of the alarm tree: block and site evaluation, turning
//! word changes into events, and the operator reset.
//!
//! Rack words are evaluated by `BmsLogic`, which owns their thresholds; the
//! kernel only stores them and reports what changed. Block and site words
//! read PCS, container and plant state the kernel already holds.

use crate::alarms::layout::{block as b, site as s};
use crate::alarms::{
    has_bit, rising, with_bit, AlarmNode, BlockSiteThresholds, ResetError, ResetScope, Severity,
    TRIP_MASK,
};
use crate::config::PlantConfig;
use crate::state::{BlockState, BreakerState, PcsOpState, SiteState};

use super::{Event, Simulation};

/// Push one raise or clear event per bit that differs between two words,
/// lowest bit first.
pub(super) fn push_changes(events: &mut Vec<Event>, node: AlarmNode, old: u32, new: u32) {
    let changed = old ^ new;
    if changed == 0 {
        return;
    }
    for bit in 0..16u8 {
        if has_bit(changed, bit) {
            let severity = Severity::of_bit(bit);
            events.push(if has_bit(new, bit) {
                Event::AlarmRaised {
                    node,
                    bit,
                    severity,
                }
            } else {
                Event::AlarmCleared {
                    node,
                    bit,
                    severity,
                }
            });
        }
    }
}

/// Next block word and setpoint-miss timer, from the block as this tick
/// left it.
pub(super) fn evaluate_block(
    block: &BlockState,
    cfg: &PlantConfig,
    th: &BlockSiteThresholds,
    dt_s: f64,
) -> (u32, f64) {
    let word = block.alarm_bits;

    let miss = (block.pcs.p_ac_setpoint_w - block.pcs.p_ac_w).abs() / cfg.pcs_rated_w;
    let miss_s = if miss >= th.setpoint_miss.raise {
        block.setpoint_miss_s + dt_s
    } else {
        0.0
    };
    let not_met = if has_bit(word, b::SETPOINT_NOT_MET) {
        miss > th.setpoint_miss.clear
    } else {
        miss_s >= th.setpoint_deadband_s
    };

    let air_max_c = block
        .containers
        .iter()
        .map(|c| c.air_temp_c)
        .fold(f64::NEG_INFINITY, f64::max);
    let hot = rising(
        has_bit(word, b::CONTAINER_OVER_TEMP),
        air_max_c,
        th.container_air_c,
    );
    let hvac_failed = block.containers.iter().any(|c| c.hvac.failed);
    let pcs_fault = block.pcs.op_state == PcsOpState::Fault;

    let mut next = word;
    next = with_bit(next, b::SETPOINT_NOT_MET, not_met);
    next = with_bit(next, b::CONTAINER_OVER_TEMP, hot);
    next = with_bit(next, b::HVAC_FAILURE, hvac_failed);
    next = with_bit(next, b::PCS_FAULT, pcs_fault);
    (next, miss_s)
}

/// Next site word, from the site as this tick left it.
fn evaluate_site(state: &SiteState, cfg: &PlantConfig, th: &BlockSiteThresholds) -> u32 {
    let word = state.alarm_bits;
    // Delivered, not capability: `available_*` sums every rack's limit, but
    // the even split has no redistribution pass, so a block can claim its
    // rating while its hot racks leave its share undelivered. What the
    // blocks actually converted is what the plant did.
    let setpoint_w = state.ems.site_setpoint_w;
    let delivered_w: f64 = state.blocks.iter().map(|blk| blk.pcs.p_ac_w).sum();
    let shortfall = if setpoint_w >= 0.0 {
        setpoint_w - delivered_w
    } else {
        delivered_w - setpoint_w
    }
    .max(0.0)
        / cfg.grid.site_rated_w;
    let limited = rising(
        has_bit(word, s::POWER_LIMITED),
        shortfall,
        th.site_shortfall,
    );
    let partial = state
        .blocks
        .iter()
        .any(|blk| blk.pcs.op_state == PcsOpState::Fault)
        || state.racks().any(|r| !r.in_service);
    let breaker_open = state.substation.hv_breaker == BreakerState::Open;

    let mut next = word;
    next = with_bit(next, s::POWER_LIMITED, limited);
    next = with_bit(next, s::PARTIAL_AVAILABILITY, partial);
    next = with_bit(next, s::HV_BREAKER_OPEN, breaker_open);
    next
}

impl Simulation {
    /// End of tick: evaluate the site word, then fold every event this tick
    /// hands out, reset clears included, into the log. An event counts when
    /// it is handed out, so the counter a poller reads never runs ahead of
    /// what any surface could have seen.
    pub(super) fn close_tick_alarms(&mut self) {
        let word = evaluate_site(&self.state, &self.cfg, &self.thresholds);
        push_changes(
            &mut self.events,
            AlarmNode::Site,
            self.state.alarm_bits,
            word,
        );
        self.state.alarm_bits = word;
        for event in &self.events {
            self.state.event_log.record(event);
        }
    }

    /// Operator reset, the HMI button: clear the latched bits in `scope` and
    /// take any PCS in scope out of fault. The clears and the PCS leaving
    /// fault are events; they wait in the tree (`SiteState::pending_events`,
    /// so a checkpoint taken now keeps them) and go out with the next tick.
    ///
    /// Returns the bits whose cause is still present. They re-raise on the
    /// next tick, as a real plant does to an impatient operator. A scope
    /// naming a block, container or rack the site does not have is refused
    /// before anything changes.
    pub fn reset_alarms(&mut self, scope: ResetScope) -> Result<Vec<(AlarmNode, u8)>, ResetError> {
        self.check_scope(scope)?;
        let mut events = Vec::new();
        let mut still = Vec::new();
        let (blocks, site_too): (Vec<usize>, bool) = match scope {
            ResetScope::Site => ((0..self.state.blocks.len()).collect(), true),
            ResetScope::Block(i) => (vec![i], false),
            ResetScope::Rack { .. } => (Vec::new(), false),
        };

        for &bi in &blocks {
            let block = &mut self.state.blocks[bi];
            if block.pcs.op_state == PcsOpState::Fault {
                block.pcs.op_state = PcsOpState::Standby;
                events.push(Event::PcsStateChanged {
                    block: bi,
                    from: PcsOpState::Fault,
                    to: PcsOpState::Standby,
                });
            }
            let old = block.alarm_bits;
            block.alarm_bits &= !TRIP_MASK;
            push_changes(
                &mut events,
                AlarmNode::Block { block: bi },
                old,
                block.alarm_bits,
            );
            let shape: Vec<usize> = block.containers.iter().map(|c| c.racks.len()).collect();
            for (ci, racks) in shape.into_iter().enumerate() {
                for ri in 0..racks {
                    self.reset_rack(bi, ci, ri, &mut events, &mut still);
                }
            }
        }
        if let ResetScope::Rack {
            block,
            container,
            rack,
        } = scope
        {
            self.reset_rack(block, container, rack, &mut events, &mut still);
        }
        if site_too {
            let old = self.state.alarm_bits;
            self.state.alarm_bits &= !TRIP_MASK;
            push_changes(&mut events, AlarmNode::Site, old, self.state.alarm_bits);
        }

        self.state.pending_events.extend(events);
        Ok(still)
    }

    fn check_scope(&self, scope: ResetScope) -> Result<(), ResetError> {
        let blocks = &self.state.blocks;
        let ok = match scope {
            ResetScope::Site => true,
            ResetScope::Block(b) => b < blocks.len(),
            ResetScope::Rack {
                block,
                container,
                rack,
            } => blocks
                .get(block)
                .and_then(|b| b.containers.get(container))
                .is_some_and(|c| rack < c.racks.len()),
        };
        if ok {
            Ok(())
        } else {
            Err(ResetError::NoSuchNode(scope))
        }
    }

    fn reset_rack(
        &mut self,
        block: usize,
        container: usize,
        rack: usize,
        events: &mut Vec<Event>,
        still: &mut Vec<(AlarmNode, u8)>,
    ) {
        let node = AlarmNode::Rack {
            block,
            container,
            rack,
        };
        let r = &mut self.state.blocks[block].containers[container].racks[rack];
        let old = r.alarm_bits;
        r.alarm_bits &= !TRIP_MASK;
        push_changes(events, node, old, r.alarm_bits);
        let again = self.models.bms.rack_alarms(r, &self.cfg.rack) & old & TRIP_MASK;
        still.extend(
            (8..16u8)
                .filter(|&bit| has_bit(again, bit))
                .map(|bit| (node, bit)),
        );
    }
}
