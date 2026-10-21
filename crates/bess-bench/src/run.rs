//! The annual run: one process, one replayed year, one set of measurements.
//!
//! Everything here is a reader. The run drives the same kernel and the same
//! default models the emulator ships, touches no configuration a normal run
//! would not have, and only watches. A calibration harness that could steer
//! the plant would be measuring itself.

use bess_core::state::HvacMode;
use bess_core::{PlantConfig, Simulation, SiteState, TICK_SECONDS};
use bess_models::{gw01_models, gw01_weather};
mod record;

pub use record::{ControlKpis, EnergyKpis, HvacKpis, Kpis, LossKpis, RunRecord, ThermalKpis};

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
    /// The quarter-hour report's running sums (M4 phase 2 PR2).
    periods_seen: u32,
    avail_discharge_sum: f64,
    avail_charge_sum: f64,
    abs_deviation_sum: f64,
    over_frame: u64,
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
            periods_seen: 0,
            avail_discharge_sum: 0.0,
            avail_charge_sum: 0.0,
            abs_deviation_sum: 0.0,
            over_frame: 0,
        }
    }

    /// The report over the periods seen.
    fn control(&self) -> ControlKpis {
        let n = f64::from(self.periods_seen.max(1));
        ControlKpis {
            availability_discharge: round(self.avail_discharge_sum / n, 4),
            availability_charge: round(self.avail_charge_sum / n, 4),
            mean_abs_deviation_share: round(self.abs_deviation_sum / n, 5),
            periods_over_frame: self.over_frame,
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

        // The quarter-hour report: one reading per closed period, on the
        // tick that closed it.
        let closed = state.substation.revenue.periods_closed;
        if closed != self.periods_seen {
            self.periods_seen = closed;
            self.avail_discharge_sum += state.kpi.last_availability_discharge;
            self.avail_charge_sum += state.kpi.last_availability_charge;
            self.abs_deviation_sum += state.kpi.last_deviation_share.abs();
            self.over_frame += u64::from(state.kpi.over_frame);
        }
    }
}

/// Run the plant and measure it. `progress` is called once per simulated day
/// with the day just completed and the total.
///
/// Returns the gated annual figures and the chart material together, because
/// they come from the same pass: producing a figure from a second run would
/// be producing it from a second plant.
pub fn run(spec: RunSpec, progress: impl FnMut(u64, u64)) -> Measured {
    measure(spec, progress)
}

/// What one pass produces: the record, the chart material, and every
/// closed quarter-hour (written only on request).
pub type Measured = (
    Kpis,
    crate::series::StudySeries,
    Vec<crate::periods::ClosedPeriod>,
);

fn measure(spec: RunSpec, mut progress: impl FnMut(u64, u64)) -> Measured {
    let cfg = PlantConfig::gw01();
    let site_id = cfg.site_id.clone();
    let containers = cfg.blocks * cfg.containers_per_block;
    let racks = cfg.total_racks();
    let nominal_energy_wh = cfg.nominal_energy_wh();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, spec.seed, spec.start_unix_s);
    let weather = gw01_weather();

    let stored_start_wh = sim.stored_energy_wh();
    let ticks = spec.days * DAY_S / TICK_SECONDS;
    let mut tallies = Tallies::new(containers);
    let mut alarms = crate::alarms::AlarmKpis::default();
    let mut estimator = crate::estimator::Tally::default();
    let mut periods = crate::periods::Collector::new(spec.start_unix_s);
    let (window, label) = crate::series::window::hot_week(weather, spec);
    let mut series = crate::series::Collector::new(
        containers,
        window,
        crate::series::window::TRACE_INTERVAL_S,
        label,
    );

    for tick in 0..ticks {
        let inputs = weather.inputs_at(sim.unix_time_s());
        {
            let events = sim.step(&inputs);
            alarms.observe(events);
            estimator.observe_events(events);
        }
        tallies.observe(&sim, tick);
        if tick.is_multiple_of(CELL_SAMPLE_TICKS) {
            estimator.observe_state(sim.state());
        }
        periods.observe(sim.state());
        series.observe(&sim, sim.unix_time_s());
        if (tick + 1) % (DAY_S / TICK_SECONDS) == 0 {
            progress((tick + 1) / (DAY_S / TICK_SECONDS), spec.days);
        }
    }

    let stored_delta_wh = sim.stored_energy_wh() - stored_start_wh;
    let container_ticks = ticks as f64 * containers as f64;
    let state = sim.state();
    let losses = losses_of(state);
    let (period_kpis, period_rows) = periods.finish();

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
        alarms,
        periods: period_kpis,
        estimator: estimator.finish(racks, spec.days),
        control: tallies.control(),
    };
    let series = series.finish(&sim, kpis.run.clone());
    (kpis, series, period_rows)
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
    // Zero on a year without a trip; a scenario that opens the breaker
    // feeds the house load through here, and the balance has to see it.
    let station_service_wh = state.substation.station_service_wh;
    let aux_wh = state.energy.aux_wh;
    let metered_losses_wh = state.energy.battery_loss_wh
        + state.energy.pcs_loss_wh
        + state.energy.transformer_loss_wh
        + aux_wh;
    let residual_wh =
        (import_wh + station_service_wh - export_wh) - stored_delta_wh - metered_losses_wh;
    // The import-side ratios read only the house load that came in through
    // the POI: what the station-service feed carried never crossed the
    // import meter, so subtracting it from the import would count it twice.
    let aux_via_poi_wh = aux_wh - station_service_wh;
    let throughput_wh = (import_wh + export_wh).max(1.0);

    EnergyKpis {
        import_mwh: mwh(import_wh),
        export_mwh: mwh(export_wh),
        round_trip_efficiency: round(export_wh / import_wh.max(1.0), 4),
        round_trip_efficiency_excluding_aux: round(
            export_wh / (import_wh - aux_via_poi_wh).max(1.0),
            4,
        ),
        equivalent_full_cycles: round(export_wh / nominal_energy_wh, 1),
        stored_delta_mwh: mwh(stored_delta_wh),
        aux_share_of_import: round(aux_via_poi_wh / import_wh.max(1.0), 5),
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
pub(crate) fn round(value: f64, decimals: u32) -> f64 {
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

    /// A year with a trip: part of the house load came in on the
    /// station-service feed and never crossed the import meter, so the
    /// import-side ratios leave it out while the balance takes it in.
    #[test]
    fn the_import_ratios_leave_out_what_station_service_carried() {
        let cfg = bess_core::PlantConfig::gw01();
        let mut state = SiteState::new(&cfg, 1, 0);
        state.substation.import_wh = 1_000.0e6;
        state.substation.export_wh = 800.0e6;
        state.substation.station_service_wh = 30.0e6;
        state.energy.aux_wh = 100.0e6;
        state.energy.battery_loss_wh = 130.0e6;
        let e = energy_of(&state, 0.0, 200.0e6);
        assert!((e.round_trip_efficiency_excluding_aux - round(800.0 / 930.0, 4)).abs() < 1.0e-12);
        assert!((e.aux_share_of_import - 0.07).abs() < 1.0e-12);
        assert!((e.aux_share_of_export - 0.125).abs() < 1.0e-12);
        assert!(e.balance_residual_share.abs() < 1.0e-12);
    }

    #[test]
    fn published_figures_keep_only_the_digits_they_mean() {
        assert!((round(0.876_543_21, 4) - 0.8765).abs() < 1.0e-12);
        assert!((mwh(1_234_567.0) - 1.2).abs() < 1.0e-12);
    }
}
