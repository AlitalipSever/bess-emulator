//! What the plant is doing right now: state-of-charge gauges, status
//! indicators, running fans, energy moving along the electrical path, night
//! lighting, and the selection frame.
//!
//! Everything here is a function of the state tree and the frame clock. The
//! geometry it decorates is built once in [`super::site`].

use bess_core::state::{HvacMode, PcsOpState, SiteState};

use crate::layout::{Selection, SiteLayout, CONTAINER_SIZE, GANTRY_H, GANTRY_X, TRANSFORMER_X};
use crate::style::{Palette, Style};

use super::push;

/// Point on a polyline at normalized position `t` (0..1), by arc length.
fn lerp_poly(points: &[[f32; 3]], t: f32) -> [f32; 3] {
    let mut total = 0.0f32;
    let mut lens = [0.0f32; 8];
    for (i, w) in points.windows(2).enumerate() {
        let d = ((w[1][0] - w[0][0]).powi(2)
            + (w[1][1] - w[0][1]).powi(2)
            + (w[1][2] - w[0][2]).powi(2))
        .sqrt();
        lens[i] = d;
        total += d;
    }
    let mut target = t.clamp(0.0, 1.0) * total;
    let segs = points.len() - 1;
    for i in 0..segs {
        if target <= lens[i] || i == segs - 1 {
            let f = if lens[i] > 0.0 { target / lens[i] } else { 0.0 };
            let (a, b) = (points[i], points[i + 1]);
            return [
                a[0] + (b[0] - a[0]) * f,
                a[1] + (b[1] - a[1]) * f,
                a[2] + (b[2] - a[2]) * f,
            ];
        }
        target -= lens[i];
    }
    *points.last().expect("polyline has points")
}

/// Emissive edge frame around an axis-aligned box (selection highlight).
fn push_frame(o: &mut Vec<f32>, center: [f32; 3], size: [f32; 3], color: [f32; 3]) {
    let (hx, hy, hz) = (size[0] / 2.0, size[1] / 2.0, size[2] / 2.0);
    let t = 0.07;
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            push(
                o,
                [center[0] + sx * hx, center[1], center[2] + sz * hz],
                [t, size[1], t],
                color,
                1.0,
            );
        }
        push(
            o,
            [center[0] + sx * hx, center[1] + hy, center[2]],
            [t, t, size[2]],
            color,
            1.0,
        );
    }
    for sz in [-1.0f32, 1.0] {
        push(
            o,
            [center[0], center[1] + hy, center[2] + sz * hz],
            [size[0], t, t],
            color,
            1.0,
        );
    }
}

/// Everything the dynamic pass needs.
pub struct DynamicInput<'a> {
    /// The state tree being visualized.
    pub state: &'a SiteState,
    /// Site geometry.
    pub layout: &'a SiteLayout,
    /// Neutral materials.
    pub style: &'a Style,
    /// Signal colors.
    pub palette: &'a Palette,
    /// 0 at full day, 1 at night (drives site lighting).
    pub nightness: f32,
    /// Wall-clock animation phase, seconds. Cosmetic only: flow dots and
    /// fans animate at watchable speed regardless of simulation speed.
    pub anim_s: f32,
    /// Current selection, highlighted with an edge frame.
    pub selection: Option<Selection>,
}

