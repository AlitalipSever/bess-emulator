//! The computed sun against the measured sun.
//!
//! Every other test of the solar position checks it against geometry: the
//! solstice elevations, the equinox azimuths, where the sun is when it is
//! highest. Those are strong but they are all the same kind of evidence, and
//! a systematic error in the algorithm could satisfy every one of them.
//!
//! This one is a different kind. The site replays a year of DWD pyranometer
//! readings from the station whose coordinates the sun is computed at, so the
//! dataset already knows when it was light at Lindenberg in 2024. If the two
//! disagree about when the sun was up, one of them is wrong, and it is not
//! the pyranometer.

use bess_core::config::SiteLocation;
use bess_scene::sun::sun_position;

/// GW-01's nominal location: DWD station 3015, Lindenberg (Mark). Mirrors
/// `PlantConfig::gw01()`, and the test below asserts they have not drifted
/// apart.
const SITE: SiteLocation = SiteLocation {
    latitude_deg: 52.21,
    longitude_deg: 14.12,
};

/// 2024-01-01 00:00:00 UTC, the first hour bucket of the reference year.
const YEAR_START_UNIX_S: i64 = 1_704_067_200;

/// Irradiance above this is unambiguous daylight, well clear of both
/// instrument offset and the deepest twilight.
const DAYLIGHT_WM2: f32 = 50.0;

/// Below this elevation it is night by any definition: civil twilight ends
/// at -6 degrees.
const NIGHT_ELEVATION_DEG: f64 = -8.0;

/// Readings this small at night are instrument offset, not sunlight.
const DARK_WM2: f32 = 5.0;

#[test]
fn the_site_the_sun_is_computed_for_is_the_site_the_weather_came_from() {
    let cfg = bess_core::PlantConfig::gw01();
    assert!((cfg.location.latitude_deg - SITE.latitude_deg).abs() < 1e-9);
    assert!((cfg.location.longitude_deg - SITE.longitude_deg).abs() < 1e-9);
    let year = bess_data::lindenberg_2024();
    assert_eq!(year.station_id(), 3015);
    assert_eq!(year.year(), 2024);
}

#[test]
fn the_computed_sun_and_the_measured_irradiance_agree_about_daylight() {
    let year = bess_data::lindenberg_2024();
    let ghi = year.ghi_wm2();
    assert_eq!(ghi.len(), 8_784, "2024 is a leap year of 8784 hours");

    let mut sunlight_in_the_dark = Vec::new();
    let mut darkness_in_the_sun = Vec::new();

    for (hour, &w) in ghi.iter().enumerate() {
        // The series is an hourly mean, so it belongs to the middle of its
        // bucket. Reading it at the bucket edge would put every sunrise hour
        // half an hour before the sun.
        let t = YEAR_START_UNIX_S + hour as i64 * 3600 + 1800;
        let elevation = sun_position(t, SITE).elevation_deg;

        if w > DAYLIGHT_WM2 && elevation < 0.0 {
            sunlight_in_the_dark.push((hour, w, elevation));
        }
        if elevation < NIGHT_ELEVATION_DEG && w > DARK_WM2 {
            darkness_in_the_sun.push((hour, w, elevation));
        }
    }

    assert!(
        sunlight_in_the_dark.is_empty(),
        "{} hours measured more than {DAYLIGHT_WM2} W/m2 with the sun below the \
         horizon; first few: {:?}",
        sunlight_in_the_dark.len(),
        &sunlight_in_the_dark[..sunlight_in_the_dark.len().min(5)]
    );
    assert!(
        darkness_in_the_sun.is_empty(),
        "{} hours measured more than {DARK_WM2} W/m2 with the sun below \
         {NIGHT_ELEVATION_DEG} degrees; first few: {:?}",
        darkness_in_the_sun.len(),
        &darkness_in_the_sun[..darkness_in_the_sun.len().min(5)]
    );
}

#[test]
fn the_brightest_hours_of_the_year_are_the_ones_with_the_sun_highest() {
    // A weaker but independent statement: irradiance and elevation should be
    // strongly rank-correlated over the daylight hours. A longitude sign
    // error would keep both of the checks above happy on a symmetric day and
    // fail here, because it would put the peak on the wrong side of noon.
    let year = bess_data::lindenberg_2024();
    let mut sum_w = 0.0f64;
    let mut sum_e = 0.0f64;
    let mut sum_we = 0.0f64;
    let mut sum_ww = 0.0f64;
    let mut sum_ee = 0.0f64;
    let mut n = 0.0f64;

    for (hour, &w) in year.ghi_wm2().iter().enumerate() {
        let t = YEAR_START_UNIX_S + hour as i64 * 3600 + 1800;
        let elevation = sun_position(t, SITE).elevation_deg;
        if elevation <= 0.0 {
            continue;
        }
        let (w, e) = (f64::from(w), elevation);
        sum_w += w;
        sum_e += e;
        sum_we += w * e;
        sum_ww += w * w;
        sum_ee += e * e;
        n += 1.0;
    }

    let covariance = sum_we - sum_w * sum_e / n;
    let spread = ((sum_ww - sum_w * sum_w / n) * (sum_ee - sum_e * sum_e / n)).sqrt();
    let correlation = covariance / spread;
    assert!(
        correlation > 0.7,
        "irradiance and solar elevation correlate at only {correlation:.3} over \
         {n} daylight hours; cloud cover scatters this but should not break it"
    );
}
