//! Where the sun is. Astronomy, and nothing else.
//!
//! A pure function of UTC and the site's coordinates. Nothing in this file
//! decides how anything looks; that is [`super::light`]'s job, and the split
//! is deliberate, because one of these two files makes claims about the world
//! and the other makes choices about a picture.

use std::f64::consts::TAU;

use bess_core::config::SiteLocation;

/// Earth mean radius over one astronomical unit: the small parallax
/// correction that turns a geocentric sun position into a topocentric one.
/// 6371.01 km / 149 597 890 km, the pair the PSA algorithm publishes.
const PARALLAX: f64 = 6371.01 / 149_597_890.0;

/// Where the sun is, seen from a point on Earth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunPosition {
    /// Apparent elevation above the horizon, degrees. Negative when the sun
    /// is down. Refracted, so it is where the sun looks rather than where it
    /// geometrically is.
    pub elevation_deg: f64,
    /// Azimuth, degrees clockwise from north: 0 north, 90 east, 180 south.
    pub azimuth_deg: f64,
}

/// Sun position for a UTC instant at a point on Earth.
///
/// PSA (Blanco-Muriel et al., *Computing the solar vector*, Solar Energy 70,
/// 2001) with the refitted coefficient set published in the 2020 update,
/// followed by the Saemundsson refraction correction. Roughly fifty lines of
/// arithmetic and no dependency, which is the point: this workspace keeps its
/// dependency list short because the whole thing has to compile to WASM.
///
/// What this repository verifies is not the paper's own accuracy claim but
/// bounded agreement with the site's solar geometry (solstice elevations,
/// equinox azimuths, the timing of solar noon at this longitude) and with the
/// measured DWD irradiance series, which is an independent witness. Those
/// checks live in the tests below and in `tests/solar_daylight.rs`.
pub fn sun_position(unix_time_s: i64, location: SiteLocation) -> SunPosition {
    // Days since J2000.0, fraction included. The Unix epoch is JD 2440587.5
    // and J2000.0 is JD 2451545.0, so the offset is a constant.
    let days = unix_time_s as f64 / 86_400.0 - 10_957.5;
    let decimal_hours = unix_time_s.rem_euclid(86_400) as f64 / 3600.0;

    // -- ecliptic coordinates of the sun ------------------------------
    let omega = 2.267_127_827 - 9.300_339_267e-4 * days;
    let mean_longitude = 4.895_036_035 + 1.720_279_602e-2 * days;
    let mean_anomaly = 6.239_468_336 + 1.720_200_135e-2 * days;
    let ecliptic_longitude = mean_longitude
        + 3.338_320_972e-2 * mean_anomaly.sin()
        + 3.497_596_876e-4 * (2.0 * mean_anomaly).sin()
        - 1.544_353_226e-4
        - 8.689_729_360e-6 * omega.sin();
    let obliquity = 4.090_904_909e-1 - 6.213_605_399e-9 * days + 4.418_094_944e-5 * omega.cos();

    // -- celestial coordinates ----------------------------------------
    let sin_longitude = ecliptic_longitude.sin();
    let right_ascension = (obliquity.cos() * sin_longitude).atan2(ecliptic_longitude.cos());
    let declination = (obliquity.sin() * sin_longitude).asin();

    // -- local horizontal coordinates ---------------------------------
    let greenwich_sidereal_h = 6.697_096_103 + 6.570_984_737e-2 * days + decimal_hours;
    let local_sidereal = (greenwich_sidereal_h * 15.0 + location.longitude_deg).to_radians();
    let hour_angle = local_sidereal - right_ascension;
    let latitude = location.latitude_deg.to_radians();

    let zenith = (latitude.cos() * hour_angle.cos() * declination.cos()
        + declination.sin() * latitude.sin())
    .acos();
    let azimuth = (-hour_angle.sin())
        .atan2(declination.tan() * latitude.cos() - latitude.sin() * hour_angle.cos());

    // Geocentric to topocentric: an observer on the surface sees the sun a
    // little lower than the centre of the Earth would.
    let zenith = zenith + PARALLAX * zenith.sin();
    let elevation = 90.0 - zenith.to_degrees();

    SunPosition {
        elevation_deg: elevation + refraction_deg(elevation),
        azimuth_deg: azimuth.rem_euclid(TAU).to_degrees(),
    }
}

/// Atmospheric refraction, degrees to add to a true elevation.
///
/// Saemundsson's formula, the standard true-to-apparent companion of
/// Bennett's. About half a degree at the horizon, which is the sun's own
/// diameter and the difference between the sun looking set and looking not
/// quite set. It fades to nothing overhead.
///
/// It also needs a floor going the other way: the formula has a pole at -5.11
/// degrees and stops being monotone below about -2, so it cannot simply be
/// evaluated all the way down. A hard cutoff is the obvious floor and the
/// wrong one. Cutting at -1 degree would drop 0.65 degrees of correction in a
/// single step, which lands as an 8% jump in the twilight sky the moment the
/// sun crosses that line, twice a day. The correction fades out over a band
/// instead, reaching zero at -2 degrees, well clear of the pole and well below
/// the last elevation where a visible disc is being refracted.
fn refraction_deg(true_elevation_deg: f64) -> f64 {
    let fade = super::smoothstep(-2.0, -0.5, true_elevation_deg);
    if fade <= 0.0 {
        return 0.0;
    }
    let h = true_elevation_deg;
    let arcminutes = 1.02 / (h + 10.3 / (h + 5.11)).to_radians().tan();
    fade * arcminutes / 60.0
}

