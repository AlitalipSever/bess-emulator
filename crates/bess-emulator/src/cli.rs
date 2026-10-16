//! Command-line interface.

pub mod run_params;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

/// Synthetic grid-scale battery plant emulator (GW-01: 100 MW / 200 MWh).
///
/// Defaults bind to localhost only. Modbus has no authentication by nature;
/// expose it beyond localhost deliberately, never accidentally.
#[derive(Debug, Parser)]
#[command(name = "bess-emulator", version, about)]
pub struct Args {
    /// PRNG seed; together with the start time it fully determines the run.
    /// Default 42. A scenario that states a seed wins; a flag that
    /// disagrees with it is an error.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Simulation start, unix seconds UTC. Default 1767225600 (2026-01-01
    /// 00:00:00 UTC). A scenario that states a start wins.
    #[arg(long)]
    pub start_unix: Option<i64>,

    /// Time acceleration factor (1 = real time). Default 60: one simulated
    /// day passes in 24 wall-clock minutes. A scenario that states a speed
    /// wins.
    #[arg(long)]
    pub speed: Option<f64>,

    /// Scenario file to play from tick 0 (YAML, schema 1). Its seed, start
    /// and speed, where it states them, are the run's.
    #[arg(long, value_name = "PATH")]
    pub scenario: Option<PathBuf>,

    /// Play the scenario headless, no socket bound, as fast as the machine
    /// allows, for the file's `length`, and write the digest of what it
    /// left to this path: a library case's committed record.
    #[arg(long, value_name = "PATH", requires = "scenario")]
    pub record: Option<PathBuf>,

    /// Play the scenario headless and compare its digest with the record
    /// at this path; exit nonzero on drift. What CI runs over the library.
    #[arg(
        long,
        value_name = "PATH",
        requires = "scenario",
        conflicts_with = "record"
    )]
    pub assert: Option<PathBuf>,

    /// Modbus TCP listen address.
    #[arg(long, default_value = "127.0.0.1:1502")]
    pub modbus: SocketAddr,

    /// HTTP (REST + WebSocket + metrics) listen address.
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub http: SocketAddr,

    /// MQTT broker host to publish telemetry to.
    #[arg(long, default_value = "127.0.0.1")]
    pub mqtt_host: String,

    /// MQTT broker port.
    #[arg(long, default_value_t = 1883)]
    pub mqtt_port: u16,

    /// Disable the MQTT publisher.
    #[arg(long, default_value_t = false)]
    pub no_mqtt: bool,

    /// Write the Modbus/MQTT signal map as CSV to this path and exit.
    #[arg(long, value_name = "PATH")]
    pub dump_signal_map: Option<PathBuf>,
}
