//! Cube-instance builders: the state tree in, flat `f32` instance data out.
//! Pure functions; the renderer only uploads what these emit.
//!
//! Instance layout (see `FPI`): offset(3) scale(3) color(3) emissive(1).

use bess_core::state::{PcsOpState, SiteState};

use crate::layout::{
    BlockLayout, Selection, SiteLayout, CONTAINER_SIZE, GANTRY_H, GANTRY_X, TRANSFORMER_X,
};
use crate::style::{Palette, Style};

/// Floats per instance.
pub const FPI: usize = 10;

/// Append one cuboid instance.
pub fn push(out: &mut Vec<f32>, offset: [f32; 3], scale: [f32; 3], color: [f32; 3], em: f32) {
    out.extend_from_slice(&offset);
    out.extend_from_slice(&scale);
    out.extend_from_slice(&color);
    out.push(em);
}

/// Gravel apron, road, and concrete pads. Drawn without a shadow pass, so
/// it lives in its own instance buffer.
pub fn build_ground(l: &SiteLayout, s: &Style) -> Vec<f32> {
    let mut g = Vec::new();
    let (gc, gs) = l.gravel;
    push(
        &mut g,
        [gc[0], -0.45, gc[2]],
        [gs[0], 0.9, gs[2]],
        s.gravel,
        0.0,
    );
    let (rc, rs) = l.road;
    push(
        &mut g,
        [rc[0], -0.44, rc[2]],
        [rs[0], 0.9, rs[2]],
        s.aisle,
        0.0,
    );
    for block in &l.blocks {
        push(
            &mut g,
            [block.center[0] + 1.2, -0.43, block.center[2]],
            [11.4, 0.9, 8.6],
            s.pad,
            0.0,
        );
    }
    // substation pad
    push(
        &mut g,
        [TRANSFORMER_X + 5.0, -0.43, 0.0],
        [26.0, 0.9, 13.0],
        s.pad,
        0.0,
    );
    g
}

/// Container shell, doors, wall HVAC unit and the recessed gauge slot.
fn build_container(o: &mut Vec<f32>, center: [f32; 3], door: f32, s: &Style) {
    let (x, z) = (center[0], center[2]);
    let (len, height, width) = (CONTAINER_SIZE[0], CONTAINER_SIZE[1], CONTAINER_SIZE[2]);
    // plinth and body
    push(
        o,
        [x, 0.09, z],
        [len + 0.15, 0.18, width + 0.12],
        s.steel_dark,
        0.0,
    );
    push(
        o,
        [x, 0.18 + (height - 0.28) / 2.0, z],
        [len, height - 0.28, width],
        s.steel,
        0.0,
    );
    // roof cap
    push(
        o,
        [x, height - 0.04, z],
        [len + 0.12, 0.10, width + 0.12],
        s.roof,
        0.0,
    );
    // vertical ribs on the door face
    for i in 0..5 {
        let rx = x - 2.4 + i as f32 * 1.2;
        push(
            o,
            [rx, 1.45, z + door * (width / 2.0 + 0.02)],
            [0.06, 2.3, 0.04],
            s.fin,
            0.0,
        );
    }
    // two door frames
    for dx in [-1.55f32, 1.55] {
        push(
            o,
            [x + dx, 1.32, z + door * (width / 2.0 + 0.03)],
            [1.35, 2.25, 0.035],
            s.steel_dark,
            0.0,
        );
    }
    // wall-mounted HVAC unit on the -x end, with louvres
    push(
        o,
        [x - len / 2.0 - 0.16, 1.45, z],
        [0.32, 2.2, 1.6],
        s.steel,
        0.0,
    );
    for i in 0..4 {
        push(
            o,
            [x - len / 2.0 - 0.33, 0.8 + i as f32 * 0.45, z],
            [0.05, 0.06, 1.3],
            s.fin,
            0.0,
        );
    }
    // recessed SoC gauge slot near the +x end of the door face
    push(
        o,
        [x + 2.6, 1.45, z + door * (width / 2.0 + 0.01)],
        [0.5, 2.3, 0.06],
        s.gauge_bg,
        0.0,
    );
}

