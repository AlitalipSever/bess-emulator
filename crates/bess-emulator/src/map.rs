//! The signal map: one table drives every projection.
//!
//! Each `Point` names a signal, says how to read it from the state tree,
//! and how to encode it as Modbus registers. The Modbus banks, the MQTT
//! topics, and the published CSV reference are all generated from this one
//! table, so they cannot drift apart.
//!
//! Register conventions: 32-bit values span two registers, high word first.
//! `scale` converts physical units to register counts (register = physical
//! value x scale, rounded). Addresses are stable per COMPATIBILITY.md once
//! published: adding registers is a minor change, moving them is major.

use std::io::Write as _;
use std::path::Path;

use bess_core::config::PlantConfig;
use bess_core::state::{BlockState, BreakerState, EmsMode, HvacMode, PcsOpState, SiteState};

/// Version of the published signal map, semver, independent of the crate
/// version per COMPATIBILITY.md.
///
/// The map published through crate v0.2.0 carried no version number at all;
/// it is recorded as 0.1.0 so the sequence has a beginning. 0.2.0 is M1's
/// addition of the thermal and auxiliary points: new addresses only, nothing
/// moved, nothing renamed.
pub const MAP_VERSION: &str = "0.2.0";

/// Register space a point lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// Input registers (function code 4), read-only telemetry.
    Input,
    /// Holding registers (function codes 3/6/16), the control surface.
    Holding,
}

/// Publication class (decimation cadence in simulation time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Every simulated second.
    Fast,
    /// Every 10 simulated seconds.
    Medium,
    /// Every 60 simulated seconds.
    Slow,
}

impl Class {
    /// Minimum simulated seconds between publications.
    pub fn period_s(self) -> i64 {
        match self {
            Class::Fast => 1,
            Class::Medium => 10,
            Class::Slow => 60,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Class::Fast => "fast",
            Class::Medium => "medium",
            Class::Slow => "slow",
        }
    }
}

/// Register encoding of a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// One unsigned register.
    U16,
    /// One signed register.
    I16,
    /// Two registers, unsigned, high word first.
    U32,
    /// Two registers, signed, high word first.
    I32,
}

impl Encoding {
    /// Number of registers the encoding occupies.
    pub fn words(self) -> u16 {
        match self {
            Encoding::U16 | Encoding::I16 => 1,
            Encoding::U32 | Encoding::I32 => 2,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Encoding::U16 => "u16",
            Encoding::I16 => "i16",
            Encoding::U32 => "u32",
            Encoding::I32 => "i32",
        }
    }
}

/// One signal: where it comes from and how every surface presents it.
pub struct Point {
    /// Dotted path, e.g. `site.poi.active_power_w` (MQTT topic uses `/`).
    pub name: String,
    /// Physical unit of the extracted value.
    pub unit: &'static str,
    /// Publication class.
    pub class: Class,
    /// `true` if the point is writable (holding space control surface).
    pub writable: bool,
    /// Register encoding.
    pub encoding: Encoding,
    /// Register counts per physical unit.
    pub scale: f64,
    /// First register address inside `space`.
    pub addr: u16,
    /// Register space.
    pub space: Space,
    /// Reads the physical value from the state tree.
    pub extract: Box<dyn Fn(&SiteState) -> f64 + Send + Sync>,
}

/// Size of the input register bank (site block + 20 power blocks).
pub const INPUT_BANK_LEN: usize = BLOCK_BASE as usize + 20 * BLOCK_STRIDE as usize;
/// Size of the holding register bank.
pub const HOLDING_BANK_LEN: usize = 3;

/// First register of block `b` in the input space.
const BLOCK_BASE: u16 = 1000;
/// Register stride between blocks.
const BLOCK_STRIDE: u16 = 10;

/// Holding register: site external setpoint, W, i32 (write switches the EMS
/// to external mode).
pub const HOLDING_SETPOINT_ADDR: u16 = 0;
/// Holding register: EMS mode (0 = follow internal plan, 1 = external).
pub const HOLDING_MODE_ADDR: u16 = 2;

macro_rules! point {
    ($name:expr, $unit:expr, $class:expr, $enc:expr, $scale:expr, $addr:expr, $space:expr, $extract:expr) => {
        Point {
            name: $name.into(),
            unit: $unit,
            class: $class,
            writable: matches!($space, Space::Holding),
            encoding: $enc,
            scale: $scale,
            addr: $addr,
            space: $space,
            extract: Box::new($extract),
        }
    };
}

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

