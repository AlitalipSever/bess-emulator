//! The shapes of a year that a single number cannot hold.
//!
//! The annual record answers "what did the plant do over the year". A study
//! asks two things it cannot: how the answer moved from month to month, and
//! what one hard week actually looked like hour by hour. Both come off the
//! same single pass as everything else, because a second run to produce a
//! chart would be a second plant.
//!
//! Kept apart from the annual KPIs on purpose. Those are gated in CI and
//! compared field by field against a committed record; these are material
//! for a chart, they are large, and holding them to the same tolerance would
//! mean re-measuring a year every time a curve wobbled in its last digit.

pub mod window;

use bess_core::state::HvacMode;
use bess_core::{Simulation, SiteState};
use bess_models::month_day_from_unix_days;
use serde::{Deserialize, Serialize};

/// One month of the replayed year.
///
/// `month` names which one, and the unit suffixes are the project's naming
/// convention rather than repetition: a published energy figure whose name
/// does not carry its unit is one waiting to be read in the wrong one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct Month {
    /// 1 to 12.
    pub month: u32,
    /// Days the bucket covers. A run that does not start and end on month
    /// boundaries has partial months at its ends, and a chart that treated
    /// them as whole ones would draw a cliff that is not there.
    pub days: f64,
    /// Energy imported at the POI, MWh.
    pub import_mwh: f64,
    /// Energy exported at the POI, MWh.
    pub export_mwh: f64,
    /// Export over import for the month alone.
    pub round_trip_efficiency: f64,
    /// Auxiliary energy, MWh.
    pub aux_mwh: f64,
    /// Auxiliary energy over energy imported.
    pub aux_share_of_import: f64,
    /// Container HVAC energy, MWh, the item that carries the season.
    pub hvac_mwh: f64,
    /// Mean ambient temperature over the month, degrees Celsius.
    pub ambient_mean_c: f64,
    /// Share of container-time with cooling running.
    pub cooling_duty: f64,
    /// Share of container-time heating.
    pub heating_duty: f64,
}

/// One sample of a trace: the thermal chain at an instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Unix seconds, UTC.
    pub unix_s: i64,
    /// Ambient air, degrees Celsius.
    pub ambient_c: f64,
    /// Global horizontal irradiance, W/m2.
    pub irradiance_wm2: f64,
    /// Coldest and warmest container air on site.
    pub air_min_c: f64,
    /// Warmest container air on site.
    pub air_max_c: f64,
    /// Coldest cell on site.
    pub cell_min_c: f64,
    /// Warmest cell on site.
    pub cell_max_c: f64,
    /// Containers running one cooling unit.
    pub cooling_1: usize,
    /// Containers running both.
    pub cooling_2: usize,
    /// Containers heating.
    pub heating: usize,
    /// Active power at the POI, MW, positive when exporting.
    pub poi_mw: f64,
    /// Auxiliary draw at that instant, kW.
    pub aux_kw: f64,
}

/// A named window of samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    /// What the window is, for a chart caption.
    pub label: String,
    /// Seconds between samples.
    pub interval_s: u64,
    /// The samples, in order.
    pub samples: Vec<Sample>,
}

/// Everything a study needs beyond the annual figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudySeries {
    /// The run that produced this, mirrored from the annual record so a
    /// chart can never be shown beside figures from a different plant.
    pub run: crate::run::RunRecord,
    /// Month by month.
    pub months: Vec<Month>,
    /// The hardest week the year had, whichever one that turns out to be.
    pub hot_week: Trace,
}

/// Accumulates what a chart needs while the year runs.
///
/// Folded into the same pass as everything else. A second run to produce a
/// figure would be a second plant, and the two would drift the first time
/// anything changed.
pub struct Collector {
    /// One entry per month the run passes through, in order.
    ///
    /// A list rather than twelve slots keyed by calendar month, because a
    /// year-long run that starts on 1 January ends on 1 January of the next
    /// year: its last tick belongs to a second January, and keying by
    /// calendar month let that one-tick month overwrite the real one with
    /// zeroes. Found by reading the output rather than by a test, which is
    /// why the output is now published with the day count that would have
    /// made it obvious.
    months: Vec<MonthAccumulator>,
    interval_s: u64,
    window: (i64, i64),
    label: String,
    samples: Vec<Sample>,
    containers: f64,
    opened_at: Meters,
}

/// Meter readings at the last month boundary. A month's energy is the
/// difference between two snapshots rather than a second set of accumulators
/// kept in step with the kernel's own.
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_field_names)]
struct Meters {
    import_wh: f64,
    export_wh: f64,
    aux_wh: f64,
    hvac_wh: f64,
}

#[derive(Default)]
struct MonthAccumulator {
    month: u32,
    closed: Meters,
    ambient_sum: f64,
    ticks: f64,
    cooling_ticks: f64,
    heating_ticks: f64,
}