/// PCS skid: base, body, roof and the grille facing the road.
fn build_pcs(o: &mut Vec<f32>, block: &BlockLayout, s: &Style) {
    let [x, _, z] = block.pcs_center;
    let d = block.door_sign;
    push(o, [x, 0.08, z], [3.2, 0.16, 2.4], s.steel_dark, 0.0);
    push(o, [x, 1.15, z], [3.0, 2.0, 2.2], s.steel, 0.0);
    push(o, [x, 2.24, z], [3.1, 0.09, 2.3], s.roof, 0.0);
    push(
        o,
        [x, 1.15, z + d * 1.12],
        [2.6, 1.6, 0.05],
        s.steel_dark,
        0.0,
    );
    for i in 0..4 {
        push(
            o,
            [x, 0.55 + i as f32 * 0.4, z + d * 1.16],
            [2.4, 0.05, 0.03],
            s.fin,
            0.0,
        );
    }
    // MV conduit from the skid toward the road-side cable spine
    let spine_z = -d * 2.3; // spine sits on the block's side of the road
    let z0 = z + d * 1.2;
    let z1 = -d * 2.3;
    push(
        o,
        [x, 0.07, (z0 + z1) / 2.0],
        [0.3, 0.14, (z1 - z0).abs()],
        s.steel_dark,
        0.0,
    );
    let _ = spine_z;
}

/// Substation: transformer, breaker bay, gantry, outgoing lines.
fn build_substation(o: &mut Vec<f32>, s: &Style) {
    let tx = TRANSFORMER_X;
    // transformer plinth, tank, roof
    push(o, [tx, 0.06, 0.0], [3.8, 0.12, 3.0], s.pad, 0.0);
    push(o, [tx, 1.42, 0.0], [3.4, 2.6, 2.4], s.steel_dark, 0.0);
    push(o, [tx, 2.78, 0.0], [3.5, 0.12, 2.5], s.roof, 0.0);
    // radiator bank on the -z side
    for i in 0..8 {
        push(
            o,
            [tx - 1.4 + i as f32 * 0.4, 1.35, -1.35],
            [0.08, 2.0, 0.5],
            s.fin,
            0.0,
        );
    }
    // three HV bushings, leaning toward the gantry side
    for zb in [-0.8f32, 0.0, 0.8] {
        push(o, [tx + 1.0, 3.2, zb], [0.14, 0.9, 0.14], s.fin, 0.0);
        push(o, [tx + 1.0, 3.7, zb], [0.2, 0.1, 0.2], s.steel_dark, 0.0);
    }
    // breaker bay: three post pairs with a top bar
    for zb in [-0.8f32, 0.0, 0.8] {
        push(o, [72.0, 1.2, zb], [0.12, 2.4, 0.12], s.steel_dark, 0.0);
        push(o, [72.0, 2.5, zb], [0.5, 0.14, 0.14], s.fin, 0.0);
    }
    // gantry: poles, crossarm, insulators
    for pz in [-3.0f32, 3.0] {
        push(
            o,
            [GANTRY_X, GANTRY_H / 2.0, pz],
            [0.25, GANTRY_H, 0.25],
            s.steel_dark,
            0.0,
        );
    }
    push(
        o,
        [GANTRY_X, GANTRY_H - 0.15, 0.0],
        [0.3, 0.3, 7.0],
        s.steel_dark,
        0.0,
    );
    for zb in [-0.8f32, 0.0, 0.8] {
        push(
            o,
            [GANTRY_X, GANTRY_H - 0.55, zb],
            [0.12, 0.5, 0.12],
            s.fin,
            0.0,
        );
    }
    // conductors: bushings -> gantry in two stepped segments, then the
    // outgoing spans toward the horizon
    for zb in [-0.8f32, 0.0, 0.8] {
        push(
            o,
            [(tx + 1.0 + 72.5) / 2.0, 4.9, zb],
            [72.5 - tx - 1.0, 0.05, 0.05],
            s.steel_dark,
            0.0,
        );
        push(
            o,
            [(72.5 + GANTRY_X) / 2.0, 6.6, zb],
            [GANTRY_X - 72.5, 0.05, 0.05],
            s.steel_dark,
            0.0,
        );
        push(
            o,
            [GANTRY_X + 6.0, GANTRY_H - 0.2, zb * 1.8],
            [12.0, 0.05, 0.05],
            s.steel_dark,
            0.0,
        );
    }
}