/// Build the full signal map for a plant configuration.
#[allow(clippy::too_many_lines)]
pub fn build_points(cfg: &PlantConfig) -> Vec<Point> {
    use Class::{Fast, Medium, Slow};
    use Encoding::{I16, I32, U16, U32};
    use Space::{Holding, Input};

    let mut points: Vec<Point> = vec![
        point!(
            "site.poi.active_power_w",
            "W",
            Fast,
            I32,
            1.0,
            0,
            Input,
            |s: &SiteState| s.substation.poi_active_power_w
        ),
        point!(
            "site.poi.reactive_power_var",
            "var",
            Fast,
            I32,
            1.0,
            2,
            Input,
            |s: &SiteState| s.substation.poi_reactive_power_var
        ),
        point!(
            "site.poi.voltage_kv",
            "kV",
            Fast,
            U16,
            100.0,
            4,
            Input,
            |s: &SiteState| s.substation.poi_voltage_kv
        ),
        point!(
            "site.poi.frequency_hz",
            "Hz",
            Fast,
            U16,
            1000.0,
            5,
            Input,
            |s: &SiteState| s.substation.frequency_hz
        ),
        point!(
            "site.soc_pct",
            "%",
            Medium,
            U16,
            100.0,
            6,
            Input,
            |s: &SiteState| s.average_soc() * 100.0
        ),
        point!("site.soh_pct", "%", Slow, U16, 100.0, 7, Input, |_| 100.0),
        point!(
            "site.available_discharge_kw",
            "kW",
            Fast,
            U32,
            0.001,
            8,
            Input,
            |s: &SiteState| s.ems.available_discharge_w
        ),
        point!(
            "site.available_charge_kw",
            "kW",
            Fast,
            U32,
            0.001,
            10,
            Input,
            |s: &SiteState| s.ems.available_charge_w
        ),
        point!(
            "site.meter.export_kwh",
            "kWh",
            Slow,
            U32,
            0.001,
            12,
            Input,
            |s: &SiteState| s.substation.export_wh
        ),
        point!(
            "site.meter.import_kwh",
            "kWh",
            Slow,
            U32,
            0.001,
            14,
            Input,
            |s: &SiteState| s.substation.import_wh
        ),
        point!(
            "site.substation.hv_breaker_closed",
            "bool",
            Medium,
            U16,
            1.0,
            16,
            Input,
            |s: &SiteState| f64::from(s.substation.hv_breaker == BreakerState::Closed)
        ),
        point!(
            "site.ems.mode",
            "enum",
            Medium,
            U16,
            1.0,
            17,
            Input,
            |s: &SiteState| { f64::from(s.ems.mode == EmsMode::External) }
        ),
        point!(
            "site.ems.setpoint_w",
            "W",
            Fast,
            I32,
            1.0,
            18,
            Input,
            |s: &SiteState| s.ems.site_setpoint_w
        ),
        point!(
            "site.weather.ambient_c",
            "degC",
            Medium,
            I16,
            10.0,
            20,
            Input,
            |s: &SiteState| s.weather.ambient_c
        ),
        point!(
            "site.weather.irradiance_wm2",
            "W/m2",
            Medium,
            U16,
            1.0,
            21,
            Input,
            |s: &SiteState| s.weather.irradiance_wm2
        ),
        point!(
            "site.aux_power_w",
            "W",
            Medium,
            U32,
            1.0,
            22,
            Input,
            |s: &SiteState| s.substation.aux_power_w
        ),
        point!(
            "site.transformer_loss_w",
            "W",
            Medium,
            U32,
            1.0,
            24,
            Input,
            |s: &SiteState| s.substation.transformer_loss_w
        ),
        point!(
            "site.sim.tick",
            "count",
            Fast,
            U32,
            1.0,
            26,
            Input,
            |s: &SiteState| { (s.tick % u64::from(u32::MAX)) as f64 }
        ),
        point!(
            "site.sim.unix_time_s",
            "s",
            Fast,
            U32,
            1.0,
            28,
            Input,
            |s: &SiteState| s.unix_time_s() as f64
        ),
        point!(
            "site.pcs_online_count",
            "count",
            Medium,
            U16,
            1.0,
            30,
            Input,
            |s: &SiteState| {
                s.blocks
                    .iter()
                    .filter(|b| b.pcs.op_state == PcsOpState::Run)
                    .count() as f64
            }
        ),
        point!(
            "site.alarm_count",
            "count",
            Medium,
            U16,
            1.0,
            31,
            Input,
            |s: &SiteState| {
                s.racks()
                    .map(|r| f64::from(r.alarm_bits.count_ones()))
                    .sum()
            }
        ),
        // The house load, item by item. `site.aux_power_w` above is the
        // total the substation meters; these five are what drew it, and they
        // sum to it. They start at 32 rather than beside the total, because
        // the total's address is published and moving it would be a breaking
        // change; the map trades adjacency for stability.
        point!(
            "site.aux.hvac_w",
            "W",
            Medium,
            U32,
            1.0,
            32,
            Input,
            |s: &SiteState| s.aux.hvac_w
        ),
        point!(
            "site.aux.bms_w",
            "W",
            Medium,
            U32,
            1.0,
            34,
            Input,
            |s: &SiteState| s.aux.bms_w
        ),
        point!(
            "site.aux.pcs_standby_w",
            "W",
            Medium,
            U32,
            1.0,
            36,
            Input,
            |s: &SiteState| s.aux.pcs_standby_w
        ),
        point!(
            "site.aux.controls_w",
            "W",
            Medium,
            U32,
            1.0,
            38,
            Input,
            |s: &SiteState| s.aux.controls_w
        ),
        point!(
            "site.aux.lighting_and_safety_w",
            "W",
            Medium,
            U32,
            1.0,
            40,
            Input,
            |s: &SiteState| s.aux.lighting_and_safety_w
        ),
        // Control surface.
        point!(
            "control.site_setpoint_w",
            "W",
            Fast,
            I32,
            1.0,
            HOLDING_SETPOINT_ADDR,
            Holding,
            |s: &SiteState| s.ems.external_setpoint_w
        ),
        point!(
            "control.ems_mode",
            "enum",
            Fast,
            U16,
            1.0,
            HOLDING_MODE_ADDR,
            Holding,
            |s: &SiteState| f64::from(s.ems.mode == EmsMode::External)
        ),
    ];

    for b in 0..cfg.blocks {
        let base = BLOCK_BASE + b as u16 * BLOCK_STRIDE;
        let prefix = format!("block{b:02}");
        points.push(point!(
            format!("{prefix}.pcs.p_ac_kw"),
            "kW",
            Fast,
            I16,
            0.001,
            base,
            Input,
            move |s: &SiteState| s.blocks[b].pcs.p_ac_w
        ));
        points.push(point!(
            format!("{prefix}.pcs.p_dc_kw"),
            "kW",
            Fast,
            I16,
            0.001,
            base + 1,
            Input,
            move |s: &SiteState| s.blocks[b].pcs.p_dc_w
        ));
        points.push(point!(
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
        ));
        points.push(point!(
            format!("{prefix}.soc_pct"),
            "%",
            Medium,
            U16,
            100.0,
            base + 3,
            Input,
            move |s: &SiteState| s.blocks[b].average_soc() * 100.0
        ));
        points.push(point!(
            format!("{prefix}.cell_temp_min_c"),
            "degC",
            Medium,
            I16,
            10.0,
            base + 4,
            Input,
            move |s: &SiteState| s.blocks[b].cell_temp_min_max_c().0
        ));
        points.push(point!(
            format!("{prefix}.cell_temp_max_c"),
            "degC",
            Medium,
            I16,
            10.0,
            base + 5,
            Input,
            move |s: &SiteState| s.blocks[b].cell_temp_min_max_c().1
        ));
        points.push(point!(
            format!("{prefix}.alarm_bits"),
            "bitfield",
            Medium,
            U16,
            1.0,
            base + 6,
            Input,
            move |s: &SiteState| {
                f64::from(
                    s.blocks[b]
                        .containers
                        .iter()
                        .flat_map(|c| c.racks.iter())
                        .fold(0u32, |acc, r| acc | r.alarm_bits),
                )
            }
        ));
        points.push(point!(
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
        ));
        points.push(point!(
            format!("{prefix}.container.air_temp_c"),
            "degC",
            Medium,
            I16,
            10.0,
            base + 8,
            Input,
            move |s: &SiteState| block_air_temp_max_c(&s.blocks[b])
        ));
        points.push(point!(
            format!("{prefix}.hvac.state"),
            "enum",
            Medium,
            U16,
            1.0,
            base + 9,
            Input,
            move |s: &SiteState| hvac_mode_code(block_hvac_mode(&s.blocks[b]))
        ));
    }

    points
}

