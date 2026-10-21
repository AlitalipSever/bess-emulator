//! One power block through one tick: BMS, PCS, racks, thermal, and the
//! rack alarm words that read the result.

use crate::alarms::{AlarmNode, BlockSiteThresholds};
use crate::config::PlantConfig;
use crate::state::{BlockState, ContainerState};
use crate::traits::{Models, PowerLimits};

use super::alarms::{evaluate_block, push_changes};
use super::setpoint::SetpointDynamics;
use super::{Event, Weather};

/// What every block of one tick shares.
pub(super) struct TickContext<'a> {
    pub(super) models: &'a Models,
    pub(super) cfg: &'a PlantConfig,
    pub(super) thresholds: &'a BlockSiteThresholds,
    pub(super) dynamics: &'a SetpointDynamics,
    pub(super) share_w: f64,
    /// Each dispatchable block's share of the plant's frequency response,
    /// W (M4 phase 1): the fast term the follower does not ramp.
    pub(super) fast_share_w: f64,
    pub(super) weather: Weather,
    pub(super) dt_s: f64,
}

/// What one block contributed during a tick.
pub(super) struct BlockOutcome {
    /// AC power produced (positive) or consumed (negative), W.
    pub(super) p_ac_w: f64,
    /// HVAC electrical draw of the block's containers, W.
    pub(super) hvac_w: f64,
    /// Battery heat released (cell losses and balancing bleed), W.
    pub(super) battery_heat_w: f64,
    /// AC-side capability given current BMS limits; zero for a block
    /// outside the dispatchable set, whatever its racks could do.
    pub(super) ac_capability: PowerLimits,
}

/// Per-tick working buffers, owned by the simulation and reused across
/// blocks so the hot loop performs no allocation.
pub(super) struct Scratch {
    /// DC limits of the racks in the block being stepped.
    pub(super) rack_limits: Vec<PowerLimits>,
    /// Balancing heat of the racks in the block being stepped, W.
    pub(super) bms_heat: Vec<f64>,
    /// Heat released by the racks of the container being stepped, in rack
    /// order.
    pub(super) rack_heat: Vec<f64>,
}

/// Step one power block: advance the BMS, aggregate its limits, convert the
/// AC share to a DC request, distribute it over in-service racks, advance
/// the electrical and thermal models, evaluate the rack words, finalize the
/// PCS, and evaluate the block word. Events go out in that order: racks,
/// the PCS transition, the block.
///
/// A block outside the dispatchable set (`in_set` false: maintained,
/// faulted, not yet released, or the site disconnected) is asked for
/// nothing and reports no capability, but its BMS, racks and containers
/// step as always.
pub(super) fn step_block(
    ctx: &TickContext,
    scratch: &mut Scratch,
    block: &mut BlockState,
    block_idx: usize,
    in_set: bool,
    events: &mut Vec<Event>,
) -> BlockOutcome {
    let TickContext {
        models,
        cfg,
        thresholds,
        dynamics,
        share_w,
        fast_share_w,
        weather,
        dt_s,
    } = *ctx;
    let Scratch {
        rack_limits,
        bms_heat,
        rack_heat,
    } = scratch;

    // The BMS's own dynamics first (spread, balancing, the estimator),
    // then the DC capability of this block, rack by rack.
    let (block_limits, in_service) = step_bms(
        models,
        cfg,
        block,
        block_idx,
        dt_s,
        rack_limits,
        bms_heat,
        events,
    );
    // The DC bus the converter sees: the racks' terminal voltage as the
    // last tick left it, which carries SoC, current and hysteresis.
    block.pcs.dc_voltage_v = dc_bus_voltage_v(block, cfg);
    // The converter's own heat and budget, from the last tick's loss and
    // power, decide what it permits this tick. It sits outdoors between
    // the containers, so the air it cools toward is the ambient.
    models
        .pcs
        .step_thermal(&mut block.pcs, weather.ambient_c, dt_s);
    let ac_capability = if in_set {
        models.pcs.ac_capability_w(&block.pcs, &block_limits)
    } else {
        PowerLimits::default()
    };

    // The share is what the controller asks; the converter follows it
    // through its dead time, ramp and lag, and converts what it has
    // reached.
    block.pcs.p_ac_setpoint_w = if in_set { share_w } else { 0.0 };
    dynamics.follow(&mut block.pcs, in_set, cfg.pcs_rated_w, dt_s, fast_share_w);
    let dc_request_w = models.pcs.dc_request_w(&block.pcs, &block_limits);

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
    for (container_idx, container) in block.containers.iter_mut().enumerate() {
        let racks_here = container.racks.len();
        debug_assert!(rack_heat.len() >= racks_here, "rack heat scratch too small");
        let heat_slots = &mut rack_heat[..racks_here];
        for (slot, rack) in heat_slots.iter_mut().zip(&mut container.racks) {
            let lim = rack_limits[i];
            let bleed_w = bms_heat[i];
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
            // Bleed resistors dissipate inside the rack, next to the cells:
            // the same thermal mass, and the same loss row.
            *slot = res.heat_w + bleed_w;
            battery_heat_w += res.heat_w + bleed_w;
        }
        hvac_w += models
            .thermal
            .step_container(container, &rack_heat[..racks_here], weather, dt_s)
            .hvac_electrical_w;

        // Rack words last, so they read this tick's temperatures.
        evaluate_racks(models, cfg, container, block_idx, container_idx, events);
    }

    let op_state_before = block.pcs.op_state;
    let p_ac_w = models.pcs.finalize(&mut block.pcs, p_dc_block_w);
    if block.pcs.op_state != op_state_before {
        events.push(Event::PcsStateChanged {
            block: block_idx,
            from: op_state_before,
            to: block.pcs.op_state,
        });
    }

    let (word, miss_s) = evaluate_block(block, cfg, thresholds, dt_s);
    push_changes(
        events,
        AlarmNode::Block { block: block_idx },
        block.alarm_bits,
        word,
    );
    block.alarm_bits = word;
    block.setpoint_miss_s = miss_s;

    BlockOutcome {
        p_ac_w,
        hvac_w,
        battery_heat_w,
        ac_capability,
    }
}

