//! The annual run: one process, one replayed year, one set of measurements.
//!
//! Everything here is a reader. The run drives the same kernel and the same
//! default models the emulator ships, touches no configuration a normal run
//! would not have, and only watches. A calibration harness that could steer
//! the plant would be measuring itself.

use bess_core::state::HvacMode;
use bess_core::{PlantConfig, Simulation, SiteState, TICK_SECONDS};
use bess_models::{gw01_models, gw01_weather};
use serde::{Deserialize, Serialize};

/// 2026-01-01 00:00:00 UTC. The replayed weather year is mapped onto the
/// simulated calendar by date, so the run starts on the reference year's
/// first day whatever year the clock says.
pub const DEFAULT_START_UNIX_S: i64 = 1_767_225_600;

/// The seed the day-scale calibration tests use. Kept identical so the annual
/// figures and the published daily readings describe the same plant rather
/// than two plants that happen to share a name.
pub const DEFAULT_SEED: u64 = 7;

/// A non-leap year. February 29th of the leap reference year is not replayed.
pub const DEFAULT_DAYS: u64 = 365;

/// Seconds in a day.
const DAY_S: u64 = 86_400;

/// How often the rack-level temperature sweep runs, in ticks. Container air
/// is read every tick because the HVAC duty accounting already walks the
/// containers; sweeping all 480 racks that often would double the cost of the
/// run to resolve extremes that move over minutes.
const CELL_SAMPLE_TICKS: u64 = 60;

/// What the run was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSpec {
    /// PRNG seed.
    pub seed: u64,
    /// Unix timestamp (UTC seconds) of tick 0.
    pub start_unix_s: i64,
    /// Days to simulate.
    pub days: u64,
}

impl Default for RunSpec {
    fn default() -> Self {
        Self {
            seed: DEFAULT_SEED,
            start_unix_s: DEFAULT_START_UNIX_S,
            days: DEFAULT_DAYS,
        }
    }
}

/// Identity of the run that produced a set of measurements. Published beside
/// them, so a number can never be read without knowing what produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Site identifier.
    pub site_id: String,
    /// Kernel crate version.
    pub engine_version: String,
    /// PRNG seed.
    pub seed: u64,
    /// Unix timestamp (UTC seconds) of tick 0.
    pub start_unix_s: i64,
    /// Days simulated.
    pub days: u64,
    /// Ticks simulated.
    pub ticks: u64,
}

/// Energy at the point of interconnection, and what it implies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyKpis {
    /// Energy imported at the POI over the run, MWh.
    pub import_mwh: f64,
    /// Energy exported at the POI over the run, MWh.
    pub export_mwh: f64,
    /// Export over import. Everything the plant spent on itself is inside
    /// this ratio, because it was metered at the POI on the way in.
    ///
    /// Uncorrected for the stored-energy endpoints, which is deliberate: the
    /// fleet figure this is gated against is computed the same uncorrected
    /// way from reported consumption and generation, and correcting one side
    /// of a comparison is worse than correcting neither. `stored_delta_mwh`
    /// says how much the endpoints are worth.
    pub round_trip_efficiency: f64,
    /// The same ratio with the house load taken back out of import.
    ///
    /// Arithmetic on the meters, not a second measurement: a plant that truly
    /// had no auxiliary load would also load its transformer slightly less.
    /// It is published so the cost of the house load can be read off the
    /// record instead of recomputed by hand in prose that then goes stale.
    pub round_trip_efficiency_excluding_aux: f64,
    /// Energy exported over the site's nameplate energy.
    pub equivalent_full_cycles: f64,
    /// Stored energy at the end minus at the start, MWh. A round-trip figure
    /// over a finite window is only honest if the window's endpoints are
    /// stated: a run that ends fuller than it started exported less than it
    /// otherwise would have.
    pub stored_delta_mwh: f64,
    /// Auxiliary energy over energy imported, the definition every daily
    /// reading in CALIBRATION.md uses.
    pub aux_share_of_import: f64,
    /// Auxiliary energy over energy exported, for readers who define
    /// throughput as energy discharged.
    pub aux_share_of_export: f64,
    /// Unexplained energy over throughput: what the meters say crossed the
    /// POI, less what was stored and what the loss accounts claim. Zero is
    /// the only defensible value; anything else is accounting the model
    /// cannot support.
    pub balance_residual_share: f64,
}

