//! Camera with two modes sharing one orientation convention, so switching
//! modes never jumps the view:
//!
//! - **Orbit** (default): revolve around a target, pan moves the target.
//! - **Fly**: free camera; WASD + Q/E move, dragging looks around.
//!
//! Pure math; input routing lives in the scene widget.

use crate::math::{self, Mat4, Ray, Vec3};

/// Near clip plane, m.
const NEAR: f32 = 0.5;
/// Far clip plane, m (the site is ~160 m long).
const FAR: f32 = 800.0;

/// Camera control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    /// Revolve around `target`.
    Orbit,
    /// Free flight.
    Fly,
}

/// Movement keys held this frame (fly mode).
#[derive(Debug, Clone, Copy, Default)]
pub struct FlyInput {
    /// W.
    pub forward: bool,
    /// S.
    pub back: bool,
    /// A.
    pub left: bool,
    /// D.
    pub right: bool,
    /// E (up).
    pub up: bool,
    /// Q (down).
    pub down: bool,
    /// Shift (sprint).
    pub fast: bool,
}

/// The scene camera.
#[derive(Debug, Clone)]
pub struct Camera {
    /// Active mode.
    pub mode: CameraMode,
    /// Orbit target (also the pan anchor).
    pub target: Vec3,
    /// Azimuth, radians.
    pub yaw: f32,
    /// Elevation, radians.
    pub pitch: f32,
    /// Orbit distance, m.
    pub dist: f32,
    /// Fly-mode position.
    pub pos: Vec3,
    /// Vertical field of view, radians.
    pub fovy: f32,
    /// Slow automatic yaw while idle in orbit mode.
    pub auto_orbit: bool,
}

impl Camera {
    /// Wide overview of the whole site.
    pub fn overview() -> Self {
        Self {
            mode: CameraMode::Orbit,
            target: [10.0, 1.0, 0.0],
            yaw: 1.05,
            pitch: 0.55,
            dist: 110.0,
            pos: [0.0, 2.0, 0.0],
            fovy: 0.78,
            auto_orbit: true,
        }
    }

    /// Eye position in world space.
    pub fn eye(&self) -> Vec3 {
        match self.mode {
            CameraMode::Orbit => math::add(self.target, self.offset()),
            CameraMode::Fly => self.pos,
        }
    }

    /// Unit view direction.
    pub fn forward(&self) -> Vec3 {
        math::scale(self.offset_dir(), -1.0)
    }

    fn offset_dir(&self) -> Vec3 {
        math::normalize([
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        ])
    }

    fn offset(&self) -> Vec3 {
        math::scale(self.offset_dir(), self.dist)
    }

