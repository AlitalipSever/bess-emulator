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

    /// Publish the kernel's exact values instead of instrumented readings:
    /// no meter class, no noise, no wander on any surface. A research
    /// switch; the default plant measures the way a real one does.
    #[arg(long, default_value_t = false)]
    pub exact_instruments: bool,

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

    /// Replay a measured grid-frequency year compiled by `compile-frequency`
    /// from Energy-Charts responses (M4 phase 1) instead of the synthetic
    /// series. The file is yours, under the source's terms; a headless
    /// record names its hash.
    #[arg(long, value_name = "PATH")]
    pub frequency: Option<PathBuf>,

    /// Replay a year of German balancing tender results compiled by
    /// `compile-balancing` from regelleistung.net's files (M4 phase 3)
    /// in place of the bundled annual figures. The file is yours; a
    /// headless record names its hash.
    #[arg(long, value_name = "PATH")]
    pub balancing: Option<PathBuf>,

    /// Replay a per-second aFRR request year compiled by `compile-afrr`
    /// (M4 phase 3) in place of the synthetic request. The file is yours;
    /// a headless record names its hash.
    #[arg(long, value_name = "PATH")]
    pub afrr: Option<PathBuf>,

    /// Write the Modbus/MQTT signal map as CSV to this path and exit.
    #[arg(long, value_name = "PATH")]
    pub dump_signal_map: Option<PathBuf>,
}
