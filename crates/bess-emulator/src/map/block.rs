//! Each power block's telemetry: PCS, state of charge, cell and container
//! temperatures, HVAC, and the rack spread, in both per-block ranges.
//!
//! A block carries more than one container and gets one register per
//! quantity, so every aggregate here says which container or rack it speaks
//! for, and the answer is always the one nearest a limit.

use bess_core::state::{BlockState, HvacMode, PcsOpState, SiteState};

use super::{block_base, block_ext_base, block_prefix, point, Class, Encoding, Point, Space};

/// Container air temperature reported for a block: the hottest container in
/// it. A block carries more than one container and gets one register, so the
/// register reports the one nearest a limit, which is the convention the cell
/// temperature registers already follow.
fn block_air_temp_max_c(block: &BlockState) -> f64 {
    let max = block
        .containers
        .iter()
        .map(|c| c.air_temp_c)
        .fold(f64::NEG_INFINITY, f64::max);
    if max.is_finite() {
        max
    } else {
        0.0
    }
}

/// HVAC mode reported for a block: the mode of the container drawing the most
/// electrical power, ties going to the lower index.
///
/// One register per block has to answer "what is this block's HVAC doing",
/// and the single honest answer is what its heaviest consumer is doing.
/// Ranking the enum values instead would report a block as heating while one
/// container heats and another runs both compressors.
fn block_hvac_mode(block: &BlockState) -> HvacMode {
    let mut mode = HvacMode::Off;
    let mut heaviest_w = f64::NEG_INFINITY;
    for container in &block.containers {
        if container.hvac.electrical_w > heaviest_w {
            heaviest_w = container.hvac.electrical_w;
            mode = container.hvac.mode;
        }
    }
    mode
}

/// Register encoding of an HVAC mode. Published values, so they are fixed:
/// changing one is a major map change per COMPATIBILITY.md.
fn hvac_mode_code(mode: HvacMode) -> f64 {
    match mode {
        HvacMode::Off => 0.0,
        HvacMode::Cool1 => 1.0,
        HvacMode::Cool2 => 2.0,
        HvacMode::Heat => 3.0,
    }
}

/// Widest cell voltage spread among the block's racks, V.
fn block_cell_dv_max_v(block: &BlockState) -> f64 {
    block
        .containers
        .iter()
        .flat_map(|c| c.racks.iter())
        .map(|r| r.cell_dv_v)
        .fold(0.0, f64::max)
}

/// Block `b`'s telemetry points. Its alarm points live in `alarms`.
#[allow(clippy::too_many_lines)]
pub(super) fn points(b: usize) -> Vec<Point> {
    use Class::{Fast, Medium};
    use Encoding::{I16, U16};
    use Space::Input;

    let base = block_base(b);
    let ext = block_ext_base(b);
    let prefix = block_prefix(b);
    vec![
        point!(
            format!("{prefix}.pcs.p_ac_kw"),
            "kW",
            Fast,
            I16,
            0.001,
            base,
            Input,
            move |s: &SiteState| s.blocks[b].pcs.p_ac_w
        ),
        point!(
            format!("{prefix}.pcs.p_dc_kw"),
            "kW",
            Fast,
            I16,
            0.001,
            base + 1,
            Input,
            move |s: &SiteState| s.blocks[b].pcs.p_dc_w
        ),
        point!(
            format!("{prefix}.pcs.state"),
            "enum",
            Fast,
            U16,
            1.0,
            base + 2,
            Input,
            move |s: &SiteState| match s.blocks[b].pcs.op_state {
                PcsOpState::Standby => 0.0,
                PcsOpState::Run => 1.0,
                PcsOpState::Fault => 2.0,
            }
        ),
        point!(
            format!("{prefix}.soc_pct"),
            "%",
            Medium,
            U16,
            100.0,
            base + 3,
            Input,
            move |s: &SiteState| s.blocks[b].average_soc() * 100.0
        ),
        point!(
            format!("{prefix}.cell_temp_min_c"),
            "degC",
            Medium,
            I16,
            10.0,
            base + 4,
            Input,
            move |s: &SiteState| s.blocks[b].cell_temp_min_max_c().0
        ),
        point!(
            format!("{prefix}.cell_temp_max_c"),
            "degC",
            Medium,
            I16,
            10.0,
            base + 5,
            Input,
            move |s: &SiteState| s.blocks[b].cell_temp_min_max_c().1
        ),
        point!(
            format!("{prefix}.pcs.efficiency_pct"),
            "%",
            Fast,
            U16,
            100.0,
            base + 7,
            Input,
            move |s: &SiteState| {
                let pcs = &s.blocks[b].pcs;
                // Output over input for the current flow direction; 0 when
                // idle (a converter that converts nothing has no efficiency
                // to report).
                if pcs.p_dc_w > f64::EPSILON && pcs.p_ac_w > 0.0 {
                    100.0 * pcs.p_ac_w / pcs.p_dc_w
                } else if pcs.p_dc_w < -f64::EPSILON && pcs.p_ac_w < 0.0 {
                    100.0 * pcs.p_dc_w / pcs.p_ac_w
                } else {
                    0.0
                }
            }
        ),
        point!(
            format!("{prefix}.container.air_temp_c"),
            "degC",
            Medium,
            I16,
            10.0,
            base + 8,
            Input,
            move |s: &SiteState| block_air_temp_max_c(&s.blocks[b])
        ),
        point!(
            format!("{prefix}.hvac.state"),
            "enum",
            Medium,
            U16,
            1.0,
            base + 9,
            Input,
            move |s: &SiteState| hvac_mode_code(block_hvac_mode(&s.blocks[b]))
        ),
        // The spread, in millivolts so a u16 covers it at 1 mV. The state
        // keeps volts like every other voltage in the tree; the unit change
        // is the projection's job.
        point!(
            format!("{prefix}.cell_dv_mv"),
            "mV",
            Medium,
            U16,
            1000.0,
            ext + 2,
            Input,
            move |s: &SiteState| block_cell_dv_max_v(&s.blocks[b])
        ),
    ]
}