    /// Combined view-projection matrix.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let eye = self.eye();
        let center = math::add(eye, self.forward());
        math::mat_mul(
            &math::perspective(self.fovy, aspect, NEAR, FAR),
            &math::look_at(eye, center, [0.0, 1.0, 0.0]),
        )
    }

    /// Switch mode without moving the viewpoint.
    pub fn set_mode(&mut self, mode: CameraMode) {
        if mode == self.mode {
            return;
        }
        match mode {
            CameraMode::Fly => self.pos = self.eye(),
            CameraMode::Orbit => {
                // Re-anchor the target in front of the current position.
                self.target = math::add(self.pos, math::scale(self.forward(), self.dist));
            }
        }
        self.mode = mode;
    }

    /// Primary drag: orbit around the target, or look around in fly mode.
    pub fn look_drag(&mut self, dx: f32, dy: f32) {
        match self.mode {
            CameraMode::Orbit => {
                self.yaw += dx * 0.008;
                self.pitch = (self.pitch + dy * 0.006).clamp(0.06, 1.35);
            }
            CameraMode::Fly => {
                self.yaw += dx * 0.004;
                self.pitch = (self.pitch - dy * 0.004).clamp(-1.4, 1.4);
            }
        }
    }

    /// Secondary drag: slide the orbit target on the ground plane.
    pub fn pan_drag(&mut self, dx: f32, dy: f32) {
        let fwd = self.forward();
        let right = math::normalize(math::cross(fwd, [0.0, 1.0, 0.0]));
        let ahead = math::normalize([fwd[0], 0.0, fwd[2]]);
        let k = self.dist * 0.0016;
        let delta = math::add(math::scale(right, -dx * k), math::scale(ahead, dy * k));
        match self.mode {
            CameraMode::Orbit => self.target = math::add(self.target, delta),
            CameraMode::Fly => self.pos = math::add(self.pos, delta),
        }
    }

    /// Scroll: change orbit distance, or dolly along the view in fly mode.
    pub fn zoom(&mut self, scroll: f32) {
        match self.mode {
            CameraMode::Orbit => {
                self.dist = (self.dist * (1.0 - scroll * 0.0015)).clamp(6.0, 300.0);
            }
            CameraMode::Fly => {
                let step = math::scale(self.forward(), scroll * 0.05);
                self.pos = math::add(self.pos, step);
                self.pos[1] = self.pos[1].max(0.6);
            }
        }
    }

    /// Advance fly movement by `dt` seconds.
    pub fn fly_move(&mut self, input: FlyInput, dt: f32) {
        if self.mode != CameraMode::Fly {
            return;
        }
        let speed = if input.fast { 40.0 } else { 14.0 } * dt;
        let fwd = self.forward();
        let right = math::normalize(math::cross(fwd, [0.0, 1.0, 0.0]));
        let mut step = [0.0f32; 3];
        let mut moved = false;
        let mut apply = |step: &mut [f32; 3], v: [f32; 3], sign: f32, on: bool| {
            if on {
                *step = math::add(*step, math::scale(v, sign));
                moved = true;
            }
        };
        apply(&mut step, fwd, 1.0, input.forward);
        apply(&mut step, fwd, -1.0, input.back);
        apply(&mut step, right, 1.0, input.right);
        apply(&mut step, right, -1.0, input.left);
        apply(&mut step, [0.0, 1.0, 0.0], 1.0, input.up);
        apply(&mut step, [0.0, 1.0, 0.0], -1.0, input.down);
        if moved {
            self.pos = math::add(self.pos, math::scale(math::normalize(step), speed));
            // Stay above the ground; hard collision is not the point here.
            self.pos[1] = self.pos[1].max(0.6);
        }
    }

    /// Idle auto-orbit, `dt` seconds.
    pub fn idle(&mut self, dt: f32) {
        if self.auto_orbit && self.mode == CameraMode::Orbit {
            self.yaw += dt * 0.05;
        }
    }

    /// Ray through a point given in normalized device coordinates
    /// (x right, y up, both -1..1).
    pub fn ray_through(&self, ndc_x: f32, ndc_y: f32, aspect: f32) -> Ray {
        let fwd = self.forward();
        let right = math::normalize(math::cross(fwd, [0.0, 1.0, 0.0]));
        let up = math::cross(right, fwd);
        let tan = (self.fovy / 2.0).tan();
        let dir = math::normalize(math::add(
            fwd,
            math::add(
                math::scale(right, ndc_x * tan * aspect),
                math::scale(up, ndc_y * tan),
            ),
        ));
        Ray {
            origin: self.eye(),
            dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Camera, CameraMode};
    use crate::math;

    #[test]
    fn orbit_eye_sits_at_orbit_distance() {
        let cam = Camera::overview();
        let d = math::sub(cam.eye(), cam.target);
        assert!((math::dot(d, d).sqrt() - cam.dist).abs() < 1e-3);
    }

    #[test]
    fn center_ray_matches_forward() {
        let cam = Camera::overview();
        let ray = cam.ray_through(0.0, 0.0, 1.6);
        let f = cam.forward();
        assert!(math::dot(ray.dir, f) > 0.9999);
    }

    #[test]
    fn mode_switch_keeps_the_eye_in_place() {
        let mut cam = Camera::overview();
        let eye_before = cam.eye();
        cam.set_mode(CameraMode::Fly);
        let eye_after = cam.eye();
        for i in 0..3 {
            assert!((eye_before[i] - eye_after[i]).abs() < 1e-4);
        }
        // And back: forward direction survives the round trip.
        let f_before = cam.forward();
        cam.set_mode(CameraMode::Orbit);
        let f_after = cam.forward();
        assert!(math::dot(f_before, f_after) > 0.9999);
    }

    #[test]
    fn fly_never_sinks_below_ground() {
        let mut cam = Camera::overview();
        cam.set_mode(CameraMode::Fly);
        cam.pitch = -1.2;
        for _ in 0..200 {
            cam.fly_move(
                super::FlyInput {
                    forward: true,
                    ..Default::default()
                },
                0.05,
            );
        }
        assert!(cam.pos[1] >= 0.6);
    }
}
