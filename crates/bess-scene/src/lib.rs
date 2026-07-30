//! The view layer: a 3D instanced-rendering scene of the GW-01 site plus
//! egui panels, all inside one canvas over one `glow` context (WebGL2 in the
//! browser, OpenGL natively).
//!
//! Design rule: the scene is a projection of the state tree. Everything here
//! reads `&SiteState` and emits [`ViewerCommand`]s; nothing in this crate
//! steps the kernel or owns plant state. The optional `sim` feature adds
//! [`viewer::ViewerApp`], a self-contained application that owns a
//! `Simulation` and mediates between it and the view.
//!
//! Module map:
//! - `math`      camera matrices and rays (pure)
//! - `sun`       day-night model from the simulated clock (pure)
//! - `layout`    GW-01 site geometry and picking boxes (pure)
//! - `style`     material palette
//! - `mesh`      the unit cube every instance reuses
//! - `shaders`   GLSL sources, composed per platform
//! - `instances` state tree -> cube instances (pure)
//! - `renderer`  the only module with GL calls (and the only unsafe one)
//! - `camera`    orbit / pan / fly control
//! - `scene`     egui widget tying camera, picking and renderer together
//! - `panels`    site and selection panels

pub mod camera;
pub mod instances;
pub mod layout;
pub mod math;
pub mod mesh;
pub mod panels;
pub mod renderer;
pub mod scene;
pub mod shaders;
pub mod style;
pub mod sun;
#[cfg(feature = "sim")]
pub mod viewer;

/// Commands the view emits toward whoever owns the simulation (the viewer
/// app natively, the wasm shell in the browser).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewerCommand {
    /// Change the time acceleration factor.
    SetSpeed(f64),
    /// Write (`Some`, W, positive = discharge) or clear (`None`) the
    /// external site setpoint.
    SetSetpoint(Option<f64>),
}
