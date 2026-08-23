//! The sun: where it is, and what that does to the light.
//!
//! Two different things, deliberately in two different files.
//! [`position`] is astronomy, a pure function of UTC and the site's
//! coordinates, and it makes claims about the world that tests hold against
//! published geometry and against a measured irradiance year. [`light`] turns
//! a position into colour and intensity, and every line of it is a choice
//! about a picture. Keeping them apart means a reader can tell which is
//! which without reading the doc comments, which is the whole reason the
//! split is worth two files instead of one.
//!
//! The line the scene does not cross is unchanged: the kernel never reads any
//! of this, and the only authority for radiation in the energy accounting is
//! the measured irradiance series.
//!
//! **World axes.** The site layout already fixes them: `layout.rs` puts row 0
//! south of the road at negative z and row 1 north of it at positive z. So
//! +z is north, +y is up, and +x is east. A northern-hemisphere sun therefore
//! crosses the sky on the negative-z side, and the light it casts travels
//! toward +z, which `the_sun_comes_from_the_south_at_midday` holds.

pub mod light;
pub mod position;

pub use light::{sun_at, SunLight};
pub use position::{sun_position, SunPosition};

/// Hermite ramp from 0 at `lo` to 1 at `hi`.
///
/// Shared by both children: the refraction taper in [`position`] and the
/// twilight ramps in [`light`] want the same curve, and two copies of a curve
/// is how two copies drift apart.
pub(crate) fn smoothstep(lo: f64, hi: f64, x: f64) -> f64 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
pub(crate) mod testing {
    use bess_core::config::SiteLocation;

    /// GW-01's nominal location, the DWD station its weather comes from.
    /// `tests/solar_daylight.rs` asserts this has not drifted from
    /// `PlantConfig::gw01()`.
    pub const SITE: SiteLocation = SiteLocation {
        latitude_deg: 52.21,
        longitude_deg: 14.12,
    };

    /// 2026-01-01 00:00:00 UTC.
    pub const NEW_YEAR: i64 = 1_767_225_600;

    /// Midnight UTC of the day `days` after New Year 2026.
    pub fn day(days: i64) -> i64 {
        NEW_YEAR + days * 86_400
    }
}

#[cfg(test)]
mod tests {
    use super::smoothstep;

    #[test]
    fn the_shared_ramp_is_flat_outside_its_band_and_smooth_inside_it() {
        assert!((smoothstep(-6.0, 6.0, -10.0) - 0.0).abs() < 1e-12);
        assert!((smoothstep(-6.0, 6.0, 10.0) - 1.0).abs() < 1e-12);
        assert!((smoothstep(-6.0, 6.0, 0.0) - 0.5).abs() < 1e-12);
        // Monotone, and no step anywhere across the band.
        let mut previous = 0.0;
        for i in 0..=1_200 {
            let x = -6.0 + f64::from(i) * 0.01;
            let s = smoothstep(-6.0, 6.0, x);
            assert!(s >= previous - 1e-12 && s - previous < 0.01);
            previous = s;
        }
    }
}
