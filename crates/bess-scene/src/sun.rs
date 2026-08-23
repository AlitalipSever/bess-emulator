//! Where the sun is, and what that does to the light.
//!
//! Two different things live in this module and it is worth keeping them
//! apart. Sun position is astronomy: a pure function of UTC and the site's
//! coordinates, and it is the reason a December replay keeps the sun low all
//! day while a June replay lifts it overhead. Everything after that, colour
//! and intensity and how the sky reads at dusk, is art direction, and the
//! functions that do it say so.
//!
//! The line the scene does not cross is unchanged: the kernel never reads any
//! of this, and the only authority for radiation in the energy accounting is
//! the measured irradiance series.
//!
//! **World axes.** The site layout already fixes them: `layout.rs` puts row 0
//! south of the road at negative z and row 1 north of it at positive z. So
//! +z is north, +y is up, and +x is east. A northern-hemisphere sun therefore
//! crosses the sky on the negative-z side, which is what the site looks like
//! from the overview camera.

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
/// quite set. It fades to nothing overhead and is not applied well below the
/// horizon, where it stops meaning anything.
fn refraction_deg(true_elevation_deg: f64) -> f64 {
    if true_elevation_deg < -1.0 {
        return 0.0;
    }
    let h = true_elevation_deg;
    let arcminutes = 1.02 / (h + 10.3 / (h + 5.11)).to_radians().tan();
    arcminutes / 60.0
}

/// Lighting for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunLight {
    /// Normalized direction the light travels (FROM the light).
    pub dir: [f32; 3],
    /// Light color premultiplied by intensity.
    pub color: [f32; 3],
    /// Geometric daylight factor: the sine of the sun's elevation, zero when
    /// it is down. This is the same cosine-of-zenith that sets how much of a
    /// beam a horizontal surface catches, so a January noon here reads around
    /// 0.25 while a July noon reads around 0.88. The site never sees a
    /// tropical noon and the scene should not pretend otherwise.
    pub daylight: f32,
    /// Sky blend factor: 0 at the end of civil twilight, 1 once the sun is
    /// well up, smooth between. Art direction, so that dusk fades instead of
    /// snapping to black the moment the sun sets.
    pub sky: f32,
}

/// Sun direction, colour and daylight for a UTC instant at a site.
///
/// Position is astronomy (see [`sun_position`]); everything below the first
/// two lines is art direction.
pub fn sun_at(unix_time_s: i64, location: SiteLocation) -> SunLight {
    let pos = sun_position(unix_time_s, location);
    let elevation = pos.elevation_deg as f32;

    let daylight = elevation.to_radians().sin().max(0.0);
    let sky = smoothstep(-6.0, 6.0, elevation);

    // Direction from the site toward the sun, in the world axes fixed above.
    let el = elevation.to_radians();
    let az = (pos.azimuth_deg as f32).to_radians();
    let horizontal = el.cos();
    let toward_sun = [az.sin() * horizontal, el.sin(), az.cos() * horizontal];
    let len = (toward_sun[0] * toward_sun[0]
        + toward_sun[1] * toward_sun[1]
        + toward_sun[2] * toward_sun[2])
        .sqrt()
        .max(1e-6);
    let day_dir = [
        -toward_sun[0] / len,
        -toward_sun[1] / len,
        -toward_sun[2] / len,
    ];

    // A fixed moonlight direction takes over once the sun is down, so a night
    // scene still has shape instead of flat ambient.
    let moon_dir = [-0.301_09, -0.822_98, -0.481_74]; // unit length

    // Low sun reads warm. Tied to elevation rather than to intensity, because
    // the golden hour is about the path through the atmosphere.
    let warm = smoothstep(0.0, 22.0, elevation);
    let lit = daylight.sqrt();
    let day_col = [
        1.0 * lit,
        (0.6 + 0.37 * warm) * lit,
        (0.35 + 0.55 * warm) * lit,
    ];
    let night = 1.0 - sky;
    let color = [
        day_col[0] + 0.12 * night,
        day_col[1] + 0.15 * night,
        day_col[2] + 0.22 * night,
    ];

    SunLight {
        dir: if elevation > 0.5 { day_dir } else { moon_dir },
        color,
        daylight,
        sky,
    }
}