impl Collector {
    /// Prepare to collect. The window is the trace's start and end in
    /// simulated Unix seconds.
    pub fn new(containers: usize, window: (i64, i64), interval_s: u64, label: String) -> Self {
        Self {
            months: Vec::new(),
            interval_s,
            window,
            label,
            samples: Vec::new(),
            containers: containers as f64,
            opened_at: Meters::default(),
        }
    }

    /// Read one tick.
    pub fn observe(&mut self, sim: &Simulation, unix_s: i64) {
        let state = sim.state();
        let (month, _) = month_day_from_unix_days(unix_s.div_euclid(86_400));
        let index = month as u32 - 1;
        let now = Meters {
            import_wh: state.substation.import_wh,
            export_wh: state.substation.export_wh,
            aux_wh: state.energy.aux_wh,
            hvac_wh: state.energy.aux_items.hvac_wh,
        };
        if self.months.last().map(|m| m.month) != Some(index) {
            // A month begins: bank what the meters read, and close the one
            // before it against the same reading.
            if let Some(previous) = self.months.last_mut() {
                previous.closed = difference(now, self.opened_at);
            }
            self.opened_at = now;
            self.months.push(MonthAccumulator {
                month: index,
                ..MonthAccumulator::default()
            });
        }

        let acc = self.months.last_mut().expect("a month was just opened");
        acc.ambient_sum += state.weather.ambient_c;
        acc.ticks += 1.0;
        for container in state.blocks.iter().flat_map(|b| b.containers.iter()) {
            match container.hvac.mode {
                HvacMode::Cool1 | HvacMode::Cool2 => acc.cooling_ticks += 1.0,
                HvacMode::Heat => acc.heating_ticks += 1.0,
                HvacMode::Off => {}
            }
        }

        if unix_s >= self.window.0
            && unix_s <= self.window.1
            && (unix_s - self.window.0).rem_euclid(self.interval_s as i64) == 0
        {
            self.samples.push(sample(state, unix_s));
        }
    }

    /// Close the books and hand over the series.
    pub fn finish(mut self, sim: &Simulation, run: crate::run::RunRecord) -> StudySeries {
        let state = sim.state();
        if let Some(last) = self.months.last_mut() {
            let now = Meters {
                import_wh: state.substation.import_wh,
                export_wh: state.substation.export_wh,
                aux_wh: state.energy.aux_wh,
                hvac_wh: state.energy.aux_items.hvac_wh,
            };
            last.closed = difference(now, self.opened_at);
        }

        // A bucket shorter than a day is the run spilling over a boundary,
        // not a month. The year-long run's final tick makes exactly one.
        let months = self
            .months
            .iter()
            .filter(|acc| acc.ticks >= f64::from(DAY_TICKS))
            .map(|acc| {
                let container_ticks = acc.ticks * self.containers;
                Month {
                    month: acc.month + 1,
                    days: round(acc.ticks / f64::from(DAY_TICKS), 2),
                    import_mwh: round(acc.closed.import_wh / 1.0e6, 1),
                    export_mwh: round(acc.closed.export_wh / 1.0e6, 1),
                    round_trip_efficiency: round(
                        acc.closed.export_wh / acc.closed.import_wh.max(1.0),
                        4,
                    ),
                    aux_mwh: round(acc.closed.aux_wh / 1.0e6, 1),
                    aux_share_of_import: round(
                        acc.closed.aux_wh / acc.closed.import_wh.max(1.0),
                        5,
                    ),
                    hvac_mwh: round(acc.closed.hvac_wh / 1.0e6, 1),
                    ambient_mean_c: round(acc.ambient_sum / acc.ticks, 1),
                    cooling_duty: round(acc.cooling_ticks / container_ticks, 5),
                    heating_duty: round(acc.heating_ticks / container_ticks, 5),
                }
            })
            .collect();

        StudySeries {
            run,
            months,
            hot_week: Trace {
                label: self.label,
                interval_s: self.interval_s,
                samples: self.samples,
            },
        }
    }
}

fn difference(now: Meters, then: Meters) -> Meters {
    Meters {
        import_wh: now.import_wh - then.import_wh,
        export_wh: now.export_wh - then.export_wh,
        aux_wh: now.aux_wh - then.aux_wh,
        hvac_wh: now.hvac_wh - then.hvac_wh,
    }
}

