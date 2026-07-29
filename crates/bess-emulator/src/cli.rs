//! Command-line interface.

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
    #[arg(long, default_value_t = 42)]
    pub seed: u64,

    /// Simulation start, unix seconds UTC (default 2026-01-01 00:00:00 UTC).
    #[arg(long, default_value_t = 1_767_225_600)]
    pub start_unix: i64,

    /// Time acceleration factor (1 = real time). One simulated day passes
    /// in 24 wall-clock minutes at the default of 60.
    #[arg(long, default_value_t = 60.0)]
    pub speed: f64,

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