/// The loss waterfall, every category on its own meter, MWh.
///
/// The shared unit suffix is the project's naming convention, not repetition:
/// a published energy figure whose name does not carry its unit is a figure
/// waiting to be read in the wrong one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct LossKpis {
    /// Heat dissipated inside the racks.
    pub battery_mwh: f64,
    /// PCS conversion losses.
    pub pcs_mwh: f64,
    /// Main transformer, no-load plus load losses.
    pub transformer_mwh: f64,
    /// Container HVAC.
    pub aux_hvac_mwh: f64,
    /// Rack battery-management electronics.
    pub aux_bms_mwh: f64,
    /// PCS standby tare.
    pub aux_pcs_standby_mwh: f64,
    /// Plant control, protection and SCADA.
    pub aux_controls_mwh: f64,
    /// Fire and gas detection, security, lighting and small power.
    pub aux_lighting_and_safety_mwh: f64,
}

impl LossKpis {
    /// The five auxiliary items, MWh.
    pub fn aux_total_mwh(&self) -> f64 {
        self.aux_hvac_mwh
            + self.aux_bms_mwh
            + self.aux_pcs_standby_mwh
            + self.aux_controls_mwh
            + self.aux_lighting_and_safety_mwh
    }

    /// Every category, MWh.
    pub fn total_mwh(&self) -> f64 {
        self.battery_mwh + self.pcs_mwh + self.transformer_mwh + self.aux_total_mwh()
    }

    /// The waterfall in publication order, coarsest first.
    pub fn waterfall(&self) -> [(&'static str, f64); 8] {
        [
            ("Battery", self.battery_mwh),
            ("PCS conversion", self.pcs_mwh),
            ("Transformer", self.transformer_mwh),
            ("Auxiliary: HVAC", self.aux_hvac_mwh),
            ("Auxiliary: rack electronics", self.aux_bms_mwh),
            ("Auxiliary: PCS standby", self.aux_pcs_standby_mwh),
            ("Auxiliary: controls and protection", self.aux_controls_mwh),
            (
                "Auxiliary: lighting and safety",
                self.aux_lighting_and_safety_mwh,
            ),
        ]
    }
}

/// Temperature extremes over the run, degrees Celsius. Same naming
/// convention as [`LossKpis`]: the unit stays in the name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct ThermalKpis {
    /// Coldest ambient hour of the replayed year.
    pub ambient_min_c: f64,
    /// Warmest ambient hour of the replayed year.
    pub ambient_max_c: f64,
    /// Coldest container air over the run.
    pub container_air_min_c: f64,
    /// Warmest container air over the run.
    pub container_air_max_c: f64,
    /// Coldest cell over the run.
    pub cell_min_c: f64,
    /// Warmest cell over the run.
    pub cell_max_c: f64,
}

/// What the HVAC did over the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvacKpis {
    /// Share of container-time with one cooling unit running.
    pub stage1_duty: f64,
    /// Share of container-time with both cooling units running.
    pub stage2_duty: f64,
    /// Share of container-time heating.
    pub heat_duty: f64,
    /// Transitions into cooling per container over the run. A container that
    /// steps from one unit to two is not a second start under this
    /// definition, matching the daily readings in CALIBRATION.md.
    pub compressor_starts_per_container: f64,
}

/// Everything one run measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kpis {
    /// What produced these numbers.
    pub run: RunRecord,
    /// Energy at the POI.
    pub energy: EnergyKpis,
    /// The loss waterfall.
    pub losses: LossKpis,
    /// Temperature extremes.
    pub thermal: ThermalKpis,
    /// HVAC duty and cycling.
    pub hvac: HvacKpis,
}

/// Running tallies over the walk of the state tree.
struct Tallies {
    ambient_min_c: f64,
    ambient_max_c: f64,
    air_min_c: f64,
    air_max_c: f64,
    cell_min_c: f64,
    cell_max_c: f64,
    stage1_ticks: u64,
    stage2_ticks: u64,
    heat_ticks: u64,
    starts: u64,
    cooling: Vec<bool>,
}

impl Tallies {
    fn new(containers: usize) -> Self {
        Self {
            ambient_min_c: f64::INFINITY,
            ambient_max_c: f64::NEG_INFINITY,
            air_min_c: f64::INFINITY,
            air_max_c: f64::NEG_INFINITY,
            cell_min_c: f64::INFINITY,
            cell_max_c: f64::NEG_INFINITY,
            stage1_ticks: 0,
            stage2_ticks: 0,
            heat_ticks: 0,
            starts: 0,
            cooling: vec![false; containers],
        }
    }

