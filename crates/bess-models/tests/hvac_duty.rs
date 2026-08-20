//! What the HVAC actually does over a replayed day, at plant scale.
//!
//! The unit tests in `thermal.rs` pin the control law on one container. These
//! pin the two things only a whole day of real weather and real dispatch can
//! show: that the unit is sized to hold the plant, and that it does so
//! without cycling the compressors to death.

use std::sync::OnceLock;

use bess_core::state::HvacMode;
use bess_core::{PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-01-01 00:00:00 UTC.
const NEW_YEAR_S: i64 = 1_767_225_600;
/// 2026-07-14, the warmest stretch of the replayed year.
const JULY_S: i64 = NEW_YEAR_S + 194 * 86_400;

struct DayReport {
    max_air_c: f64,
    max_cell_c: f64,
    stage1_ticks: u64,
    stage2_ticks: u64,
    heat_ticks: u64,
    /// Compressor starts per container over the day.
    starts_per_container: f64,
    /// Auxiliary energy over energy imported at the point of interconnection.
    aux_share_of_import: f64,
    /// Container-ticks in the day, the denominator for a duty figure.
    container_ticks: f64,
}

impl DayReport {
    fn stage1_duty(&self) -> f64 {
        self.stage1_ticks as f64 / self.container_ticks
    }

    fn stage2_duty(&self) -> f64 {
        self.stage2_ticks as f64 / self.container_ticks
    }
}

fn run_day(start_unix_s: i64, idle: bool) -> DayReport {
    let cfg = PlantConfig::gw01();
    let containers = cfg.blocks * cfg.containers_per_block;
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 7, start_unix_s);
    if idle {
        sim.set_external_setpoint_w(Some(0.0));
    }
    let weather = gw01_weather();

    let mut report = DayReport {
        max_air_c: f64::NEG_INFINITY,
        max_cell_c: f64::NEG_INFINITY,
        stage1_ticks: 0,
        stage2_ticks: 0,
        heat_ticks: 0,
        starts_per_container: 0.0,
        aux_share_of_import: 0.0,
        container_ticks: 86_400.0 * containers as f64,
    };
    let mut was_running = vec![false; containers];
    let mut starts = 0u64;

    for _ in 0..86_400 {
        sim.step(&weather.inputs_at(sim.unix_time_s()));
        let state = sim.state();
        for (idx, container) in state
            .blocks
            .iter()
            .flat_map(|b| b.containers.iter())
            .enumerate()
        {
            report.max_air_c = report.max_air_c.max(container.air_temp_c);
            match container.hvac.mode {
                HvacMode::Cool1 => report.stage1_ticks += 1,
                HvacMode::Cool2 => report.stage2_ticks += 1,
                HvacMode::Heat => report.heat_ticks += 1,
                HvacMode::Off => {}
            }
            let running = matches!(container.hvac.mode, HvacMode::Cool1 | HvacMode::Cool2);
            if running && !was_running[idx] {
                starts += 1;
            }
            was_running[idx] = running;
        }
        report.max_cell_c = report.max_cell_c.max(state.cell_temp_min_max_c().1);
    }
    report.starts_per_container = starts as f64 / containers as f64;
    let state = sim.state();
    report.aux_share_of_import = state.energy.aux_wh / state.substation.import_wh;
    report
}

/// One July day, computed once and shared: it is the most expensive fixture
/// in the suite and three assertions want it.
fn july_day() -> &'static DayReport {
    static DAY: OnceLock<DayReport> = OnceLock::new();
    DAY.get_or_init(|| run_day(JULY_S, false))
}

/// The regression that justifies this step. Before the HVAC was sized
/// against a container datasheet, a replayed July day left containers at
/// about 36 C air and 42 C cells against a cooling setpoint of 27 C: the
/// plant simply out-produced its cooling. It has to hold now.
#[test]
fn the_summer_peak_day_stays_under_control() {
    let july = july_day();
    // The claim is that the plant no longer out-produces its cooling, not
    // that the peak lands on a particular tenth of a degree. The thresholds
    // sit far enough above the measurement to survive later load work and
    // far enough below the broken behavior (36 C air, 42 C cells) to still
    // catch it.
    assert!(
        july.max_air_c < 32.0,
        "container air peaked at {:.1} C on the July day",
        july.max_air_c
    );
    assert!(
        july.max_cell_c < 41.0,
        "cells peaked at {:.1} C on the July day",
        july.max_cell_c
    );
    assert!(
        july.stage2_ticks > 0,
        "the second cooling unit never ran, so the staging is decoration"
    );
}

/// CALIBRATION.md publishes duty, compressor starts and auxiliary share for
/// this day as measured figures. A calibration record CI cannot falsify is a
/// claim, not a measurement, so the published numbers are held here. The
/// bands are wide enough that ordinary model work does not trip them and
/// narrow enough that a figure drifting out of the record does.
#[test]
fn the_published_calibration_readings_still_hold() {
    let july = july_day();
    let winter = run_day(NEW_YEAR_S, false);

    let july_stage1 = july.stage1_duty() * 100.0;
    let july_stage2 = july.stage2_duty() * 100.0;
    let july_aux = july.aux_share_of_import * 100.0;
    let winter_stage1 = winter.stage1_duty() * 100.0;
    let winter_aux = winter.aux_share_of_import * 100.0;

    assert!(
        (10.0..25.0).contains(&july_stage1),
        "July stage 1 duty {july_stage1:.0}%, recorded as 17%"
    );
    assert!(
        (0.2..5.0).contains(&july_stage2),
        "July stage 2 duty {july_stage2:.1}%, recorded as 1%"
    );
    assert!(
        (15.0..60.0).contains(&july.starts_per_container),
        "July compressor starts {:.1} per container, recorded as 35",
        july.starts_per_container
    );
    assert!(
        (4.0..7.0).contains(&july_aux),
        "July auxiliary share {july_aux:.1}%, recorded as 5.1%"
    );
    assert!(
        (2.0..10.0).contains(&winter_stage1),
        "January stage 1 duty {winter_stage1:.0}%, recorded as 5%"
    );
    assert!(
        (1.8..3.5).contains(&winter_aux),
        "January auxiliary share {winter_aux:.1}%, recorded as 2.6%"
    );
}

/// Sizing is only half of it: a unit that holds temperature by starting
/// every other minute would wear out in a season. The anti short-cycle rule
/// has to show up at plant scale, on a day where cooling runs hard.
#[test]
fn compressors_do_not_short_cycle() {
    let july = july_day();
    assert!(
        july.starts_per_container > 1.0,
        "cooling never cycled at all ({:.1} starts), so this proves nothing",
        july.starts_per_container
    );
    assert!(
        july.starts_per_container < 150.0,
        "{:.1} compressor starts per container in a day is short cycling",
        july.starts_per_container
    );
}

/// The cold end, which only shows on a day the batteries are not warming
/// themselves: the reference installation heats during the battery's standby
/// phase, and so does this one.
#[test]
fn an_idle_winter_day_brings_the_heater_on() {
    let winter = run_day(NEW_YEAR_S, true);
    assert!(
        winter.heat_ticks > 0,
        "an idle container never called for heat through a January day"
    );
}
