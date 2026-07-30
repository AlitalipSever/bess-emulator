//! The single geometry of the scene: a unit cube. Everything visible is an
//! instance of it (ground, containers, PCS units, wires, energy dots).

/// 36-vertex unit cube centered at origin, interleaved position + normal.
pub fn cube_mesh() -> Vec<f32> {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let mut out = Vec::with_capacity(36 * 6);
    for (n, u, v) in faces {
        let c = [n[0] * 0.5, n[1] * 0.5, n[2] * 0.5];
        let quad = [
            (-0.5, -0.5),
            (0.5, -0.5),
            (0.5, 0.5),
            (-0.5, -0.5),
            (0.5, 0.5),
            (-0.5, 0.5),
        ];
        for (a, b) in quad {
            out.extend_from_slice(&[
                c[0] + u[0] * a + v[0] * b,
                c[1] + u[1] * a + v[1] * b,
                c[2] + u[2] * a + v[2] * b,
                n[0],
                n[1],
                n[2],
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn cube_has_36_vertices_with_unit_extent() {
        let m = super::cube_mesh();
        assert_eq!(m.len(), 36 * 6);
        for v in m.chunks(6) {
            for c in &v[..3] {
                assert!(c.abs() <= 0.5 + 1e-6);
            }
        }
    }
}