    /// Read one tick of the state tree.
    fn observe(&mut self, sim: &Simulation, tick: u64) {
        let state = sim.state();
        self.ambient_min_c = self.ambient_min_c.min(state.weather.ambient_c);
        self.ambient_max_c = self.ambient_max_c.max(state.weather.ambient_c);

        for (idx, container) in state
            .blocks
            .iter()
            .flat_map(|block| block.containers.iter())
            .enumerate()
        {
            self.air_min_c = self.air_min_c.min(container.air_temp_c);
            self.air_max_c = self.air_max_c.max(container.air_temp_c);
            match container.hvac.mode {
                HvacMode::Cool1 => self.stage1_ticks += 1,
                HvacMode::Cool2 => self.stage2_ticks += 1,
                HvacMode::Heat => self.heat_ticks += 1,
                HvacMode::Off => {}
            }
            let cooling = matches!(container.hvac.mode, HvacMode::Cool1 | HvacMode::Cool2);
            if cooling && !self.cooling[idx] {
                self.starts += 1;
            }
            self.cooling[idx] = cooling;
        }

        if tick.is_multiple_of(CELL_SAMPLE_TICKS) {
            let (min, max) = state.cell_temp_min_max_c();
            self.cell_min_c = self.cell_min_c.min(min);
            self.cell_max_c = self.cell_max_c.max(max);
        }
    }
}

/// Run the plant and measure it. `progress` is called once per simulated day
/// with the day just completed and the total.
///
/// Returns the gated annual figures and the chart material together, because
/// they come from the same pass: producing a figure from a second run would
/// be producing it from a second plant.
pub fn run(spec: RunSpec, progress: impl FnMut(u64, u64)) -> (Kpis, crate::series::StudySeries) {
    measure(spec, progress)
}

fn measure(
    spec: RunSpec,
    mut progress: impl FnMut(u64, u64),
) -> (Kpis, crate::series::StudySeries) {
    let cfg = PlantConfig::gw01();
    let site_id = cfg.site_id.clone();
    let containers = cfg.blocks * cfg.containers_per_block;
    let nominal_energy_wh = cfg.nominal_energy_wh();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, spec.seed, spec.start_unix_s);
    let weather = gw01_weather();

    let stored_start_wh = sim.stored_energy_wh();
    let ticks = spec.days * DAY_S / TICK_SECONDS;
    let mut tallies = Tallies::new(containers);
    let (window, label) = crate::series::window::hot_week(weather, spec);
    let mut series = crate::series::Collector::new(
        containers,
        window,
        crate::series::window::TRACE_INTERVAL_S,
        label,
    );

    for tick in 0..ticks {
        let inputs = weather.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
        tallies.observe(&sim, tick);
        series.observe(&sim, sim.unix_time_s());
        if (tick + 1) % (DAY_S / TICK_SECONDS) == 0 {
            progress((tick + 1) / (DAY_S / TICK_SECONDS), spec.days);
        }
    }

    let stored_delta_wh = sim.stored_energy_wh() - stored_start_wh;
    let container_ticks = ticks as f64 * containers as f64;
    let state = sim.state();
    let losses = losses_of(state);

    let kpis = Kpis {
        run: RunRecord {
            site_id,
            engine_version: bess_core::version().to_string(),
            seed: spec.seed,
            start_unix_s: spec.start_unix_s,
            days: spec.days,
            ticks,
        },
        energy: energy_of(state, stored_delta_wh, nominal_energy_wh),
        losses,
        thermal: ThermalKpis {
            ambient_min_c: round(tallies.ambient_min_c, 1),
            ambient_max_c: round(tallies.ambient_max_c, 1),
            container_air_min_c: round(tallies.air_min_c, 1),
            container_air_max_c: round(tallies.air_max_c, 1),
            cell_min_c: round(tallies.cell_min_c, 1),
            cell_max_c: round(tallies.cell_max_c, 1),
        },
        hvac: HvacKpis {
            stage1_duty: round(tallies.stage1_ticks as f64 / container_ticks, 5),
            stage2_duty: round(tallies.stage2_ticks as f64 / container_ticks, 5),
            heat_duty: round(tallies.heat_ticks as f64 / container_ticks, 5),
            compressor_starts_per_container: round(tallies.starts as f64 / containers as f64, 1),
        },
    };
    let series = series.finish(&sim, kpis.run.clone());
    (kpis, series)
}

