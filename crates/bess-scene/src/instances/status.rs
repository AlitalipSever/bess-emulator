//! Alarm indications in the scene: severity shown where the cause lives.
//!
//! The container diode reads the worst of its racks' words, a diode on the
//! PCS skid reads the block word, a beacon on the transformer reads the
//! site word, and a failed HVAC unit carries a mark of its own. Colours are
//! the alarm tones of [`crate::alarms::Tone`]; nothing blinks, because the
//! operator standards keep flashing for an unacknowledged alarm and this
//! plant has no acknowledgement. Power keeps its pulse on the SoC gauge.

use bess_core::alarms::AlarmNode;
use bess_core::state::{BlockState, ContainerState, SiteState};

use crate::alarms::Tone;
use crate::layout::{BlockLayout, CONTAINER_SIZE, TRANSFORMER_X};
use crate::style::{Palette, Style};

use super::push;

/// Glow of an indication that is showing an alarm.
const ALARM_EM: f32 = 0.95;
/// Glow of an indication with nothing to report.
const QUIET_EM: f32 = 0.3;

/// The loudest tone among a container's racks.
pub fn container_tone(block: usize, container: usize, cont: &ContainerState) -> Option<Tone> {
    cont.racks
        .iter()
        .enumerate()
        .filter_map(|(rack, r)| {
            let node = AlarmNode::Rack {
                block,
                container,
                rack,
            };
            Tone::worst(node, r.alarm_bits)
        })
        .max()
}

/// The loudest tone of a block word.
pub fn block_tone(block: usize, state: &BlockState) -> Option<Tone> {
    Tone::worst(AlarmNode::Block { block }, state.alarm_bits)
}

/// The loudest tone of the site word.
pub fn site_tone(state: &SiteState) -> Option<Tone> {
    Tone::worst(AlarmNode::Site, state.alarm_bits)
}

/// Colour and glow of an indication: its tone's, or `quiet` dimly.
pub fn lamp(tone: Option<Tone>, quiet: [f32; 3], palette: &Palette) -> ([f32; 3], f32) {
    tone.map_or((quiet, QUIET_EM), |t| (t.color(palette), ALARM_EM))
}

/// The HVAC failure mark and the PCS skid diode of one block.
pub fn push_block(
    out: &mut Vec<f32>,
    b: usize,
    block_l: &BlockLayout,
    block_s: &BlockState,
    style: &Style,
    palette: &Palette,
) {
    let d = block_l.door_sign;
    for (cc, cont_s) in block_l.container_centers.iter().zip(&block_s.containers) {
        if cont_s.hvac.failed {
            // A strip across the top of the end unit's louvres: the fan is
            // already still, and this says why.
            let x = cc[0] - CONTAINER_SIZE[0] / 2.0 - 0.34;
            push(
                out,
                [x, 2.35, cc[2]],
                [0.04, 0.2, 1.2],
                palette.warning,
                ALARM_EM,
            );
        }
    }
    let [px, _, pz] = block_l.pcs_center;
    let (color, em) = lamp(block_tone(b, block_s), style.handle, palette);
    push(
        out,
        [px + 1.05, 2.07, pz + d * 1.13],
        [0.16, 0.16, 0.05],
        color,
        em,
    );
}

/// The beacon on the transformer roof.
pub fn push_site(out: &mut Vec<f32>, state: &SiteState, style: &Style, palette: &Palette) {
    let (color, em) = lamp(site_tone(state), style.handle, palette);
    push(
        out,
        [TRANSFORMER_X - 1.2, 3.0, 0.0],
        [0.3, 0.3, 0.3],
        color,
        em,
    );
}
