//! What the annual run publishes: the record's shape, figure by figure.
//!
//! Kept apart from the walk that fills it, because the two answer different
//! questions: this file says what a published number means and what it is
//! entitled to claim, `run.rs` says how the run measured it.

use serde::{Deserialize, Serialize};

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
    /// Alarm raises over the run.
    pub alarms: crate::alarms::AlarmKpis,
    /// Quarter-hours the fiscal meter closed, and those on the DST days.
    pub periods: crate::periods::PeriodKpis,
}