#[cfg(test)]
mod tests {
    use bess_core::config::PlantConfig;

    use super::super::{build_points, input_bank};
    use super::*;

    /// The block HVAC register reports the busiest container, not the highest
    /// enum value: a block with one container heating and another running
    /// both compressors is a cooling block.
    #[test]
    fn the_block_hvac_register_reports_the_heaviest_container() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let mut state = SiteState::new(&cfg, 1, 0);
        {
            let containers = &mut state.blocks[0].containers;
            containers[0].hvac.mode = HvacMode::Heat;
            containers[0].hvac.electrical_w = 20.0e3;
            containers[1].hvac.mode = HvacMode::Cool2;
            containers[1].hvac.electrical_w = 38.0e3;
        }
        let input = input_bank(&points, &state);
        assert_eq!(input[1009], 2, "cooling block");
        // Block 1 was not touched and has to say so, or every block is
        // reading the same block's state.
        assert_eq!(input[1019], 0, "an untouched block reports its own HVAC");

        // Same two modes, opposite draws: now the heater is the heaviest
        // consumer and the register follows it.
        state.blocks[0].containers[1].hvac.electrical_w = 4.0e3;
        assert_eq!(input_bank(&points, &state)[1009], 3, "heating block");
    }

    /// Blocks carry two containers and one air temperature register, which
    /// reports the hotter of them.
    #[test]
    fn the_block_air_register_reports_the_hottest_container() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let mut state = SiteState::new(&cfg, 1, 0);
        state.blocks[0].containers[0].air_temp_c = 24.4;
        state.blocks[0].containers[1].air_temp_c = 31.2;
        let input = input_bank(&points, &state);
        // i16 at scale 10.
        assert_eq!(input[1008] as i16, 312);
        // And block 1, untouched at its 20.0 C initial value, reports itself
        // rather than block 0.
        assert_eq!(
            input[1018] as i16, 200,
            "an untouched block reports its own air"
        );
    }

    /// The spread register reports the block's widest rack in millivolts,
    /// from the second range, and each block its own.
    #[test]
    fn the_spread_register_reports_the_widest_rack_in_millivolts() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let mut state = SiteState::new(&cfg, 1, 0);
        let racks = &mut state.blocks[3].containers[1].racks;
        racks[4].cell_dv_v = 0.0314;
        racks[5].cell_dv_v = 0.0127;
        let input = input_bank(&points, &state);
        assert_eq!(input[2032], 31, "31.4 mV rounds to the 1 mV LSB");
        assert_eq!(input[2022], 0, "block 2 reports its own racks");
    }
}
