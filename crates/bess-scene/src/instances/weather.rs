//! What the sky is doing this frame: precipitation, drifting on the wind.
//!
//! Particles are instances like everything else. The renderer is the only
//! module with GL calls and the only one allowed `unsafe`, so widening it
//! with a second pipeline for decoration would be a bad trade.
//!
//! They also carry no state. A particle's position is a pure function of its
//! index, the frame clock and the observed weather, which keeps the scene a
//! pure function of `(state, scenery, time)` and means nothing has to be
//! seeded, stepped or reset when the viewer jumps to another date.

use crate::layout::SiteLayout;
use crate::scenery::{Precip, Scenery};

use super::push;

/// Particles at the heaviest rate drawn. Sized so the field reads as weather
/// rather than as a texture, and so the instance buffer stays in the same
/// order of magnitude as the site itself.
pub const MAX_PARTICLES: usize = 2_400;

/// Rate, in millimetres per hour, at which the field reaches [`MAX_PARTICLES`].
/// Heavier rain than this looks the same; the DWD year's wettest hour at
/// Lindenberg is well under it.
const SATURATING_RATE_MM_H: f32 = 8.0;

/// How high above the ground particles are spawned, m.
const CEILING_M: f32 = 34.0;

/// Terminal-ish fall speeds, m/s. Rain falls fast and reads as a streak; snow
/// drifts and reads as a fleck.
const RAIN_FALL_MS: f32 = 8.0;
const SNOW_FALL_MS: f32 = 1.1;

/// Fraction of the measured wind the field is actually carried by.
///
/// The one number in this file that is neither measured nor derived, so it is
/// named rather than buried. A drop reaches close to the horizontal wind
/// speed, and honouring that would sweep the field clean off the plant: an
/// 8 m/s wind over a 34 m fall at 8 m/s is 34 m of drift, further than the
/// site is wide. This trades physical drift for keeping the weather over the
/// thing the scene exists to show.
const WIND_CARRY: f32 = 0.35;

/// How many particles a rate draws.
pub fn particle_count(rate_mm_h: f32) -> usize {
    if rate_mm_h <= 0.0 {
        return 0;
    }
    // Square root, not linear: perceived density of a falling field grows
    // much slower than the rate, and a linear map spends the whole budget on
    // the first millimetre.
    let t = (rate_mm_h / SATURATING_RATE_MM_H).clamp(0.0, 1.0).sqrt();
    (t * MAX_PARTICLES as f32) as usize
}

/// Append the precipitation field for one frame.
pub fn build_precipitation(
    out: &mut Vec<f32>,
    scenery: &Scenery,
    layout: &SiteLayout,
    anim_s: f32,
) {
    let count = particle_count(scenery.precip_mm_h);
    if count == 0 || scenery.precip == Precip::None {
        return;
    }

    // Shape carries the difference more than colour does: rain is a streak
    // stretched along its fall, snow is a fleck that is nearly a point.
    let (size, fall_ms, color, emissive) = match scenery.precip {
        Precip::Snow => ([0.10, 0.10, 0.10], SNOW_FALL_MS, [0.92, 0.94, 0.98], 0.35),
        // Sleet falls like rain and reads like snow, which is what sleet is.
        Precip::Sleet => (
            [0.05, 0.42, 0.05],
            RAIN_FALL_MS * 0.8,
            [0.80, 0.86, 0.92],
            0.25,
        ),
        _ => ([0.035, 0.75, 0.035], RAIN_FALL_MS, [0.62, 0.72, 0.85], 0.18),
    };

    let (min_x, min_z, max_x, max_z) = layout.fence;
    let span_x = (max_x - min_x) + 24.0;
    let span_z = (max_z - min_z) + 24.0;
    let base_x = min_x - 12.0;
    let base_z = min_z - 12.0;

    // Wind pushes the column over as it falls: a particle that has fallen
    // half the ceiling has drifted half as far as one about to land.
    //
    // A direction of exactly zero is the dataset's way of saying calm or
    // undetermined, not north, so it carries nothing. 360 is north and needs
    // no special case, the trigonometry already agrees.
    let carried = if scenery.wind_dir_deg == 0.0 {
        0.0
    } else {
        scenery.wind_ms * WIND_CARRY
    };
    let drift_rad = scenery.wind_dir_deg.to_radians();
    let (drift_x, drift_z) = (-carried * drift_rad.sin(), -carried * drift_rad.cos());

    for i in 0..count {
        let (rx, rz, phase) = scatter(i);
        let cycle = CEILING_M / fall_ms;
        let fallen = ((anim_s / cycle + phase).fract()) * CEILING_M;
        let y = CEILING_M - fallen;
        let x = base_x + rx * span_x + drift_x * fallen / fall_ms;
        let z = base_z + rz * span_z + drift_z * fallen / fall_ms;
        push(out, [x, y, z], size, color, emissive);
    }
}

