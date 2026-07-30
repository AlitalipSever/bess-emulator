//! Material palette. Site materials carry fixed, physically plausible
//! albedos (light-gray container steel, concrete, asphalt) so that daylight
//! actually reads as daylight; the sun model only scales the light, never
//! the material. Signal colors (charge, discharge, alarm) are fixed so
//! their meaning never shifts.

/// Signal colors, constant across themes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Charging (power flowing into the plant).
    pub charge: [f32; 3],
    /// Discharging (power flowing to the grid).
    pub discharge: [f32; 3],
    /// Alarm / fault accents.
    pub alarm: [f32; 3],
    /// Selection highlight.
    pub select: [f32; 3],
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            charge: [0.20, 0.75, 0.62],
            discharge: [1.00, 0.66, 0.24],
            alarm: [0.95, 0.25, 0.20],
            select: [1.00, 0.85, 0.30],
        }
    }
}

/// Site material albedos and sky colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Surrounding terrain (dry grassland), extends to the horizon.
    pub terrain: [f32; 3],
    /// Site gravel apron.
    pub gravel: [f32; 3],
    /// Concrete pads.
    pub pad: [f32; 3],
    /// Asphalt road strip.
    pub aisle: [f32; 3],
    /// Road lane markings.
    pub marking: [f32; 3],
    /// Container and cabinet steel (RAL-7035-like light gray).
    pub steel: [f32; 3],
    /// Darker steel (plinths, trays, poles).
    pub steel_dark: [f32; 3],
    /// Roof caps.
    pub roof: [f32; 3],
    /// Vent fins, insulators, small details.
    pub fin: [f32; 3],
    /// Recessed gauge background.
    pub gauge_bg: [f32; 3],
    /// Handles, light mast heads.
    pub handle: [f32; 3],
    /// Hemisphere light, sky side (scaled by ambient per frame).
    pub sky: [f32; 3],
    /// Hemisphere light, ground side.
    pub ground_light: [f32; 3],
    /// Clear/fog color at full day.
    pub day_sky: [f32; 3],
    /// Clear/fog color at night.
    pub night_sky: [f32; 3],
}

impl Default for Style {
    fn default() -> Self {
        Self {
            terrain: [0.33, 0.36, 0.27],
            gravel: [0.47, 0.45, 0.40],
            pad: [0.58, 0.58, 0.56],
            aisle: [0.29, 0.29, 0.31],
            marking: [0.78, 0.78, 0.74],
            steel: [0.75, 0.76, 0.77],
            steel_dark: [0.24, 0.25, 0.27],
            roof: [0.60, 0.61, 0.62],
            fin: [0.40, 0.41, 0.43],
            gauge_bg: [0.09, 0.10, 0.11],
            handle: [0.86, 0.86, 0.87],
            sky: [0.42, 0.48, 0.58],
            ground_light: [0.34, 0.33, 0.29],
            day_sky: [0.58, 0.69, 0.84],
            night_sky: [0.015, 0.025, 0.055],
        }
    }
}
