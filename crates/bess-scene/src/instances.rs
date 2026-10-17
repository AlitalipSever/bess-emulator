//! Cube-instance builders: the state tree in, flat `f32` instance data out.
//! Pure functions; the renderer only uploads what these emit.
//!
//! Instance layout (see [`FPI`]): offset(3) scale(3) color(3) emissive(1).
//! One instance format and one pipeline, which is why weather particles are
//! cubes like everything else rather than a second way to draw.
//!
//! - [`site`]    what the plant is made of, built once from the layout
//! - [`live`]    what it is doing this frame, from the state tree
//! - [`status`]  the alarm indications, from the three alarm words
//! - [`weather`] what the sky is doing this frame, from the observations

pub mod live;
pub mod site;
pub mod status;
pub mod weather;

pub use live::{build_dynamic, DynamicInput};
pub use site::{build_ground, build_static};

/// Floats per instance.
pub const FPI: usize = 10;

/// Append one cuboid instance.
pub fn push(out: &mut Vec<f32>, offset: [f32; 3], scale: [f32; 3], color: [f32; 3], em: f32) {
    out.extend_from_slice(&offset);
    out.extend_from_slice(&scale);
    out.extend_from_slice(&color);
    out.push(em);
}

#[cfg(test)]
mod tests {
    //! The one invariant every builder shares: whatever a builder emits,
    //! it emits whole instances of finite numbers. Lives in the parent
    //! because it is a statement about the format rather than about any
    //! one of them.

    use bess_core::config::PlantConfig;
    use bess_core::state::SiteState;

    use super::live::{build_dynamic, DynamicInput};
    use super::site::{build_ground, build_static};
    use super::FPI;
    use crate::layout::{Selection, SiteLayout};
    use crate::style::{Palette, Style};

    #[test]
    fn instance_streams_are_wellformed() {
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let style = Style::default();
        let ground = build_ground(&layout, &style);
        let stat = build_static(&layout, &style);
        assert_eq!(ground.len() % FPI, 0);
        assert_eq!(stat.len() % FPI, 0);
        assert!(stat.len() / FPI > 500, "site should be furnished");

        let state = SiteState::new(&cfg, 1, 1_767_225_600);
        let mut dynamic = Vec::new();
        build_dynamic(
            &mut dynamic,
            &DynamicInput {
                state: &state,
                layout: &layout,
                style: &style,
                palette: &Palette::default(),
                nightness: 1.0,
                anim_s: 3.2,
                selection: Some(Selection::Container {
                    block: 3,
                    container: 1,
                }),
            },
        );
        assert_eq!(dynamic.len() % FPI, 0);
        // 40 gauges + 40 LEDs + masts + selection frame at minimum.
        assert!(dynamic.len() / FPI > 90);
        for chunk in dynamic.chunks(FPI) {
            assert!(chunk.iter().all(|v| v.is_finite()));
        }
    }

    /// Instances lit in `color`: the alarm indications are the only ones
    /// that take the alarm tones, so counting them counts indications.
    fn lit_in(state: &SiteState, color: [f32; 3]) -> usize {
        let cfg = PlantConfig::gw01();
        let layout = SiteLayout::new(&cfg);
        let mut dynamic = Vec::new();
        build_dynamic(
            &mut dynamic,
            &DynamicInput {
                state,
                layout: &layout,
                style: &Style::default(),
                palette: &Palette::default(),
                nightness: 0.0,
                anim_s: 1.0,
                selection: None,
            },
        );
        dynamic.chunks(FPI).filter(|c| c[6..9] == color).count()
    }

    #[test]
    fn each_indication_takes_the_tone_of_its_word() {
        use bess_core::alarms::layout::{block, rack, site};
        let palette = Palette::default();
        let mut state = SiteState::new(&PlantConfig::gw01(), 1, 1_767_225_600);
        assert_eq!(lit_in(&state, palette.warning), 0);
        assert_eq!(lit_in(&state, palette.alarm), 0);

        // A warning on one rack lights its container's diode amber.
        state.blocks[4].containers[1].racks[3].alarm_bits = 1 << rack::DERATE_ACTIVE;
        assert_eq!(lit_in(&state, palette.warning), 1);
        // A trip on another rack of the same container turns it red.
        state.blocks[4].containers[1].racks[7].alarm_bits = 1 << rack::OVER_TEMP_TRIP;
        assert_eq!(
            (
                lit_in(&state, palette.warning),
                lit_in(&state, palette.alarm)
            ),
            (0, 1)
        );

        // A PCS in fault lights its skid red; a failed HVAC unit carries an
        // amber mark; the site word lights the transformer beacon.
        state.blocks[9].alarm_bits = 1 << block::PCS_FAULT;
        assert_eq!(lit_in(&state, palette.alarm), 2);
        state.blocks[9].containers[0].hvac.failed = true;
        assert_eq!(lit_in(&state, palette.warning), 1);
        state.alarm_bits = 1 << site::POWER_LIMITED;
        assert_eq!(lit_in(&state, palette.warning), 2);

        // Maintenance is a status, not an alarm: muted.
        state.blocks[12].alarm_bits = 1 << block::MAINTENANCE;
        assert_eq!(lit_in(&state, palette.out_of_service), 1);
    }
}