#[cfg(test)]
mod tests {
    use super::{sun_position, SunPosition};
    use crate::sun::testing::{day, SITE};

    /// Scan a day at one-minute resolution for the sun's highest point.
    fn peak_of_day(day_start: i64) -> (SunPosition, i64) {
        let mut best = sun_position(day_start, SITE);
        let mut at = day_start;
        for minute in 1..1_440 {
            let t = day_start + minute * 60;
            let p = sun_position(t, SITE);
            if p.elevation_deg > best.elevation_deg {
                best = p;
                at = t;
            }
        }
        (best, at)
    }

    #[test]
    fn the_solstices_bracket_the_year_at_this_latitude() {
        // Geometry, not a fit: at latitude L the noon sun reaches
        // 90 - L + 23.44 at the June solstice and 90 - L - 23.44 in December.
        // Refraction lifts both by a few hundredths of a degree.
        let summer = peak_of_day(day(171)).0.elevation_deg; // 2026-06-21
        let winter = peak_of_day(day(354)).0.elevation_deg; // 2026-12-21
        assert!(
            (summer - 61.23).abs() < 0.5,
            "June solstice noon elevation {summer:.3}, expected about 61.2"
        );
        assert!(
            (winter - 14.35).abs() < 0.5,
            "December solstice noon elevation {winter:.3}, expected about 14.3"
        );
    }

    #[test]
    fn the_sun_reaches_due_south_when_it_is_highest() {
        for d in [10, 100, 200, 300] {
            let (peak, _) = peak_of_day(day(d));
            assert!(
                (peak.azimuth_deg - 180.0).abs() < 0.5,
                "day {d}: peak azimuth {:.3}, expected due south",
                peak.azimuth_deg
            );
        }
    }

    #[test]
    fn solar_noon_tracks_the_longitude_not_the_clock() {
        // 14.12 E is 56.5 minutes of rotation east of Greenwich, so the sun
        // peaks that much before 12:00 UTC, give or take the equation of
        // time (worst case about 16 minutes either way across the year).
        let mean_offset_s = (14.12 / 15.0 * 3600.0) as i64;
        for d in [10, 100, 200, 300] {
            let (_, at) = peak_of_day(day(d));
            let noon_utc = day(d) + 12 * 3600;
            let error_s = (at - (noon_utc - mean_offset_s)).abs();
            assert!(
                error_s < 17 * 60,
                "day {d}: solar noon off by {error_s}s from the longitude prediction"
            );
        }
    }

    #[test]
    fn the_equinox_sun_rises_east_and_sets_west() {
        // 2026-03-20, the March equinox. At the equinox the sun rises due
        // east and sets due west from anywhere on Earth.
        let start = day(78);
        let mut rise = None;
        let mut set = None;
        let mut previous = sun_position(start, SITE);
        for minute in 1..1_440 {
            let p = sun_position(start + minute * 60, SITE);
            if previous.elevation_deg <= 0.5 && p.elevation_deg > 0.5 {
                rise = Some(p.azimuth_deg);
            }
            if previous.elevation_deg > 0.5 && p.elevation_deg <= 0.5 {
                set = Some(p.azimuth_deg);
            }
            previous = p;
        }
        let rise = rise.expect("the sun rose");
        let set = set.expect("the sun set");
        assert!(
            (rise - 90.0).abs() < 2.0,
            "equinox sunrise azimuth {rise:.2}"
        );
        assert!((set - 270.0).abs() < 2.0, "equinox sunset azimuth {set:.2}");
    }

    #[test]
    fn declination_stays_inside_the_obliquity_of_the_ecliptic() {
        // The sun cannot get further from the celestial equator than the
        // Earth's axial tilt. Sampled at the equator, where elevation at the
        // sun's highest point is 90 minus the declination, so this reads the
        // obliquity coefficient directly.
        let equator = bess_core::config::SiteLocation {
            latitude_deg: 0.0,
            longitude_deg: 0.0,
        };
        let mut extreme: f64 = 0.0;
        for d in 0..365 {
            let day_start = day(d);
            let mut peak = f64::NEG_INFINITY;
            for minute in 0..1_440 {
                let p = sun_position(day_start + minute * 60, equator);
                peak = peak.max(p.elevation_deg);
            }
            extreme = extreme.max((90.0 - peak).abs());
        }
        assert!(
            (extreme - 23.44).abs() < 0.2,
            "peak declination magnitude {extreme:.3}, expected the 23.44 obliquity"
        );
    }
}