/// Read the loss meters. Every figure comes off an accumulator the kernel
/// maintained during the run; nothing here re-derives physics from a summary.
fn losses_of(state: &bess_core::SiteState) -> LossKpis {
    let energy = &state.energy;
    let items = &energy.aux_items;
    LossKpis {
        battery_mwh: mwh(energy.battery_loss_wh),
        pcs_mwh: mwh(energy.pcs_loss_wh),
        transformer_mwh: mwh(energy.transformer_loss_wh),
        aux_hvac_mwh: mwh(items.hvac_wh),
        aux_bms_mwh: mwh(items.bms_wh),
        aux_pcs_standby_mwh: mwh(items.pcs_standby_wh),
        aux_controls_mwh: mwh(items.controls_wh),
        aux_lighting_and_safety_mwh: mwh(items.lighting_and_safety_wh),
    }
}

/// Close the books at the POI.
///
/// The auxiliary shares come off the substation's own meter rather than off
/// the item accumulators, because that is what the ratio means: what the
/// house load cost, as the plant metered it. The two agree, and CI holds them
/// to agree, but the published waterfall is rounded to the digits it prints
/// and a ratio should not inherit a rounding it does not need.
fn energy_of(state: &SiteState, stored_delta_wh: f64, nominal_energy_wh: f64) -> EnergyKpis {
    let import_wh = state.substation.import_wh;
    let export_wh = state.substation.export_wh;
    let aux_wh = state.energy.aux_wh;
    let metered_losses_wh = state.energy.battery_loss_wh
        + state.energy.pcs_loss_wh
        + state.energy.transformer_loss_wh
        + aux_wh;
    let residual_wh = (import_wh - export_wh) - stored_delta_wh - metered_losses_wh;
    let throughput_wh = (import_wh + export_wh).max(1.0);

    EnergyKpis {
        import_mwh: mwh(import_wh),
        export_mwh: mwh(export_wh),
        round_trip_efficiency: round(export_wh / import_wh.max(1.0), 4),
        round_trip_efficiency_excluding_aux: round(export_wh / (import_wh - aux_wh).max(1.0), 4),
        equivalent_full_cycles: round(export_wh / nominal_energy_wh, 1),
        stored_delta_mwh: mwh(stored_delta_wh),
        aux_share_of_import: round(aux_wh / import_wh.max(1.0), 5),
        aux_share_of_export: round(aux_wh / export_wh.max(1.0), 5),
        balance_residual_share: round(residual_wh.abs() / throughput_wh, 7),
    }
}

/// Watt-hours to megawatt-hours, at the precision this record publishes.
fn mwh(wh: f64) -> f64 {
    round(wh / 1.0e6, 1)
}

/// Round to a fixed number of decimals.
///
/// Every published figure passes through here, which is what lets the
/// committed record be compared field by field: the run is deterministic, but
/// the last bits of a float are not worth defending across architectures, and
/// no reading in this file is meaningful past the digits it keeps.
fn round(value: f64, decimals: u32) -> f64 {
    let scale = 10f64.powi(decimals as i32);
    (value * scale).round() / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waterfall_lists_every_loss_account() {
        let losses = LossKpis {
            battery_mwh: 1.0,
            pcs_mwh: 2.0,
            transformer_mwh: 4.0,
            aux_hvac_mwh: 8.0,
            aux_bms_mwh: 16.0,
            aux_pcs_standby_mwh: 32.0,
            aux_controls_mwh: 64.0,
            aux_lighting_and_safety_mwh: 128.0,
        };
        // Powers of two: any category dropped from or duplicated in the
        // published waterfall changes the sum.
        let listed: f64 = losses.waterfall().iter().map(|(_, mwh)| mwh).sum();
        assert!((listed - losses.total_mwh()).abs() < 1.0e-9);
        assert!((losses.aux_total_mwh() - 248.0).abs() < 1.0e-9);
    }

    #[test]
    fn published_figures_keep_only_the_digits_they_mean() {
        assert!((round(0.876_543_21, 4) - 0.8765).abs() < 1.0e-12);
        assert!((mwh(1_234_567.0) - 1.2).abs() < 1.0e-12);
    }
}
