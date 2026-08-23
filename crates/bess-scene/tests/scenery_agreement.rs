//! The two instruments that saw the same sky.
//!
//! Cloud cover and irradiance reach the scene from different places. Cover is
//! an observation in eighths; dimming is a pyranometer reading divided by a
//! clear-sky model. The scene uses them for different things on purpose, one
//! for the colour of the sky and one for the brightness of the sun, and never
//! averages them.
//!
//! That only holds up if they agree in the aggregate. If a heavily clouded
//! hour did not read dimmer than a clear one, either the okta series is being
//! misread, the clear-sky model is wrong, or the solar elevation feeding it
//! is. This test would not say which, but it would say that one of them is.

use bess_core::config::SiteLocation;
use bess_scene::scenery::dimming;
use bess_scene::sun::sun_position;

/// GW-01's nominal location: DWD station 3015, Lindenberg (Mark).
const SITE: SiteLocation = SiteLocation {
    latitude_deg: 52.21,
    longitude_deg: 14.12,
};

/// 2024-01-01 00:00:00 UTC, the first hour bucket of the reference year.
const YEAR_START_UNIX_S: i64 = 1_704_067_200;

/// Only hours with the sun properly up: the ratio is meaningless below a few
/// degrees, and a low sun makes cloud effects hard to separate from geometry.
const MIN_ELEVATION_DEG: f64 = 20.0;

struct Hour {
    okta: u8,
    dimming: f32,
}

fn daylight_hours() -> Vec<Hour> {
    let year = bess_data::lindenberg_2024();
    (0..year.ghi_wm2().len())
        .filter_map(|hour| {
            let t = YEAR_START_UNIX_S + hour as i64 * 3600 + 1800;
            let elevation = sun_position(t, SITE).elevation_deg;
            if elevation < MIN_ELEVATION_DEG {
                return None;
            }
            let sample = year.hour(hour);
            Some(Hour {
                okta: sample.cloud_okta,
                dimming: dimming(sample.ghi_wm2, elevation),
            })
        })
        .collect()
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

#[test]
fn an_overcast_hour_reads_dimmer_than_a_clear_one() {
    let hours = daylight_hours();
    assert!(
        hours.len() > 1_500,
        "only {} hours had the sun above {MIN_ELEVATION_DEG} degrees",
        hours.len()
    );

    let clear: Vec<f32> = hours
        .iter()
        .filter(|h| h.okta <= 2)
        .map(|h| h.dimming)
        .collect();
    let overcast: Vec<f32> = hours
        .iter()
        .filter(|h| h.okta >= 7)
        .map(|h| h.dimming)
        .collect();
    assert!(
        clear.len() > 200 && overcast.len() > 200,
        "the year gave {} clear and {} overcast hours to compare",
        clear.len(),
        overcast.len()
    );

    let (clear_mean, overcast_mean) = (mean(&clear), mean(&overcast));
    assert!(
        clear_mean > overcast_mean + 0.2,
        "clear hours averaged {clear_mean:.3} dimming and overcast {overcast_mean:.3}; \
         two instruments that saw the same sky should not disagree this much"
    );
    assert!(
        clear_mean > 0.65,
        "a clear sky averaged only {clear_mean:.3} of its clear-sky expectation, \
         which points at the model or the elevation feeding it"
    );
}

#[test]
fn dimming_falls_as_the_sky_fills() {
    // Not just the two extremes: the whole eighth-by-eighth progression
    // should trend down. Cloud cover is a coarse human-scale observation, so
    // this asserts the trend rather than a strict ordering of neighbours.
    let hours = daylight_hours();
    let mut means = Vec::new();
    for okta in 0..=8u8 {
        let bucket: Vec<f32> = hours
            .iter()
            .filter(|h| h.okta == okta)
            .map(|h| h.dimming)
            .collect();
        if bucket.len() >= 30 {
            means.push((okta, mean(&bucket)));
        }
    }
    assert!(
        means.len() >= 6,
        "only {} okta buckets had data",
        means.len()
    );

    let first = means.first().expect("a bucket").1;
    let last = means.last().expect("a bucket").1;
    assert!(
        first > last + 0.25,
        "dimming went from {first:.3} at {} okta to {last:.3} at {} okta",
        means[0].0,
        means[means.len() - 1].0
    );
}
