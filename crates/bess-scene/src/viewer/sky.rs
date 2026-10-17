//! The observed hour, in the terms the scene draws.
//!
//! The only place the two halves meet: `bess-scene` names no `bess-data`
//! type, and `bess-data` has never heard of a scene. The viewer owns both,
//! so the translation is its job, and it is its own file because it is a
//! claim about the dataset rather than about the viewer's loop.

use bess_models::{HourSample, PrecipForm};

use crate::scenery::{Observed, Precip};

/// One observed hour, in the terms the scene uses.
///
/// A free function with a test rather than six lines inside a method that
/// needs a GL context to reach. Six field-to-field assignments is exactly the
/// shape that silently swaps two of them, and the register map's review found
/// that same defect the same way.
pub fn observed_from(sample: &HourSample) -> Observed {
    Observed {
        cloud_okta: sample.cloud_okta,
        irradiance_wm2: sample.ghi_wm2,
        precip_mm_h: sample.precip_mm,
        precip: precip_of(sample.precip_form, sample.temp_c),
        wind_ms: sample.wind_ms,
        wind_dir_deg: sample.wind_dir_deg,
    }
}

/// The dataset's precipitation code, as the scene draws it.
///
/// One judgement call: DWD reports a form of "unknown" for hours where
/// something fell and nobody classified it. Rather than drop those hours or
/// guess a form, they follow the temperature, which is the same thing an
/// observer would have done.
pub fn precip_of(form: PrecipForm, temp_c: f32) -> Precip {
    match form {
        PrecipForm::NoPrecip => Precip::None,
        PrecipForm::Rain => Precip::Rain,
        PrecipForm::Snow => Precip::Snow,
        PrecipForm::Mixed => Precip::Sleet,
        PrecipForm::Unknown => {
            if temp_c <= 0.5 {
                Precip::Snow
            } else if temp_c <= 2.5 {
                Precip::Sleet
            } else {
                Precip::Rain
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{observed_from, precip_of};
    use crate::scenery::Precip;
    use bess_models::{HourSample, PrecipForm};

    /// Every field a different value, so a swapped pair cannot hide behind a
    /// coincidence. The precipitation form is the one field that is derived
    /// rather than copied, and it is pinned separately below.
    fn distinct() -> HourSample {
        HourSample {
            temp_c: 11.0,
            rel_humidity_pct: 22.0,
            ghi_wm2: 333.0,
            precip_mm: 4.4,
            precip_form: PrecipForm::Rain,
            wind_ms: 5.5,
            wind_dir_deg: 66.0,
            cloud_okta: 7,
        }
    }

    #[test]
    fn every_observed_field_comes_from_its_own_measurement() {
        let o = observed_from(&distinct());
        assert_eq!(o.cloud_okta, 7);
        assert!((o.irradiance_wm2 - 333.0).abs() < f32::EPSILON);
        assert!((o.precip_mm_h - 4.4).abs() < f32::EPSILON);
        assert!((o.wind_ms - 5.5).abs() < f32::EPSILON);
        assert!((o.wind_dir_deg - 66.0).abs() < f32::EPSILON);
        assert_eq!(o.precip, Precip::Rain);
        // Humidity has no consumer in the scene and must not have quietly
        // acquired one by landing in a field that belongs to something else.
        let mut humid = distinct();
        humid.rel_humidity_pct = 99.0;
        assert_eq!(observed_from(&humid), o);
    }

    #[test]
    fn an_unclassified_hour_follows_the_temperature() {
        let unknown = |temp_c| precip_of(PrecipForm::Unknown, temp_c);
        assert_eq!(unknown(-4.0), Precip::Snow);
        assert_eq!(unknown(1.5), Precip::Sleet);
        assert_eq!(unknown(9.0), Precip::Rain);
        // A classified hour is never second-guessed, however cold it is.
        assert_eq!(precip_of(PrecipForm::Rain, -10.0), Precip::Rain);
        assert_eq!(precip_of(PrecipForm::Snow, 20.0), Precip::Snow);
        assert_eq!(precip_of(PrecipForm::Mixed, 20.0), Precip::Sleet);
        assert_eq!(precip_of(PrecipForm::NoPrecip, 5.0), Precip::None);
    }
}