/// Mean terminal voltage of the block's in-service racks, or the rack
/// nominal when none is in service (a de-energized bus has no better
/// number and the converter converts nothing then).
fn dc_bus_voltage_v(block: &BlockState, cfg: &PlantConfig) -> f64 {
    let mut sum = 0.0;
    let mut n = 0u32;
    for rack in block.containers.iter().flat_map(|c| c.racks.iter()) {
        if rack.in_service {
            sum += rack.voltage_v;
            n += 1;
        }
    }
    if n == 0 {
        cfg.rack.nominal_v()
    } else {
        sum / f64::from(n)
    }
}

/// Step every rack's BMS and collect the block's DC limits. Returns the
/// summed limits and the count of in-service racks; the per-rack limits
/// and balancing heat land in the scratch slices, in rack order.
#[allow(clippy::too_many_arguments)]
fn step_bms(
    models: &Models,
    cfg: &PlantConfig,
    block: &mut BlockState,
    block_idx: usize,
    dt_s: f64,
    rack_limits: &mut [PowerLimits],
    bms_heat: &mut [f64],
    events: &mut Vec<Event>,
) -> (PowerLimits, usize) {
    let mut block_limits = PowerLimits::default();
    let mut in_service = 0usize;
    let mut i = 0;
    for (container_idx, container) in block.containers.iter_mut().enumerate() {
        for (rack_idx, rack) in container.racks.iter_mut().enumerate() {
            let flows = models.bms.step_bms(rack, &cfg.rack, dt_s);
            bms_heat[i] = flows.heat_w;
            if let Some(jump) = flows.soc_resnap {
                events.push(Event::SocResnapped {
                    block: block_idx,
                    container: container_idx,
                    rack: rack_idx,
                    jump,
                });
            }
            let lim = models.bms.rack_limits(rack, &cfg.rack);
            rack_limits[i] = lim;
            block_limits.max_charge_w += lim.max_charge_w;
            block_limits.max_discharge_w += lim.max_discharge_w;
            in_service += usize::from(rack.in_service);
            i += 1;
        }
    }
    (block_limits, in_service)
}

/// Evaluate and store the rack words of one container, pushing what changed.
fn evaluate_racks(
    models: &Models,
    cfg: &PlantConfig,
    container: &mut ContainerState,
    block_idx: usize,
    container_idx: usize,
    events: &mut Vec<Event>,
) {
    for (rack_idx, rack) in container.racks.iter_mut().enumerate() {
        let word = models.bms.rack_alarms(rack, &cfg.rack);
        let node = AlarmNode::Rack {
            block: block_idx,
            container: container_idx,
            rack: rack_idx,
        };
        push_changes(events, node, rack.alarm_bits, word);
        rack.alarm_bits = word;
    }
}
