//! The projected decade (M5 phase 2): the measured year's stress replayed
//! through the aging model, year after year, and two laboratory
//! trajectories beside it for the laboratory band.
//!
//! The projection reads the year; it never changes the annual record,
//! which is why it runs after the record is taken and publishes to a file
//! of its own (`calibration/m5-aging.json`).

use bess_core::config::RackConfig;
use bess_core::state::aging::{RackAge, StressDay};
use bess_core::traits::AgingModel;
use bess_core::{Simulation, SiteState, StressYear};
use bess_models::LfpAging;
use serde::{Deserialize, Serialize};

use crate::run::RunRecord;

/// Days in a projected year.
const YEAR_DAYS: u64 = 365;

/// Where a laboratory trajectory stops looking for its crossing.
const LAB_MAX_YEARS: u32 = 40;

/// The published trajectory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgingTrajectory {
    /// The measured year the projection replays.
    pub run: RunRecord,
    /// How the projection was made: `projected` repeats the measured
    /// stress year through the aging model, `resimulated` ticks the
    /// physics every year.
    pub method: String,
    /// One point per year, year 0 the plant as the measured year left it.
    pub years: Vec<YearPoint>,
    /// Laboratory conditions through the same model.
    pub lab: Vec<LabTrajectory>,
}

/// The plant after `year` years.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YearPoint {
    /// Years since commissioning.
    pub year: u32,
    /// Mean state of health over every rack.
    pub soh_mean: f64,
    /// The worst rack.
    pub soh_min: f64,
    /// The best rack.
    pub soh_max: f64,
    /// Mean cycles per rack since commissioning.
    pub efc_mean: f64,
    /// Mean resistance growth, fraction.
    pub resistance_growth_mean: f64,
}

/// One rack under a laboratory condition, through the model alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabTrajectory {
    /// What the condition is.
    pub label: String,
    /// Cell temperature, degrees Celsius.
    pub temp_c: f64,
    /// Cycles a day.
    pub efc_per_day: f64,
    /// Depth of discharge.
    pub dod: f64,
    /// Rate, C.
    pub c_rate: f64,
    /// Cycles at which the state of health crossed 80 %, or absent when
    /// it did not inside the search.
    pub efc_to_80_pct: Option<f64>,
    /// Years at that crossing.
    pub years_to_80_pct: Option<f64>,
    /// State of health after 8,000 cycles, for the datasheet claim.
    pub soh_at_8000_efc: f64,
}

/// Read the plant's point at `year`.
pub fn point_of(state: &SiteState, year: u32) -> YearPoint {
    let mut n = 0.0_f64;
    let (mut sum, mut min, mut max) = (0.0, f64::INFINITY, f64::NEG_INFINITY);
    let (mut efc, mut res) = (0.0, 0.0);
    for rack in state.racks() {
        n += 1.0;
        sum += rack.soh;
        min = min.min(rack.soh);
        max = max.max(rack.soh);
        efc += rack.age.efc;
        res += rack.age.resistance_growth();
    }
    let n = n.max(1.0);
    YearPoint {
        year,
        soh_mean: round(sum / n, 5),
        soh_min: round(min, 5),
        soh_max: round(max, 5),
        efc_mean: round(efc / n, 1),
        resistance_growth_mean: round(res / n, 5),
    }
}

/// Project `years - 1` more years from the measured year's stress, or
/// tick them when `resimulate`; `tick_year` runs the physics for one year
/// and is the bench's own loop.
pub fn project(
    sim: &mut Simulation,
    year: &StressYear,
    years: u32,
    resimulate: bool,
    mut tick_year: impl FnMut(&mut Simulation),
) -> Vec<YearPoint> {
    let mut points = vec![point_of(sim.state(), 1)];
    for y in 2..=years {
        if resimulate {
            tick_year(sim);
        } else {
            sim.fast_forward(year, YEAR_DAYS);
        }
        points.push(point_of(sim.state(), y));
    }
    points
}