/// Encode one physical value into registers at `point.addr`.
fn write_point(point: &Point, value: f64, bank: &mut [u16]) {
    let scaled = value * point.scale;
    let addr = point.addr as usize;
    match point.encoding {
        Encoding::U16 => {
            bank[addr] = scaled.round().clamp(0.0, f64::from(u16::MAX)) as u16;
        }
        Encoding::I16 => {
            let v = scaled
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            bank[addr] = v as u16;
        }
        Encoding::U32 => {
            let v = scaled.round().clamp(0.0, f64::from(u32::MAX)) as u32;
            bank[addr] = (v >> 16) as u16;
            bank[addr + 1] = (v & 0xFFFF) as u16;
        }
        Encoding::I32 => {
            let v = scaled
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32 as u32;
            bank[addr] = (v >> 16) as u16;
            bank[addr + 1] = (v & 0xFFFF) as u16;
        }
    }
}

/// Project the state tree into the Modbus register banks.
pub fn write_banks(points: &[Point], state: &SiteState, input: &mut [u16], holding: &mut [u16]) {
    for point in points {
        let value = (point.extract)(state);
        match point.space {
            Space::Input => write_point(point, value, input),
            Space::Holding => write_point(point, value, holding),
        }
    }
}

/// Write the signal map reference as CSV (the artifact published under
/// `refmodel/`).
///
/// The first line is a `#` comment carrying [`MAP_VERSION`], so the published
/// artifact can say which version of the contract it is. Readers skip lines
/// starting with `#`.
pub fn dump_signal_map_csv(path: &Path) -> std::io::Result<()> {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(out, "# signal-map-version: {MAP_VERSION}")?;
    writeln!(
        out,
        "space,address,words,encoding,scale,name,unit,class,access"
    )?;
    for p in &points {
        let space = match p.space {
            Space::Input => "input",
            Space::Holding => "holding",
        };
        let access = if p.writable { "rw" } else { "ro" };
        writeln!(
            out,
            "{space},{},{},{},{},{},{},{},{access}",
            p.addr,
            p.encoding.words(),
            p.encoding.as_str(),
            p.scale,
            p.name,
            p.unit,
            p.class.as_str(),
        )?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use bess_core::state::AuxPower;

    use super::*;

    /// Read a two-register unsigned value, high word first.
    fn read_u32(bank: &[u16], addr: usize) -> u32 {
        (u32::from(bank[addr]) << 16) | u32::from(bank[addr + 1])
    }

    fn banks(points: &[Point], state: &SiteState) -> Vec<u16> {
        let mut input = vec![0u16; INPUT_BANK_LEN];
        let mut holding = vec![0u16; HOLDING_BANK_LEN];
        write_banks(points, state, &mut input, &mut holding);
        input
    }

    #[test]
    fn addresses_do_not_overlap_and_fit_the_banks() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let mut input_used = vec![false; INPUT_BANK_LEN];
        let mut holding_used = vec![false; HOLDING_BANK_LEN];
        for p in &points {
            let used = match p.space {
                Space::Input => &mut input_used,
                Space::Holding => &mut holding_used,
            };
            for w in 0..p.encoding.words() {
                let a = (p.addr + w) as usize;
                assert!(a < used.len(), "{}: address {a} out of bank", p.name);
                assert!(!used[a], "{}: address {a} overlaps", p.name);
                used[a] = true;
            }
        }
    }

    #[test]
    fn names_are_unique() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let mut names: Vec<&str> = points.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), points.len());
    }

    #[test]
    fn banks_reflect_state_values() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let state = SiteState::new(&cfg, 1, 0);
        let mut input = vec![0u16; INPUT_BANK_LEN];
        let mut holding = vec![0u16; HOLDING_BANK_LEN];
        write_banks(&points, &state, &mut input, &mut holding);
        // Frequency register: 50.0 Hz x1000.
        assert_eq!(input[5], 50_000);
        // POI voltage: 110 kV x100.
        assert_eq!(input[4], 11_000);
        // Site SoC: 50% x100 within spread tolerance.
        assert!((4_900..=5_100).contains(&input[6]), "soc reg {}", input[6]);
    }

    /// The auxiliary identity the kernel guards internally has to survive the
    /// projection: whoever reads the five item registers and adds them up
    /// must land on the metered total register. Deliberately awkward values,
    /// so the assertion exercises rounding rather than round numbers.
    #[test]
    fn the_house_load_items_add_up_to_the_metered_total() {
        let cfg = PlantConfig::gw01();
        let points = build_points(&cfg);
        let mut state = SiteState::new(&cfg, 1, 0);
        state.aux = AuxPower {
            hvac_w: 41_234.6,
            bms_w: 17_232.4,
            pcs_standby_w: 6_795.5,
            controls_w: 15_000.3,
            lighting_and_safety_w: 9_999.7,
        };
        state.substation.aux_power_w = state.aux.total_w();
        let input = banks(&points, &state);

        let total = read_u32(&input, 22);
        let items = read_u32(&input, 32)
            + read_u32(&input, 34)
            + read_u32(&input, 36)
            + read_u32(&input, 38)
            + read_u32(&input, 40);
        // Five items and the total each round to the nearest watt, so the
        // sums can differ by at most 3 W. Anything larger is a wiring error.
        assert!(
            total.abs_diff(items) <= 3,
            "items sum to {items} W, total register reads {total} W"
        );
    }

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
        assert_eq!(banks(&points, &state)[1009], 2, "cooling block");

        // Same two modes, opposite draws: now the heater is the heaviest
        // consumer and the register follows it.
        state.blocks[0].containers[1].hvac.electrical_w = 4.0e3;
        assert_eq!(banks(&points, &state)[1009], 3, "heating block");
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
        // i16 at scale 10.
        assert_eq!(banks(&points, &state)[1008] as i16, 312);
    }
}
