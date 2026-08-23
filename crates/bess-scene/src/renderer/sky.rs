//! The sky pass: everything above the horizon that is not an object.
//!
//! Its own file because it is its own concern. The rest of the renderer
//! draws the plant from instance buffers; this draws a picture from a view
//! ray, and the only thing the two share is a framebuffer.
//!
//! It owns its program, its uniforms and an empty vertex array, because a
//! core profile refuses to draw without one even when there are no
//! attributes to bind.

use eframe::glow::{self, HasContext as _};

use crate::math::Vec3;
use crate::shaders;

use super::compile;

/// What the sky pass draws: the ray basis it reconstructs per pixel, and the
/// weather it is drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyFrame {
    /// Camera forward, and the right and up axes scaled by the field of
    /// view. Shared with picking, so the sky and the mouse agree about
    /// where the camera is looking.
    pub ray_basis: [Vec3; 3],
    /// Colour overhead.
    pub zenith: Vec3,
    /// Colour at the horizon.
    pub horizon: Vec3,
    /// Sun colour, before the disc and glow are scaled by it.
    pub sun_color: Vec3,
    /// Observed cloud cover, 0 clear to 1 overcast.
    pub cloud: f32,
    /// Geometric daylight, which decides how brightly the deck is lit.
    pub daylight: f32,
    /// Frame clock, for the drift.
    pub anim_s: f32,
    /// Observed wind as a world-space drift, m/s in x and z.
    pub wind: [f32; 2],
}

struct Uniforms {
    ray_f: glow::UniformLocation,
    ray_r: glow::UniformLocation,
    ray_u: glow::UniformLocation,
    light_dir: glow::UniformLocation,
    zenith: glow::UniformLocation,
    horizon: glow::UniformLocation,
    sun_color: glow::UniformLocation,
    cloud: glow::UniformLocation,
    daylight: glow::UniformLocation,
    time: glow::UniformLocation,
    wind: glow::UniformLocation,
}

/// The sky program and what it needs to draw.
pub struct SkyPass {
    program: glow::Program,
    vao: glow::VertexArray,
    uniforms: Uniforms,
}

impl SkyPass {
    /// Compile the pass. `es` selects the GLSL dialect.
    pub fn new(gl: &glow::Context, es: bool) -> Result<Self, String> {
        unsafe {
            // Shares the present pass's vertex shader: the same three
            // vertices from gl_VertexID, a different colour out the far end.
            let program = gl.create_program()?;
            let vs = compile(gl, glow::VERTEX_SHADER, &shaders::present_vertex(es))?;
            let fs = compile(gl, glow::FRAGMENT_SHADER, &shaders::sky_fragment(es))?;
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            gl.detach_shader(program, vs);
            gl.detach_shader(program, fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            let loc = |name: &str| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("sky uniform {name} not found"))
            };
            let uniforms = Uniforms {
                ray_f: loc("uRayF")?,
                ray_r: loc("uRayR")?,
                ray_u: loc("uRayU")?,
                light_dir: loc("uLightDir")?,
                zenith: loc("uZenith")?,
                horizon: loc("uHorizon")?,
                sun_color: loc("uSunColor")?,
                cloud: loc("uCloud")?,
                daylight: loc("uDaylight")?,
                time: loc("uTime")?,
                wind: loc("uWind")?,
            };

            Ok(Self {
                program,
                vao: gl.create_vertex_array()?,
                uniforms,
            })
        }
    }

    /// Draw the sky, before anything else and without touching the depth
    /// buffer, so every object still draws over it.
    ///
    /// # Safety
    ///
    /// The caller has bound the target framebuffer and set the viewport,
    /// the same contract the rest of the frame runs under, and `gl` is the
    /// context this pass was compiled against.
    pub unsafe fn draw(&self, gl: &glow::Context, sky: &SkyFrame, light_dir: Vec3) {
        let u = &self.uniforms;
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.disable(glow::DEPTH_TEST);
            gl.depth_mask(false);

            let set3 = |loc: &glow::UniformLocation, v: Vec3| {
                gl.uniform_3_f32(Some(loc), v[0], v[1], v[2]);
            };
            set3(&u.ray_f, sky.ray_basis[0]);
            set3(&u.ray_r, sky.ray_basis[1]);
            set3(&u.ray_u, sky.ray_basis[2]);
            set3(&u.light_dir, light_dir);
            set3(&u.zenith, sky.zenith);
            set3(&u.horizon, sky.horizon);
            set3(&u.sun_color, sky.sun_color);
            gl.uniform_1_f32(Some(&u.cloud), sky.cloud);
            gl.uniform_1_f32(Some(&u.daylight), sky.daylight);
            gl.uniform_1_f32(Some(&u.time), sky.anim_s);
            gl.uniform_2_f32(Some(&u.wind), sky.wind[0], sky.wind[1]);

            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            gl.depth_mask(true);
            gl.enable(glow::DEPTH_TEST);
        }
    }

    /// Free the pass's GL resources.
    ///
    /// # Safety
    ///
    /// `gl` is the context this pass was compiled against, and nothing draws
    /// with it afterwards.
    pub unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
        }
    }
}
