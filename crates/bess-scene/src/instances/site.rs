//! What the site is made of: ground, containers, converter skids, the
//! substation and the perimeter. Geometry that is a function of the layout
//! and nothing else, built once and reused every frame.
//!
//! Split from the per-frame builders in [`super::live`] because the two
//! answer different questions. This file answers what the plant looks like;
//! that one answers what it is doing right now.

use crate::layout::{BlockLayout, SiteLayout, CONTAINER_SIZE, GANTRY_H, GANTRY_X, TRANSFORMER_X};
use crate::style::Style;

use super::push;

/// Terrain, gravel apron, road, and concrete pads. Drawn without a shadow
/// pass, so it lives in its own instance buffer.
pub fn build_ground(l: &SiteLayout, s: &Style) -> Vec<f32> {
    let mut g = Vec::new();
    // Surrounding terrain out to the horizon; distance fog blends it into
    // the sky so the site does not float in empty color.
    push(
        &mut g,
        [0.0, -0.55, 0.0],
        [2600.0, 1.0, 2600.0],
        s.terrain,
        0.0,
    );
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
    // dashed center line on the road
    let dashes = (rs[0] / 8.0) as i32;
    for i in 0..dashes {
        let x = rc[0] - rs[0] / 2.0 + 4.0 + i as f32 * 8.0;
        push(&mut g, [x, 0.02, rc[2]], [2.4, 0.02, 0.16], s.marking, 0.0);
    }
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
