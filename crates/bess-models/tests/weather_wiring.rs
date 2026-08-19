//! The tick inputs reach the plant, as the quantities they claim to be.
//!
//! The driver tests pin the numbers a weather year produces and the thermal
//! tests pin what a container does with a given sky. Between them sits the
//! kernel, and a swap there (irradiance where ambient belongs, or a slice
//! that never arrives) leaves both of those suites green: the energy balance
//! does not care which scalar drives the envelope, and the only other
//! witness is the golden digest, which the workflow tells you to update when
//! a change is deliberate. This file is the witness that cannot be waved
//! through.

use bess_core::kernel::Weather;
use bess_core::{Inputs, PlantConfig, Simulation};
use bess_models::{gw01_models, gw01_weather};

/// 2026-01-01 00:00:00 UTC.
const START_UNIX_S: i64 = 1_767_225_600;

/// Hold the plant idle and expose it to one fixed sky. Idle on purpose: with
/// the racks dispatching, their heat is an order of magnitude larger than
/// anything the weather does and would mask exactly the wiring under test.
fn run_with(weather: Weather, ticks: u64) -> Simulation {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 42, START_UNIX_S);
    sim.set_external_setpoint_w(Some(0.0));
    let inputs = Inputs {
        weather,
        grid_frequency_hz: 50.0,
    };
    for _ in 0..ticks {
        sim.step(&inputs);
    }
    sim
}

fn mean_air_temp_c(sim: &Simulation) -> f64 {
    let mut sum = 0.0;
    let mut n = 0u32;
    for block in &sim.state().blocks {
        for container in &block.containers {
            sum += container.air_temp_c;
            n += 1;
        }
    }
    sum / f64::from(n)
}

/// Irradiance has to arrive as irradiance: same ambient, different sky, and
/// the containers must end up warmer under the sun.
#[test]
fn sunshine_reaches_the_containers() {
    let ambient_c = 15.0;
    let dark = run_with(
        Weather {
            ambient_c,
            irradiance_wm2: 0.0,
        },
        3600,
    );
    let sunlit = run_with(
        Weather {
            ambient_c,
            irradiance_wm2: 900.0,
        },
        3600,
    );
    let gain_k = mean_air_temp_c(&sunlit) - mean_air_temp_c(&dark);
    println!("an hour of full sun: {gain_k:.2} K on container air");
    assert!(
        gain_k > 1.0,
        "an hour of full sun moved container air by only {gain_k} K"
    );
}

/// Ambient has to arrive as ambient: same sky, colder outside, colder inside.
#[test]
fn ambient_reaches_the_containers() {
    let sky = |ambient_c| Weather {
        ambient_c,
        irradiance_wm2: 0.0,
    };
    let cold = run_with(sky(-5.0), 4 * 3600);
    let warm = run_with(sky(35.0), 4 * 3600);
    let spread_k = mean_air_temp_c(&warm) - mean_air_temp_c(&cold);
    println!("four hours at -5 C versus 35 C: {spread_k:.2} K on container air");
    assert!(
        spread_k > 5.0,
        "a 40 K ambient swing moved the containers by only {spread_k} K"
    );
}

/// What the driver produced is what the state tree reports, field for field.
/// The state weather is the projection every surface reads (Modbus
/// `site.weather.*`, the viewer panel), so a stale or reordered copy here
/// would misreport the plant while the physics ran on something else.
#[test]
fn the_state_tree_reports_the_inputs_it_ran_on() {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, 42, START_UNIX_S);
    let driver = gw01_weather();
    // A midsummer noon, where ambient and irradiance are far apart in
    // magnitude and a swap would be obvious.
    for _ in 0..10 {
        let inputs = driver.inputs_at(sim.unix_time_s());
        sim.step(&inputs);
        assert_eq!(sim.state().weather, inputs.weather);
    }
    let noon = START_UNIX_S + 195 * 86_400 + 12 * 3600;
    let inputs = driver.inputs_at(noon);
    sim.step(&inputs);
    assert_eq!(sim.state().weather, inputs.weather);
    assert!(
        sim.state().weather.irradiance_wm2 > 100.0,
        "the sample must actually carry sunlight, or it proves nothing"
    );
}
