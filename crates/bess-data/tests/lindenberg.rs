//! Integrity and sanity gates for the bundled Lindenberg 2024 artifact.
//!
//! The loader itself asserts the pinned hash; these tests pin the shape and
//! plausibility of the decoded year, so a regenerated artifact that drifts
//! (different fills, implausible physics) fails CI even when hash and pin
//! were moved together.

use bess_data::{lindenberg_2024, PrecipForm, HOURS_2024};

#[test]
fn loads_with_expected_shape() {
    let year = lindenberg_2024();
    assert_eq!(year.len(), HOURS_2024);
    assert!(!year.is_empty());
    assert_eq!(year.year(), 2024);
    assert_eq!(year.station_id(), 3015);
}

/// Spot checks against raw DWD rows read by eye from the source archives.
#[test]
fn first_hour_matches_the_raw_observations() {
    let hour = lindenberg_2024().hour(0); // 2024-01-01 00:00 UTC
    assert!((hour.temp_c - 5.3).abs() < 0.06);
    assert!((hour.rel_humidity_pct - 83.0).abs() < 0.06);
    assert!((hour.wind_ms - 3.3).abs() < 0.06);
    assert!((hour.wind_dir_deg - 190.0).abs() < 0.5);
    assert_eq!(hour.cloud_okta, 8);
    assert!((hour.precip_mm - 0.0).abs() < 0.06);
    assert_eq!(hour.precip_form, PrecipForm::NoPrecip);
    assert!(hour.ghi_wm2 < 0.1, "midnight irradiance must be zero");
}

#[test]
fn annual_statistics_are_plausible_for_brandenburg() {
    let year = lindenberg_2024();
    let ghi_kwh_m2: f32 = year.ghi_wm2().iter().sum::<f32>() / 1000.0;
    assert!(
        (900.0..1400.0).contains(&ghi_kwh_m2),
        "annual GHI {ghi_kwh_m2} kWh/m2 outside the plausible band"
    );
    let mean_temp: f32 = year.temp_c().iter().sum::<f32>() / year.len() as f32;
    assert!(
        (5.0..15.0).contains(&mean_temp),
        "annual mean temperature {mean_temp} C outside the plausible band"
    );
    let precip_mm: f32 = year.precip_mm().iter().sum();
    assert!(
        (300.0..1200.0).contains(&precip_mm),
        "annual precipitation {precip_mm} mm outside the plausible band"
    );
}

#[test]
fn every_hour_stays_inside_physical_bounds() {
    let year = lindenberg_2024();
    for idx in 0..year.len() {
        let hour = year.hour(idx);
        assert!((-35.0..50.0).contains(&hour.temp_c), "hour {idx} temp");
        assert!(
            (0.0..=100.0).contains(&hour.rel_humidity_pct),
            "hour {idx} rh"
        );
        assert!((0.0..1200.0).contains(&hour.ghi_wm2), "hour {idx} ghi");
        assert!((0.0..60.0).contains(&hour.precip_mm), "hour {idx} precip");
        assert!((0.0..60.0).contains(&hour.wind_ms), "hour {idx} wind");
        assert!((0.0..=360.0).contains(&hour.wind_dir_deg), "hour {idx} dir");
        assert!(hour.cloud_okta <= 8, "hour {idx} okta");
    }
}

#[test]
fn summer_noon_is_bright_and_winter_night_is_dark() {
    let year = lindenberg_2024();
    // 2024-07-01 is day-of-year 183; noon UTC bucket.
    let july_noon = (183 - 1) * 24 + 12;
    assert!(year.hour(july_noon).ghi_wm2 > 100.0);
    // 2024-01-15, 22:00 UTC.
    let january_night = (15 - 1) * 24 + 22;
    assert!(year.hour(january_night).ghi_wm2 < 0.1);
}

/// The gap-fill bookkeeping of the bundled artifact, pinned. A regenerated
/// artifact with different fills is a different dataset and must be looked
/// at, not waved through.
#[test]
fn fill_counts_match_the_recorded_compilation() {
    let fills = lindenberg_2024().fill_counts();
    assert_eq!(fills.temp, 0);
    assert_eq!(fills.rel_humidity, 0);
    assert_eq!(fills.ghi, 0);
    assert_eq!(fills.precip_mm, 2);
    assert_eq!(fills.precip_form, 2);
    assert_eq!(fills.wind_ms, 37);
    assert_eq!(fills.wind_dir, 36);
    assert_eq!(fills.cloud, 35);
}

#[test]
fn the_year_sees_rain_and_snow() {
    let forms = lindenberg_2024().precip_form();
    assert!(forms.contains(&PrecipForm::Rain));
    assert!(forms.contains(&PrecipForm::Snow));
    assert!(forms.contains(&PrecipForm::Mixed));
}
