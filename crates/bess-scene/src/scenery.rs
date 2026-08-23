//! What the sky is doing, as the scene needs it.
//!
//! The kernel's inputs stop at ambient temperature and irradiance, because
//! those are the two the physics consumes. The other four compiled series,
//! cloud cover, precipitation amount and form, and wind, are labelled
//! scenery-only in `bess-data` and DATA-LICENSES.md, and this is where they
//! arrive.
//!
//! They arrive as plain numbers. Nothing here names a `bess-data` type,
//! because `bess-scene` builds without the `sim` feature and because a
//! scenery type the kernel cannot see is a scenery type that cannot
//! accidentally become physics. The viewer does the mapping.

/// What is falling, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precip {
    /// Nothing.
    #[default]
    None,
    /// Liquid.
    Rain,
    /// Solid.
    Snow,
    /// Mixed, or reported without a form on an hour cold enough to matter.
    Sleet,
}

/// One hour of observations, in the units the dataset publishes them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Observed {
    /// Total cloud cover, okta: 0 clear to 8 overcast.
    pub cloud_okta: u8,
    /// Measured global horizontal irradiance, W/m2.
    pub irradiance_wm2: f32,
    /// Precipitation over the hour, millimetres.
    pub precip_mm_h: f32,
    /// What was falling.
    pub precip: Precip,
    /// Wind speed, m/s.
    pub wind_ms: f32,
    /// Wind direction, degrees the wind blows *from*, clockwise from north.
    pub wind_dir_deg: f32,
}

/// The sky, ready to draw.
///
/// `Default` is a clear sky rather than a zeroed struct, because a zeroed
/// struct means `dimming: 0.0`, which is a sun with nothing coming out of it.
/// A default that has to be avoided is a trap left in a public type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scenery {
    /// Cloud cover as a fraction, 0 clear to 1 overcast.
    pub cloud: f32,
    /// Measured irradiance over what a clear sky would have delivered at this
    /// solar elevation, clamped to 0..1. Drives how bright the sun is drawn.
    pub dimming: f32,
    /// Precipitation over the hour, millimetres.
    pub precip_mm_h: f32,
    /// What is falling.
    pub precip: Precip,
    /// Wind speed, m/s.
    pub wind_ms: f32,
    /// Wind direction, degrees the wind blows from, clockwise from north.
    pub wind_dir_deg: f32,
}

/// Below this solar elevation the clear-sky denominator is too small for the
/// ratio to carry information.
const RATIO_FLOOR_DEG: f64 = 3.0;

impl Default for Scenery {
    fn default() -> Self {
        Self::clear()
    }
}

impl Scenery {
    /// A clear, still sky. What the scene draws when there is no observation
    /// to draw from, and the identity the synthetic weather driver produces.
    pub fn clear() -> Self {
        Self {
            cloud: 0.0,
            dimming: 1.0,
            precip_mm_h: 0.0,
            precip: Precip::None,
            wind_ms: 0.0,
            wind_dir_deg: 0.0,
        }
    }

    /// Build from one hour of observations and the sun's position.
    ///
    /// Two independent witnesses to the same sky arrive here and stay
    /// independent. Cloud cover is a human or ceilometer observation in
    /// eighths; dimming is a pyranometer reading divided by a clear-sky
    /// model. They are used for different things on purpose, cloud for the
    /// colour of the sky and dimming for the brightness of the sun, so that
    /// when they disagree the picture shows it rather than averaging it away.
    pub fn from_observed(o: &Observed, solar_elevation_deg: f64) -> Self {
        Self {
            cloud: f32::from(o.cloud_okta.min(8)) / 8.0,
            dimming: dimming(o.irradiance_wm2, solar_elevation_deg),
            precip_mm_h: o.precip_mm_h.max(0.0),
            precip: o.precip,
            wind_ms: o.wind_ms.max(0.0),
            wind_dir_deg: o.wind_dir_deg,
        }
    }
}