/// Perimeter fence and light masts.
fn build_perimeter(o: &mut Vec<f32>, l: &SiteLayout, s: &Style) {
    let (x0, z0, x1, z1) = l.fence;
    let mut post = |x: f32, z: f32| push(o, [x, 1.1, z], [0.1, 2.2, 0.1], s.steel_dark, 0.0);
    let mut x = x0;
    while x <= x1 {
        post(x, z0);
        post(x, z1);
        x += 8.0;
    }
    let mut z = z0;
    while z <= z1 {
        post(x0, z);
        post(x1, z);
        z += 8.0;
    }
    for rail_y in [1.0f32, 2.0] {
        push(
            o,
            [(x0 + x1) / 2.0, rail_y, z0],
            [x1 - x0, 0.05, 0.05],
            s.fin,
            0.0,
        );
        push(
            o,
            [(x0 + x1) / 2.0, rail_y, z1],
            [x1 - x0, 0.05, 0.05],
            s.fin,
            0.0,
        );
        push(o, [x0, rail_y, 0.0], [0.05, 0.05, z1 - z0], s.fin, 0.0);
        push(o, [x1, rail_y, 0.0], [0.05, 0.05, z1 - z0], s.fin, 0.0);
    }
    for m in &l.masts {
        push(o, [m[0], 4.5, m[2]], [0.16, 9.0, 0.16], s.steel_dark, 0.0);
        let arm = -m[2].signum();
        push(
            o,
            [m[0], 8.9, m[2] + arm * 0.6],
            [0.12, 0.1, 1.2],
            s.steel_dark,
            0.0,
        );
    }
    // road-side cable spines on both sides
    let last_x = l
        .blocks
        .iter()
        .map(|b| b.pcs_center[0])
        .fold(f32::MIN, f32::max);
    let first_x = l
        .blocks
        .iter()
        .map(|b| b.center[0])
        .fold(f32::MAX, f32::min);
    for sz in [-2.3f32, 2.3] {
        let x_end = TRANSFORMER_X - 3.0;
        push(
            o,
            [(first_x + x_end) / 2.0, 0.1, sz],
            [x_end - first_x, 0.12, 0.3],
            s.steel_dark,
            0.0,
        );
    }
    let _ = last_x;
}

/// All static site furniture (everything that never changes per frame).
pub fn build_static(l: &SiteLayout, s: &Style) -> Vec<f32> {
    let mut o = Vec::new();
    for block in &l.blocks {
        for cc in &block.container_centers {
            build_container(&mut o, *cc, block.door_sign, s);
        }
        build_pcs(&mut o, block, s);
    }
    build_substation(&mut o, s);
    build_perimeter(&mut o, l, s);
    o
}

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

            // HVAC fan blades orbit on the end unit while cooling runs
            if cont_s.hvac.cooling_on {
                let fan_x = x - CONTAINER_SIZE[0] / 2.0 - 0.36;
                let spin = anim * 9.0;
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

#[cfg(test)]
mod tests {
    use bess_core::config::PlantConfig;
    use bess_core::state::SiteState;

    use super::{build_dynamic, build_ground, build_static, DynamicInput, FPI};
    use crate::layout::{Selection, SiteLayout};
    use crate::style::{Palette, Style};

    #[test]
    fn instance_streams_are_wellformed() {
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let style = Style::default();
        let ground = build_ground(&layout, &style);
        let stat = build_static(&layout, &style);
        assert_eq!(ground.len() % FPI, 0);
        assert_eq!(stat.len() % FPI, 0);
        assert!(stat.len() / FPI > 500, "site should be furnished");

        let state = SiteState::new(&cfg, 1, 1_767_225_600);
        let mut dynamic = Vec::new();
        build_dynamic(
            &mut dynamic,
            &DynamicInput {
                state: &state,
                layout: &layout,
                style: &style,
                palette: &Palette::default(),
                nightness: 1.0,
                anim_s: 3.2,
                selection: Some(Selection::Container {
                    block: 3,
                    container: 1,
                }),
            },
        );
        assert_eq!(dynamic.len() % FPI, 0);
        // 40 gauges + 40 LEDs + masts + selection frame at minimum.
        assert!(dynamic.len() / FPI > 90);
        for chunk in dynamic.chunks(FPI) {
            assert!(chunk.iter().all(|v| v.is_finite()));
        }
    }
}
