//! Column-major mat4 and vec3 helpers for the camera. Pure functions,
//! ported from the original WebGL2 scene.

/// Column-major 4x4 matrix.
pub type Mat4 = [f32; 16];
/// 3-component vector.
pub type Vec3 = [f32; 3];

/// `a * b`.
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// Right-handed perspective projection (OpenGL clip space).
pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy / 2.0).tan();
    let nf = 1.0 / (near - far);
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) * nf;
    m[11] = -1.0;
    m[14] = 2.0 * far * near * nf;
    m
}

/// Right-handed look-at view matrix.
pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = normalize(sub(center, eye));
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

/// `a - b`.
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a + b`.
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `v * k`.
pub fn scale(v: Vec3, k: f32) -> Vec3 {
    [v[0] * k, v[1] * k, v[2] * k]
}

/// Dot product.
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Cross product.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Unit vector (guarded against zero length).
pub fn normalize(v: Vec3) -> Vec3 {
    let l = dot(v, v).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// A ray in world space.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// Origin.
    pub origin: Vec3,
    /// Unit direction.
    pub dir: Vec3,
}

/// Axis-aligned box, used for picking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl Aabb {
    /// Box from center and full size.
    pub fn from_center_size(center: Vec3, size: Vec3) -> Self {
        let h = scale(size, 0.5);
        Self {
            min: sub(center, h),
            max: add(center, h),
        }
    }

    /// Slab test; returns the entry distance along the ray if it hits.
    pub fn hit(&self, ray: &Ray) -> Option<f32> {
        let mut t_near = f32::NEG_INFINITY;
        let mut t_far = f32::INFINITY;
        for i in 0..3 {
            if ray.dir[i].abs() < 1e-9 {
                if ray.origin[i] < self.min[i] || ray.origin[i] > self.max[i] {
                    return None;
                }
                continue;
            }
            let inv = 1.0 / ray.dir[i];
            let (t0, t1) = {
                let a = (self.min[i] - ray.origin[i]) * inv;
                let b = (self.max[i] - ray.origin[i]) * inv;
                if a < b {
                    (a, b)
                } else {
                    (b, a)
                }
            };
            t_near = t_near.max(t0);
            t_far = t_far.min(t1);
            if t_near > t_far {
                return None;
            }
        }
        if t_far < 0.0 {
            None
        } else {
            Some(t_near.max(0.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Aabb, Ray};

    #[test]
    fn ray_hits_box_in_front() {
        let b = Aabb::from_center_size([0.0, 0.0, -5.0], [2.0, 2.0, 2.0]);
        let r = Ray {
            origin: [0.0, 0.0, 0.0],
            dir: [0.0, 0.0, -1.0],
        };
        let t = b.hit(&r).expect("should hit");
        assert!((t - 4.0).abs() < 1e-5);
    }

    #[test]
    fn ray_misses_box_behind() {
        let b = Aabb::from_center_size([0.0, 0.0, 5.0], [2.0, 2.0, 2.0]);
        let r = Ray {
            origin: [0.0, 0.0, 0.0],
            dir: [0.0, 0.0, -1.0],
        };
        assert!(b.hit(&r).is_none());
    }

    #[test]
    fn ray_starting_inside_hits_at_zero() {
        let b = Aabb::from_center_size([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
        let r = Ray {
            origin: [0.0, 0.0, 0.0],
            dir: [1.0, 0.0, 0.0],
        };
        assert_eq!(b.hit(&r), Some(0.0));
    }

    #[test]
    fn axis_parallel_ray_outside_slab_misses() {
        let b = Aabb::from_center_size([0.0, 0.0, -5.0], [2.0, 2.0, 2.0]);
        let r = Ray {
            origin: [5.0, 0.0, 0.0],
            dir: [0.0, 0.0, -1.0],
        };
        assert!(b.hit(&r).is_none());
    }
}