/// Measured irradiance over the clear-sky expectation, clamped to 0..1.
///
/// The expectation is Haurwitz's model, one line and one fitted pair, which
/// is the right depth for deciding how bright to draw a sun. Anything more
/// careful would still be a number that never leaves the view layer.
///
/// Below [`RATIO_FLOOR_DEG`] the denominator collapses and the ratio stops
/// meaning anything, so it reports 1. That is safe rather than arbitrary: the
/// only thing dimming multiplies is the sun's own brightness, which the
/// lighting has already taken to zero by the time the sun is that low.
pub fn dimming(irradiance_wm2: f32, solar_elevation_deg: f64) -> f32 {
    if solar_elevation_deg < RATIO_FLOOR_DEG {
        return 1.0;
    }
    let cos_zenith = solar_elevation_deg.to_radians().sin();
    let clear_sky = 1098.0 * cos_zenith * (-0.059 / cos_zenith).exp();
    if clear_sky <= 1.0 {
        return 1.0;
    }
    (f64::from(irradiance_wm2) / clear_sky).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::{dimming, Observed, Precip, Scenery};

    #[test]
    fn a_clear_sky_is_the_identity_and_the_default() {
        assert_eq!(Scenery::default(), Scenery::clear());
        let clear = Scenery::clear();
        assert!((clear.dimming - 1.0).abs() < f32::EPSILON);
        assert!(clear.cloud.abs() < f32::EPSILON);
        assert_eq!(clear.precip, Precip::None);
    }

    #[test]
    fn okta_becomes_a_fraction_and_cannot_leave_the_scale() {
        for (okta, expected) in [(0u8, 0.0), (4, 0.5), (8, 1.0), (9, 1.0), (255, 1.0)] {
            let o = Observed {
                cloud_okta: okta,
                ..Observed::default()
            };
            let s = Scenery::from_observed(&o, 30.0);
            assert!(
                (s.cloud - expected).abs() < 1e-6,
                "{okta} okta became {}",
                s.cloud
            );
        }
    }

    #[test]
    fn dimming_is_bounded_and_ordered() {
        // A sun 40 degrees up delivers roughly 700 W/m2 through a clear sky,
        // so these three readings should order themselves without the test
        // needing to know the exact clear-sky value.
        let overcast = dimming(60.0, 40.0);
        let hazy = dimming(350.0, 40.0);
        let clear = dimming(690.0, 40.0);
        assert!((0.0..=1.0).contains(&overcast));
        assert!(overcast < hazy && hazy < clear);
        assert!(clear > 0.8, "a clear hour read only {clear:.3}");
        // Above the model is still bounded: a bright edge-of-cloud hour can
        // beat the clear-sky expectation and must not produce a sun brighter
        // than the sun.
        assert!((dimming(1400.0, 40.0) - 1.0).abs() < f32::EPSILON);
        assert!(dimming(0.0, 40.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_sun_too_low_to_divide_by_reports_no_dimming() {
        // Not a fudge: the ratio's denominator collapses here, and the only
        // thing it multiplies has already gone to zero.
        assert!((dimming(0.0, 1.0) - 1.0).abs() < f32::EPSILON);
        assert!((dimming(0.0, -20.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn dimming_moves_smoothly_as_the_sun_climbs() {
        // Continuity, the category PR1's review added. The ratio crosses its
        // floor at 3 degrees; a step there would flicker the sun at every
        // sunrise, so the reading either side has to meet.
        let steady_wm2 = 120.0;
        let mut previous = dimming(steady_wm2, 0.0);
        let mut worst: f32 = 0.0;
        for i in 0..=600 {
            let elevation = f64::from(i) * 0.02;
            let d = dimming(steady_wm2, elevation);
            worst = worst.max((d - previous).abs());
            previous = d;
        }
        assert!(worst < 0.05, "dimming stepped {worst:.4} across the floor");
    }
}