fn sample(state: &SiteState, unix_s: i64) -> Sample {
    let (mut air_min, mut air_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut cooling_1, mut cooling_2, mut heating) = (0, 0, 0);
    for container in state.blocks.iter().flat_map(|b| b.containers.iter()) {
        air_min = air_min.min(container.air_temp_c);
        air_max = air_max.max(container.air_temp_c);
        match container.hvac.mode {
            HvacMode::Cool1 => cooling_1 += 1,
            HvacMode::Cool2 => cooling_2 += 1,
            HvacMode::Heat => heating += 1,
            HvacMode::Off => {}
        }
    }
    let (cell_min, cell_max) = state.cell_temp_min_max_c();
    Sample {
        unix_s,
        ambient_c: round(state.weather.ambient_c, 1),
        irradiance_wm2: round(state.weather.irradiance_wm2, 0),
        air_min_c: round(air_min, 1),
        air_max_c: round(air_max, 1),
        cell_min_c: round(cell_min, 1),
        cell_max_c: round(cell_max, 1),
        cooling_1,
        cooling_2,
        heating,
        poi_mw: round(state.substation.poi_active_power_w / 1.0e6, 2),
        aux_kw: round(state.aux.total_w() / 1.0e3, 1),
    }
}

/// Ticks in a day, the unit a month is measured in here.
const DAY_TICKS: u32 = 86_400;

/// Same rounding discipline as the annual record: publish the digits that
/// mean something and no more.
fn round(value: f64, decimals: u32) -> f64 {
    let scale = 10f64.powi(decimals as i32);
    (value * scale).round() / scale
}

#[cfg(test)]
mod tests {
    use super::StudySeries;

    fn committed() -> StudySeries {
        let text = include_str!("../../../calibration/m1-study-series.json");
        crate::report::series_from_json(text).expect("the committed series parses")
    }

    #[test]
    fn the_year_is_twelve_whole_and_distinct_months() {
        // The defect this pins: a year-long run starting on 1 January ends on
        // 1 January of the next year, so its final tick belongs to a second
        // January. Keyed by calendar month, that one-tick month overwrote the
        // real one and published a January of zeroes. Reading the output
        // found it; nothing else did.
        let series = committed();
        assert_eq!(series.months.len(), 12, "{:?}", series.months);

        let mut seen = std::collections::BTreeSet::new();
        for month in &series.months {
            assert!(
                seen.insert(month.month),
                "calendar month {} appears twice",
                month.month
            );
            assert!(
                (28.0..=31.0).contains(&month.days),
                "month {} covers {} days",
                month.month,
                month.days
            );
            assert!(
                month.import_mwh > 1_000.0,
                "month {} imported {} MWh, which is not a month of this plant",
                month.month,
                month.import_mwh
            );
            assert!(
                (0.70..0.95).contains(&month.round_trip_efficiency),
                "month {} reads {:.4} round trip",
                month.month,
                month.round_trip_efficiency
            );
        }
        assert_eq!(seen.len(), 12);
    }

    #[test]
    fn the_seasons_are_visible_in_the_monthly_figures() {
        // The whole reason the monthly series exists: the annual number hides
        // a season. If a change ever flattens this, the study built on it is
        // making a claim its data no longer supports.
        let series = committed();
        let january = &series.months[0];
        let august = &series.months[7];
        assert_eq!((january.month, august.month), (1, 8));

        assert!(
            january.round_trip_efficiency > august.round_trip_efficiency + 0.015,
            "January {:.4} against August {:.4}: the seasonal spread is gone",
            january.round_trip_efficiency,
            august.round_trip_efficiency
        );
        assert!(
            august.hvac_mwh > january.hvac_mwh * 1.8,
            "August cooling {:.1} MWh against January {:.1}",
            august.hvac_mwh,
            january.hvac_mwh
        );
        assert!(
            august.ambient_mean_c > january.ambient_mean_c + 10.0,
            "the replayed year has no summer"
        );
    }

    #[test]
    fn the_trace_is_an_even_walk_through_one_week() {
        let series = committed();
        let trace = &series.hot_week;
        assert!(!trace.label.is_empty());
        assert!(
            trace.samples.len() > 600,
            "only {} samples in the week",
            trace.samples.len()
        );

        let interval = trace.interval_s as i64;
        for pair in trace.samples.windows(2) {
            let step = pair[1].unix_s - pair[0].unix_s;
            assert_eq!(step, interval, "the trace skipped from {}", pair[0].unix_s);
        }

        // It is meant to be the warm week, and every sample has to be a
        // plausible plant rather than a hole in the data.
        let warmest = trace
            .samples
            .iter()
            .map(|s| s.ambient_c)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(warmest > 22.0, "the warm week peaked at {warmest} C");
        for s in &trace.samples {
            assert!(s.air_max_c >= s.air_min_c);
            assert!(s.cell_max_c >= s.cell_min_c);
            assert!(s.cooling_1 + s.cooling_2 + s.heating <= 40);
            assert!(s.aux_kw > 0.0, "the plant drew nothing at {}", s.unix_s);
        }
    }

    #[test]
    fn the_series_and_the_record_describe_the_same_run() {
        let record = crate::report::from_json(include_str!("../../../calibration/m1-annual.json"))
            .expect("the committed record parses");
        assert_eq!(committed().run, record.run);
    }
}
