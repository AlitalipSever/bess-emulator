//! What the sun's position does to the light. Art direction, and it says so.
//!
//! Everything here is a choice about a picture rather than a claim about the
//! world. The one discipline it keeps is that each choice is driven by the
//! real elevation from [`super::position`] rather than by a curve someone
//! liked: the seasons differ because the sun differs, not because a constant
//! was tuned until winter looked wintry.

use bess_core::config::SiteLocation;

use super::{position::sun_position, smoothstep};

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
pub fn sun_at(unix_time_s: i64, location: SiteLocation) -> SunLight {
    let pos = sun_position(unix_time_s, location);
    let elevation = pos.elevation_deg;

    let daylight = elevation.to_radians().sin().max(0.0) as f32;
    let sky = smoothstep(-6.0, 6.0, elevation) as f32;

    // Direction from the site toward the sun, in the world axes fixed in the
    // parent module.
    let el = (elevation as f32).to_radians();
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
    let warm = smoothstep(0.0, 22.0, elevation) as f32;
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
        // The handover to moonlight happens exactly at the horizon, where the
        // day term is zero by construction (`lit` is the square root of a
        // daylight that has just reached 0). Switching any higher, as this
        // did at half a degree, swings the shadows while the sun is still
        // lighting the site at about a tenth of its intensity, and the eye
        // catches it.
        dir: if elevation > 0.0 { day_dir } else { moon_dir },
        color,
        daylight,
        sky,
    }
}

#[cfg(test)]
mod tests {
    use super::sun_at;
    use crate::clock::format_utc;
    use crate::sun::position::sun_position;
    use crate::sun::testing::{day, NEW_YEAR, SITE};

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
    fn nothing_the_eye_integrates_moves_in_steps() {
        // Every other test here samples: a solstice noon, a daily peak, an
        // hour bucket. A discontinuity lives between samples and passes all
        // of them, which is exactly how a 0.65 degree refraction cliff got
        // through review. So walk a whole day at ten seconds and hold each
        // quantity to a bound on how far it may move in one step.
        //
        // The sun climbs at most 15 degrees an hour, so 10 seconds is at
        // most 0.042 degrees of true elevation. The generous ceilings below
        // leave room for the refraction ramp steepening near the horizon
        // while still failing on any real cliff by two orders of magnitude.
        for start in [day(10), day(80), day(171), day(354)] {
            let mut previous = sun_at(start, SITE);
            let mut previous_pos = sun_position(start, SITE);
            for step in 1..=8_640 {
                let t = start + step * 10;
                let pos = sun_position(t, SITE);
                let light = sun_at(t, SITE);

                let d_elevation = (pos.elevation_deg - previous_pos.elevation_deg).abs();
                assert!(
                    d_elevation < 0.1,
                    "elevation stepped {d_elevation:.4} deg at {}",
                    format_utc(t)
                );
                let d_sky = (light.sky - previous.sky).abs();
                assert!(d_sky < 0.01, "sky stepped {d_sky:.4} at {}", format_utc(t));
                let d_daylight = (light.daylight - previous.daylight).abs();
                assert!(
                    d_daylight < 0.01,
                    "daylight stepped {d_daylight:.4} at {}",
                    format_utc(t)
                );

                previous = light;
                previous_pos = pos;
            }
        }
    }

    #[test]
    fn the_light_does_not_swing_while_the_sun_is_still_up() {
        // The direction hands over from sun to moon in one step, which is
        // fine only if the sun has nothing left to cast by then. So: whenever
        // the direction swings, the day contribution on both sides of the
        // swing has to be gone.
        //
        // Not gone to zero, gone to the resolution of the walk. Ten seconds
        // is at most 0.042 degrees of elevation, so the last sample before
        // the handover can still read sin(0.042 deg) = 7.3e-4 of daylight and
        // be correct. Handing over half a degree up, as this did before
        // review, reads 8.7e-3, which is twelve times larger; the bound sits
        // between the two rather than at a round number.
        const RESIDUAL_DAYLIGHT: f32 = 2.0e-3;
        for start in [day(10), day(171)] {
            let mut previous = sun_at(start, SITE);
            for step in 1..=8_640 {
                let light = sun_at(start + step * 10, SITE);
                let swing = (0..3)
                    .map(|i| (light.dir[i] - previous.dir[i]).abs())
                    .fold(0.0f32, f32::max);
                if swing > 0.05 {
                    let left_behind = light.daylight.max(previous.daylight);
                    assert!(
                        left_behind < RESIDUAL_DAYLIGHT,
                        "direction swung {swing:.3} with {left_behind:.5} of daylight \
                         still on the site at {}",
                        format_utc(start + step * 10)
                    );
                }
                previous = light;
            }
        }
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
}
