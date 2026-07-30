//! The only module that talks to GL, and therefore the only unsafe one:
//! the `glow` API is unsafe by design. Everything above this module hands
//! over plain `f32` instance data and uniform values.
//!
//! The scene renders into its own offscreen framebuffer with a real depth
//! buffer, then blits into egui's target. This sidesteps the fact that
//! neither eframe's native surface nor the WebGL2 canvas is guaranteed a
//! depth buffer, and it isolates our GL state from egui's.
#![allow(unsafe_code)]

use eframe::glow::{self, HasContext as _};

use crate::instances::FPI;
use crate::math::{Mat4, Vec3};
use crate::{mesh, shaders};

/// Everything one frame needs: instance streams plus uniforms. Built by the
/// scene widget (pure code), consumed here.
pub struct FrameData {
    /// Ground instances (drawn without the shadow pass).
    pub ground: Vec<f32>,
    /// Object instances (static furniture + dynamic layer).
    pub objects: Vec<f32>,
    /// Combined view-projection matrix.
    pub view_proj: Mat4,
    /// Camera position.
    pub eye: Vec3,
    /// Light direction (FROM the light).
    pub light_dir: Vec3,
    /// Light color, premultiplied by intensity.
    pub light_color: Vec3,
    /// Hemisphere sky color.
    pub sky: Vec3,
    /// Hemisphere ground color.
    pub ground_light: Vec3,
    /// Fog / clear color.
    pub fog_color: Vec3,
    /// Fog start and full distance, m.
    pub fog_range: [f32; 2],
}

struct Target {
    fbo: glow::Framebuffer,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    size: (i32, i32),
}

struct Uniforms {
    view_proj: glow::UniformLocation,
    light_dir: glow::UniformLocation,
    light_color: glow::UniformLocation,
    sky: glow::UniformLocation,
    ground: glow::UniformLocation,
    fog: glow::UniformLocation,
    eye: glow::UniformLocation,
    shadow: glow::UniformLocation,
    floor_y: glow::UniformLocation,
    fog_range: glow::UniformLocation,
}

/// GL resources of the scene. One instance per GL context.
pub struct Renderer {
    program: glow::Program,
    vao: glow::VertexArray,
    mesh_vbo: glow::Buffer,
    ground_vbo: glow::Buffer,
    obj_vbo: glow::Buffer,
    uniforms: Uniforms,
    target: Option<Target>,
}

fn compile(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(kind)?;
        gl.shader_source(shader, src);
        gl.compile_shader(shader);
        if gl.get_shader_compile_status(shader) {
            Ok(shader)
        } else {
            Err(gl.get_shader_info_log(shader))
        }
    }
}