/// Three stable pseudo-random numbers in 0..1 from a particle index.
///
/// An integer hash rather than a generator: the field has to look the same
/// every frame for a given index, and it has to cost nothing to produce the
/// ten-thousandth particle without producing the nine-thousandth first.
fn scatter(index: usize) -> (f32, f32, f32) {
    let mut h = index as u32;
    let mut next = move || {
        h = h.wrapping_mul(0x9E37_79B9).wrapping_add(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^= h >> 13;
        (h >> 8) as f32 / 16_777_216.0
    };
    (next(), next(), next())
}

#[cfg(test)]
mod tests {
    use super::{build_precipitation, particle_count, scatter, MAX_PARTICLES};
    use crate::layout::SiteLayout;
    use crate::scenery::{Precip, Scenery};
    use bess_core::PlantConfig;

    fn falling(precip: Precip, rate: f32) -> Scenery {
        Scenery {
            precip,
            precip_mm_h: rate,
            wind_ms: 4.0,
            wind_dir_deg: 250.0,
            ..Scenery::clear()
        }
    }

    #[test]
    fn the_field_answers_to_the_measured_rate() {
        assert_eq!(particle_count(0.0), 0);
        assert_eq!(particle_count(-1.0), 0);
        let mut previous = 0;
        for tenths in 1..=200 {
            let count = particle_count(tenths as f32 * 0.1);
            assert!(count >= previous, "density fell as the rate rose");
            assert!(count <= MAX_PARTICLES);
            previous = count;
        }
        // Drizzle is visible and a downpour is not the whole screen.
        assert!(particle_count(0.2) > 100);
        assert_eq!(particle_count(20.0), MAX_PARTICLES);
    }

    #[test]
    fn a_dry_sky_draws_nothing() {
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let mut out = Vec::new();
        build_precipitation(&mut out, &Scenery::clear(), &layout, 3.0);
        assert!(out.is_empty());
        // A form with no rate, and a rate with no form, are both nothing.
        build_precipitation(&mut out, &falling(Precip::Rain, 0.0), &layout, 3.0);
        build_precipitation(&mut out, &falling(Precip::None, 5.0), &layout, 3.0);
        assert!(out.is_empty());
    }

    #[test]
    fn particles_stay_over_the_site_and_above_the_ground() {
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let (min_x, min_z, max_x, max_z) = layout.fence;
        let scenery = falling(Precip::Rain, 6.0);
        for frame in 0..40 {
            let mut out = Vec::new();
            build_precipitation(&mut out, &scenery, &layout, frame as f32 * 0.37);
            assert!(!out.is_empty());
            let (instances, rest) = out.as_chunks::<{ super::super::FPI }>();
            assert!(rest.is_empty(), "a partial instance was emitted");
            for chunk in instances {
                let (x, y, z) = (chunk[0], chunk[1], chunk[2]);
                assert!((-0.1..=35.0).contains(&y), "particle at y = {y}");
                // Wind carries the column off the fence line; the bound is
                // the site plus the widest drift the dataset's wind allows.
                assert!(x > min_x - 60.0 && x < max_x + 60.0, "particle at x = {x}");
                assert!(z > min_z - 60.0 && z < max_z + 60.0, "particle at z = {z}");
                assert!(chunk.iter().all(|v| v.is_finite()));
            }
        }
    }

    #[test]
    fn a_calm_hour_does_not_drift_north() {
        // The dataset spends 0 on "calm or undetermined", not on north.
        // Lindenberg 2024 never uses it, so this guards another station or
        // another year rather than a picture anyone has seen.
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let mut undetermined = falling(Precip::Rain, 4.0);
        undetermined.wind_dir_deg = 0.0;
        undetermined.wind_ms = 9.0;
        let mut still = undetermined;
        still.wind_ms = 0.0;

        let (mut drifting, mut calm) = (Vec::new(), Vec::new());
        build_precipitation(&mut drifting, &undetermined, &layout, 5.0);
        build_precipitation(&mut calm, &still, &layout, 5.0);
        assert_eq!(
            drifting, calm,
            "an undetermined direction carried the field somewhere"
        );

        // And a real direction still does move it.
        let mut northerly = undetermined;
        northerly.wind_dir_deg = 360.0;
        let mut moved = Vec::new();
        build_precipitation(&mut moved, &northerly, &layout, 5.0);
        assert_ne!(moved, calm, "a 9 m/s wind moved nothing");
    }

    #[test]
    fn the_field_is_the_same_field_every_time_it_is_asked() {
        // Particles carry no state, so a jump to another date and back has to
        // produce the identical frame. This is what buys the scene its purity.
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let scenery = falling(Precip::Snow, 2.0);
        let mut first = Vec::new();
        let mut again = Vec::new();
        build_precipitation(&mut first, &scenery, &layout, 12.25);
        build_precipitation(&mut again, &scenery, &layout, 12.25);
        assert_eq!(first, again);
    }

    #[test]
    fn the_scatter_covers_its_range_without_clumping() {
        // Three coordinates per particle, each expected to fill 0..1. A hash
        // that returned the same number three times, or that lived in a
        // corner of the range, would still produce a plausible-looking field
        // in a screenshot and a wrong one everywhere else.
        let mut buckets = [[0usize; 10]; 3];
        for i in 0..MAX_PARTICLES {
            let (a, b, c) = scatter(i);
            for (axis, v) in [a, b, c].into_iter().enumerate() {
                assert!((0.0..1.0).contains(&v), "scatter gave {v}");
                buckets[axis][(v * 10.0) as usize] += 1;
            }
        }
        let floor = MAX_PARTICLES / 20;
        for (axis, counts) in buckets.iter().enumerate() {
            for (tenth, &n) in counts.iter().enumerate() {
                assert!(n > floor, "axis {axis} tenth {tenth} held only {n}");
            }
        }
    }
}
