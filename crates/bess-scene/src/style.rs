//! Material palette. Neutral site colors derive from the page background so
//! the scene reads correctly on light and dark surfaces; the signal colors
//! (charge, discharge, alarm) are fixed so their meaning never shifts.

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

/// Neutral material colors derived from the page background.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Site gravel apron.
    pub gravel: [f32; 3],
    /// Concrete pads.
    pub pad: [f32; 3],
    /// Painted road / aisle strip.
    pub aisle: [f32; 3],
    /// Container and cabinet steel.
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
    /// Hemisphere light, sky side.
    pub sky: [f32; 3],
    /// Hemisphere light, ground side.
    pub ground_light: [f32; 3],
    /// Page background the style was derived from.
    pub background: [f32; 3],
}

/// Default dark page background for the standalone viewer.
pub const DARK_BACKGROUND: [f32; 3] = [0.055, 0.060, 0.075];

impl Style {
    /// Derive the neutral material set from a page background color (0..1).
    pub fn from_background(bg: [f32; 3]) -> Style {
        let tint = |k: f32, lift: f32| {
            [
                (bg[0] * k + lift).min(1.0),
                (bg[1] * k + lift).min(1.0),
                (bg[2] * k + lift * 0.97).min(1.0),
            ]
        };
        Style {
            gravel: tint(0.8, 0.02),
            pad: tint(0.88, 0.04),
            aisle: tint(0.7, 0.03),
            steel: tint(0.62, 0.16),
            steel_dark: tint(0.45, 0.10),
            roof: tint(0.55, 0.2),
            fin: tint(0.5, 0.13),
            gauge_bg: tint(0.4, 0.05),
            handle: tint(0.9, 0.25),
            sky: tint(0.55, 0.34),
            ground_light: tint(0.4, 0.12),
            background: bg,
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::from_background(DARK_BACKGROUND)
    }
}