impl Renderer {
    /// Compile the program and set up the shared cube mesh. `es` selects the
    /// GLSL dialect (WebGL2 vs desktop core).
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let es = cfg!(target_arch = "wasm32");
        unsafe {
            let program = gl.create_program()?;
            let vs = compile(gl, glow::VERTEX_SHADER, &shaders::vertex(es))?;
            let fs = compile(gl, glow::FRAGMENT_SHADER, &shaders::fragment(es))?;
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

            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let mesh_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&mesh::cube_mesh()),
                glow::STATIC_DRAW,
            );
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
            gl.enable_vertex_attrib_array(1);
            let ground_vbo = gl.create_buffer()?;
            let obj_vbo = gl.create_buffer()?;
            gl.bind_vertex_array(None);

            let loc = |name: &str| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("uniform {name} not found"))
            };
            let uniforms = Uniforms {
                view_proj: loc("uViewProj")?,
                light_dir: loc("uLightDir")?,
                light_color: loc("uLightColor")?,
                sky: loc("uSky")?,
                ground: loc("uGround")?,
                fog: loc("uFog")?,
                eye: loc("uEye")?,
                shadow: loc("uShadow")?,
                floor_y: loc("uFloorY")?,
                fog_range: loc("uFogRange")?,
            };

            Ok(Self {
                program,
                vao,
                mesh_vbo,
                ground_vbo,
                obj_vbo,
                uniforms,
                target: None,
            })
        }
    }

    /// (Re)create the offscreen color+depth target at the given size.
    fn ensure_target(&mut self, gl: &glow::Context, w: i32, h: i32) -> Result<(), String> {
        if let Some(t) = &self.target {
            if t.size == (w, h) {
                return Ok(());
            }
        }
        unsafe {
            if let Some(t) = self.target.take() {
                gl.delete_framebuffer(t.fbo);
                gl.delete_renderbuffer(t.color);
                gl.delete_renderbuffer(t.depth);
            }
            let fbo = gl.create_framebuffer()?;
            let color = gl.create_renderbuffer()?;
            let depth = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(color),
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(format!("scene framebuffer incomplete: {status:#x}"));
            }
            self.target = Some(Target {
                fbo,
                color,
                depth,
                size: (w, h),
            });
        }
        Ok(())
    }

    /// Bind `buf`, upload `data`, and wire the instanced attributes 2..=5.
    unsafe fn upload_instances(gl: &glow::Context, buf: glow::Buffer, data: &[f32]) {
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buf));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(data),
                glow::DYNAMIC_DRAW,
            );
            let stride = (FPI * 4) as i32;
            for (loc, size, off) in [(2u32, 3, 0), (3, 3, 12), (4, 3, 24), (5, 1, 36)] {
                gl.vertex_attrib_pointer_f32(loc, size, glow::FLOAT, false, stride, off);
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_divisor(loc, 1);
            }
        }
    }

    /// Render one frame into the callback viewport.
    ///
    /// `target_fbo` is egui's current draw target (`None` = default
    /// framebuffer); `viewport` comes from the paint callback info.
    pub fn paint(
        &mut self,
        gl: &glow::Context,
        target_fbo: Option<glow::Framebuffer>,
        viewport: &egui::PaintCallbackInfo,
        frame: &FrameData,
    ) {
        let vp = viewport.viewport_in_pixels();
        let (w, h) = (vp.width_px.max(1), vp.height_px.max(1));
        if let Err(err) = self.ensure_target(gl, w, h) {
            // A broken FBO would panic deep in GL otherwise; skip the frame.
            log::warn!("bess-scene: {err}");
            return;
        }
        let Some(target) = &self.target else { return };
        let u = &self.uniforms;

        unsafe {
            let scissor_was_on = gl.is_enabled(glow::SCISSOR_TEST);
            gl.disable(glow::SCISSOR_TEST);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
            gl.viewport(0, 0, w, h);
            gl.enable(glow::DEPTH_TEST);
            gl.enable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.depth_mask(true);
            gl.clear_color(
                frame.fog_color[0],
                frame.fog_color[1],
                frame.fog_color[2],
                1.0,
            );
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.uniform_matrix_4_f32_slice(Some(&u.view_proj), false, &frame.view_proj);
            gl.uniform_3_f32(
                Some(&u.light_dir),
                frame.light_dir[0],
                frame.light_dir[1],
                frame.light_dir[2],
            );
            gl.uniform_3_f32(
                Some(&u.light_color),
                frame.light_color[0],
                frame.light_color[1],
                frame.light_color[2],
            );
            gl.uniform_3_f32(Some(&u.sky), frame.sky[0], frame.sky[1], frame.sky[2]);
            gl.uniform_3_f32(
                Some(&u.ground),
                frame.ground_light[0],
                frame.ground_light[1],
                frame.ground_light[2],
            );
            gl.uniform_3_f32(
                Some(&u.fog),
                frame.fog_color[0],
                frame.fog_color[1],
                frame.fog_color[2],
            );
            gl.uniform_3_f32(Some(&u.eye), frame.eye[0], frame.eye[1], frame.eye[2]);
            gl.uniform_1_f32(Some(&u.floor_y), 0.0);
            gl.uniform_2_f32(Some(&u.fog_range), frame.fog_range[0], frame.fog_range[1]);

            // 1. ground (no shadow pass on it)
            gl.uniform_1_f32(Some(&u.shadow), 0.0);
            Self::upload_instances(gl, self.ground_vbo, &frame.ground);
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 36, (frame.ground.len() / FPI) as i32);

            // 2. fake planar shadows of the objects, blended onto the pad
            gl.uniform_1_f32(Some(&u.shadow), 1.0);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.depth_mask(false);
            Self::upload_instances(gl, self.obj_vbo, &frame.objects);
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 36, (frame.objects.len() / FPI) as i32);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);

            // 3. the objects themselves
            gl.uniform_1_f32(Some(&u.shadow), 0.0);
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 36, (frame.objects.len() / FPI) as i32);

            // blit into egui's target and restore its state
            gl.bind_vertex_array(None);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target_fbo);
            gl.blit_framebuffer(
                0,
                0,
                w,
                h,
                vp.left_px,
                vp.from_bottom_px,
                vp.left_px + w,
                vp.from_bottom_px + h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, target_fbo);
            if scissor_was_on {
                gl.enable(glow::SCISSOR_TEST);
            }

            // Debug builds: surface GL errors instead of rendering garbage
            // silently. GL error checks stall the pipeline, so never in
            // release.
            #[cfg(debug_assertions)]
            {
                let err = gl.get_error();
                if err != glow::NO_ERROR {
                    log::warn!("bess-scene: GL error {err:#x} during frame");
                }
            }
        }
    }

    /// Delete all GL resources. Call before the context goes away.
    pub fn destroy(&mut self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.mesh_vbo);
            gl.delete_buffer(self.ground_vbo);
            gl.delete_buffer(self.obj_vbo);
            if let Some(t) = self.target.take() {
                gl.delete_framebuffer(t.fbo);
                gl.delete_renderbuffer(t.color);
                gl.delete_renderbuffer(t.depth);
            }
        }
    }
}