/// Per-frame dynamic instances: SoC gauges, status LEDs, HVAC and PCS fans,
/// energy flow along the electrical path, night lighting, selection frame.
pub fn build_dynamic(out: &mut Vec<f32>, inp: &DynamicInput) {
    let DynamicInput {
        state,
        layout,
        style: s,
        palette,
        nightness,
        anim_s,
        selection,
    } = inp;
    let anim = *anim_s;
    let pulse = (anim * 2.4).sin() * 0.5 + 0.5;
    let blink = if (anim * 2.2).fract() < 0.5 {
        1.0f32
    } else {
        0.25
    };

    for (b, block_l) in layout.blocks.iter().enumerate() {
        let Some(block_s) = state.blocks.get(b) else {
            break;
        };
        let p_ac = block_s.pcs.p_ac_w as f32;
        let active = p_ac.abs() > 50_000.0;
        let flow_color = if p_ac < 0.0 {
            palette.charge
        } else {
            palette.discharge
        };
        let d = block_l.door_sign;

        for (c, cc) in block_l.container_centers.iter().enumerate() {
            let Some(cont_s) = block_s.containers.get(c) else {
                break;
            };
            let n = cont_s.racks.len().max(1) as f32;
            let soc = cont_s.racks.iter().map(|r| r.soc as f32).sum::<f32>() / n;
            let alarmed = cont_s.racks.iter().any(|r| r.alarm_bits != 0);
            let (x, z) = (cc[0], cc[2]);
            let gauge_z = z + d * (CONTAINER_SIZE[2] / 2.0 + 0.05);

            // SoC gauge fill, bottom-up inside the recessed slot
            let h = (soc * 2.1).max(0.02);
            let fill_color = if active { flow_color } else { s.handle };
            let fill_em = if active { 0.25 + 0.35 * pulse } else { 0.08 };
            push(
                out,
                [x + 2.6, 0.35 + h / 2.0, gauge_z],
                [0.34, h, 0.05],
                fill_color,
                fill_em,
            );

            // status LED above the gauge
            let led_color = if alarmed { palette.alarm } else { fill_color };
            let led_em = if alarmed || active { blink } else { 0.35 };
            push(
                out,
                [x + 2.6, 2.62, gauge_z],
                [0.1, 0.1, 0.04],
                led_color,
                led_em,
            );

            // HVAC fan blades orbit on the end unit whenever the unit runs,
            // faster when the second cooling stage joins.
            let fan_rate = match cont_s.hvac.mode {
                HvacMode::Off => 0.0,
                HvacMode::Heat => 6.0,
                HvacMode::Cool1 => 9.0,
                HvacMode::Cool2 => 14.0,
            };
            if fan_rate > 0.0 {
                let fan_x = x - CONTAINER_SIZE[0] / 2.0 - 0.36;
                let spin = anim * fan_rate;
                for k in 0..4 {
                    let th = spin + k as f32 * std::f32::consts::FRAC_PI_2;
                    push(
                        out,
                        [fan_x, 1.85 + 0.3 * th.cos(), z + 0.3 * th.sin()],
                        [0.04, 0.1, 0.1],
                        s.fin,
                        0.2,
                    );
                }
            }
        }

        // PCS fan on the grille face while converting
        if block_s.pcs.op_state == PcsOpState::Run {
            let [px, _, pz] = block_l.pcs_center;
            let spin = anim * 14.0;
            for k in 0..4 {
                let th = spin + k as f32 * std::f32::consts::FRAC_PI_2;
                push(
                    out,
                    [px + 0.5 * th.cos(), 1.2 + 0.5 * th.sin(), pz + d * 1.2],
                    [0.1, 0.1, 0.04],
                    s.fin,
                    0.25,
                );
            }
        }

        // energy dots: containers -> PCS -> conduit -> spine -> transformer
        if active {
            let [px, _, pz] = block_l.pcs_center;
            let spine_z = -d * 2.3;
            let path = [
                [block_l.center[0], 1.3, pz],
                [px, 1.2, pz],
                [px, 0.2, pz],
                [px, 0.2, spine_z],
                [TRANSFORMER_X - 3.0, 0.2, spine_z],
                [TRANSFORMER_X, 1.4, 0.0],
            ];
            let dir = if p_ac >= 0.0 { 1.0f32 } else { -1.0 };
            let strength = (p_ac.abs() / 5.0e6).min(1.0);
            for k in 0..10 {
                let t = (anim * 0.22 * dir + k as f32 / 10.0).rem_euclid(1.0);
                let p = lerp_poly(&path, t);
                push(out, p, [0.14, 0.14, 0.14], flow_color, 0.4 + 0.5 * strength);
            }
        }
    }

    // site export/import dots along the HV take-off
    let poi_w = state.substation.poi_active_power_w as f32;
    if poi_w.abs() > 1.0e6 {
        let path = [
            [TRANSFORMER_X + 1.0, 3.8, 0.0],
            [72.5, 4.9, 0.0],
            [GANTRY_X, 6.6, 0.0],
            [GANTRY_X, GANTRY_H - 0.2, 0.0],
            [GANTRY_X + 12.0, GANTRY_H - 0.2, 0.0],
        ];
        let (dir, color) = if poi_w >= 0.0 {
            (1.0f32, palette.discharge)
        } else {
            (-1.0, palette.charge)
        };
        let strength = (poi_w.abs() / 1.0e8).min(1.0);
        for k in 0..14 {
            let t = (anim * 0.3 * dir + k as f32 / 14.0).rem_euclid(1.0);
            let p = lerp_poly(&path, t);
            push(out, p, [0.16, 0.16, 0.16], color, 0.5 + 0.5 * strength);
        }
    }

    // light mast heads glow after sundown
    for m in &layout.masts {
        let arm = -m[2].signum();
        push(
            out,
            [m[0], 8.82, m[2] + arm * 1.1],
            [0.5, 0.14, 0.5],
            s.handle,
            0.1 + nightness * 1.3,
        );
    }

    // selection frame
    if let Some(sel) = selection {
        match *sel {
            Selection::Container { block, container } => {
                if let Some(cc) = layout
                    .blocks
                    .get(block)
                    .and_then(|bl| bl.container_centers.get(container))
                {
                    push_frame(
                        out,
                        *cc,
                        [
                            CONTAINER_SIZE[0] + 0.25,
                            CONTAINER_SIZE[1] + 0.25,
                            CONTAINER_SIZE[2] + 0.25,
                        ],
                        palette.select,
                    );
                }
            }
            Selection::Pcs { block } => {
                if let Some(bl) = layout.blocks.get(block) {
                    push_frame(out, bl.pcs_center, [3.3, 2.6, 2.5], palette.select);
                }
            }
            Selection::Transformer => {
                push_frame(
                    out,
                    layout.transformer_center,
                    [3.9, 3.4, 3.1],
                    palette.select,
                );
            }
        }
    }
}