/// The laboratory conditions: a plant's cell at two cycles a day in a
/// 25 °C container, the condition inside Preger et al.'s matrix closest
/// to a plant's, and the same at 35 °C.
pub fn lab_trajectories(cfg: &RackConfig) -> Vec<LabTrajectory> {
    [
        ("two cycles a day at 25 °C", 25.0),
        ("two cycles a day at 35 °C", 35.0),
    ]
    .into_iter()
    .map(|(label, temp_c)| lab_trajectory(cfg, label, temp_c, 2.0, 0.9, 0.5))
    .collect()
}

fn lab_trajectory(
    cfg: &RackConfig,
    label: &str,
    temp_c: f64,
    efc_per_day: f64,
    dod: f64,
    c_rate: f64,
) -> LabTrajectory {
    let model = LfpAging::gw01();
    let day = StressDay {
        days: 1.0,
        efc: efc_per_day,
        soc_mean: 0.5,
        temp_k_mean: temp_c + 273.15,
        dod,
        c_rate,
    };
    let mut age = RackAge::fresh(1.0);
    let mut crossing = None;
    let mut soh_at_8000 = None;
    let mut days = 0u64;
    while days < u64::from(LAB_MAX_YEARS) * YEAR_DAYS {
        model.age_rack(&mut age, cfg, &day);
        age.days += 1.0;
        age.efc += day.efc;
        days += 1;
        if crossing.is_none() && age.soh() <= 0.8 {
            crossing = Some((age.efc, age.days / YEAR_DAYS as f64));
        }
        if soh_at_8000.is_none() && age.efc >= 8000.0 {
            soh_at_8000 = Some(age.soh());
        }
        if crossing.is_some() && soh_at_8000.is_some() {
            break;
        }
    }
    LabTrajectory {
        label: label.to_owned(),
        temp_c,
        efc_per_day,
        dod,
        c_rate,
        efc_to_80_pct: crossing.map(|(efc, _)| round(efc, 0)),
        years_to_80_pct: crossing.map(|(_, years)| round(years, 2)),
        soh_at_8000_efc: round(soh_at_8000.unwrap_or_else(|| age.soh()), 4),
    }
}

fn round(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

/// The trajectory as published.
pub fn to_json(trajectory: &AgingTrajectory) -> String {
    let mut text = serde_json::to_string_pretty(trajectory).expect("plain data");
    text.push('\n');
    text
}

/// Read a published trajectory.
pub fn from_json(text: &str) -> Result<AgingTrajectory, String> {
    serde_json::from_str(text).map_err(|err| format!("not a trajectory: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::PlantConfig;

    /// The laboratory trajectory crosses 80 % and reports 8,000 cycles.
    #[test]
    fn the_laboratory_rack_crosses_eighty_percent() {
        let cfg = PlantConfig::gw01().rack;
        let labs = lab_trajectories(&cfg);
        assert_eq!(labs.len(), 2);
        for lab in &labs {
            let efc = lab.efc_to_80_pct.expect("a crossing inside forty years");
            assert!(efc > 1000.0 && efc < 20_000.0, "{efc}");
            assert!(lab.soh_at_8000_efc < 0.8);
        }
        // The model's cycle term falls with temperature (its authors'
        // stated artifact of the fit), so at two cycles a day the warmer
        // rack crosses later, not sooner; the calendar term rises with
        // temperature, which the model's unit tests hold. Both are inside
        // the search either way.
        assert!(labs[1].years_to_80_pct.is_some());
    }

    #[test]
    fn a_trajectory_round_trips() {
        let cfg = PlantConfig::gw01();
        let state = SiteState::new(&cfg, 7, 1_767_225_600);
        let trajectory = AgingTrajectory {
            run: RunRecord {
                site_id: cfg.site_id.clone(),
                engine_version: "0.9.0".to_owned(),
                seed: 7,
                start_unix_s: 1_767_225_600,
                days: 365,
                ticks: 31_536_000,
                market: false,
            },
            method: "projected".to_owned(),
            years: vec![point_of(&state, 0)],
            lab: Vec::new(),
        };
        let back = from_json(&to_json(&trajectory)).expect("round trip");
        assert_eq!(back, trajectory);
        assert_eq!(back.years[0].soh_mean, 1.0);
    }
}
