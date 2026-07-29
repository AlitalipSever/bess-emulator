//! bess-emulator: the native shell.
//!
//! Wraps the deterministic kernel with the surfaces a real plant has:
//! a Modbus TCP slave, an MQTT publisher, a REST control API, and a
//! WebSocket stream. Time control (real time or accelerated) lives here;
//! the kernel cannot tell the difference.

mod cli;
mod http;
mod map;
mod modbus;
mod mqtt;
mod sim;

use std::process::ExitCode;

use clap::Parser;
use tracing::{error, info};

fn main() -> ExitCode {
    let args = cli::Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rumqttc=warn".into()),
        )
        .init();

    if let Some(path) = args.dump_signal_map.clone() {
        return match map::dump_signal_map_csv(&path) {
            Ok(()) => {
                info!("signal map written to {}", path.display());
                ExitCode::SUCCESS
            }
            Err(err) => {
                error!("failed to write signal map: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            error!("failed to start runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("{err}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: cli::Args) -> Result<(), Box<dyn std::error::Error>> {
    let (handle, sim_task) = sim::spawn(&args);

    info!(
        site = "GW-01",
        seed = args.seed,
        speed = args.speed,
        "starting emulator (kernel {})",
        bess_core::version()
    );

    let modbus_task = tokio::spawn(modbus::serve(args.modbus, handle.clone()));
    let http_task = tokio::spawn(http::serve(args.http, handle.clone()));
    let mqtt_task = if args.no_mqtt {
        None
    } else {
        Some(tokio::spawn(mqtt::publish(
            args.mqtt_host.clone(),
            args.mqtt_port,
            handle.clone(),
        )))
    };

    info!("Modbus TCP on {}, HTTP on {}", args.modbus, args.http);

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    modbus_task.abort();
    http_task.abort();
    if let Some(t) = mqtt_task {
        t.abort();
    }
    sim_task.abort();
    Ok(())
}