/// Hermite ramp from 0 at `lo` to 1 at `hi`.
fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Civil date and time (UTC) of a Unix timestamp:
/// (year, month, day, hour, minute, second). Calendar conversion after
/// Howard Hinnant's algorithm.
pub fn civil_from_unix(unix_time_s: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = unix_time_s.div_euclid(86_400);
    let secs = unix_time_s.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (if m <= 2 { y + 1 } else { y }) as i32;
    (
        year,
        m as u32,
        d as u32,
        (secs / 3600) as u32,
        (secs / 60 % 60) as u32,
        (secs % 60) as u32,
    )
}

/// `YYYY-MM-DD HH:MM:SS UTC` for panel headers.
pub fn format_utc(unix_time_s: i64) -> String {
    let (y, mo, d, h, mi, s) = civil_from_unix(unix_time_s);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

#[cfg(test)]
mod tests {
    use super::{civil_from_unix, format_utc, sun_at, sun_position, SunPosition};
    use bess_core::config::SiteLocation;

    /// GW-01's nominal location, the DWD station its weather comes from.
    const SITE: SiteLocation = SiteLocation {
        latitude_deg: 52.21,
        longitude_deg: 14.12,
    };

    /// 2026-01-01 00:00:00 UTC.
    const NEW_YEAR: i64 = 1_767_225_600;

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

    /// Midnight UTC of the day `days` after New Year 2026.
    fn day(days: i64) -> i64 {
        NEW_YEAR + days * 86_400
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
        // east and sets due west from anywhere on Earth. Sampled at the
        // crossings of the true horizon rather than the refracted one.
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
        let equator = SiteLocation {
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

    #[test]
    fn night_is_dark_and_noon_is_not() {
        assert!(sun_at(NEW_YEAR, SITE).daylight < 0.01);
        assert!(sun_at(NEW_YEAR + 11 * 3600, SITE).daylight > 0.15);
        // Winter noon is real daylight but nothing like summer noon.
        let winter = sun_at(day(354) + 11 * 3600, SITE).daylight;
        let summer = sun_at(day(171) + 11 * 3600, SITE).daylight;
        assert!(
            summer > winter * 2.5,
            "summer {summer:.3} should tower over winter {winter:.3}"
        );
    }

    #[test]
    fn light_direction_is_normalized_around_the_clock() {
        for h in 0..24 {
            let s = sun_at(NEW_YEAR + h * 3600, SITE);
            let len = (s.dir[0] * s.dir[0] + s.dir[1] * s.dir[1] + s.dir[2] * s.dir[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-3, "hour {h}: |dir| = {len}");
        }
    }

    #[test]
    fn the_sun_comes_from_the_south_at_midday() {
        // World axes: +z north. A northern-hemisphere midday sun sits south,
        // so the light travels toward +z.
        let noon = sun_at(day(171) + 11 * 3600, SITE);
        assert!(
            noon.dir[2] > 0.3,
            "midday light should travel northward, got {:?}",
            noon.dir
        );
        assert!(noon.dir[1] < -0.5, "midday light should travel downward");
    }

    #[test]
    fn civil_conversion_and_formatting_match_known_timestamps() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil_from_unix(-86_400).1, 12);
        assert_eq!(format_utc(NEW_YEAR), "2026-01-01 00:00:00 UTC");
        let t = NEW_YEAR + 194 * 86_400 + 10 * 3600 + 30 * 60;
        assert_eq!(format_utc(t), "2026-07-14 10:30:00 UTC");
    }
}
