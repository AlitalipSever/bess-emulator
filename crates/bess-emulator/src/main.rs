//! bess-emulator: the native shell.
//!
//! Wraps the deterministic kernel with the surfaces a real plant has:
//! a Modbus TCP slave, an MQTT publisher, a REST control API, and a
//! WebSocket stream. Time control (real time or accelerated) lives here;
//! the kernel cannot tell the difference.

mod cli;
mod events;
#[cfg(test)]
mod fixtures;
mod http;
mod map;
mod modbus;
mod mqtt;
mod sim;
mod tap;

use std::path::Path;
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
    // The file is read and checked before anything binds a socket, so a
    // refused scenario is a process that never started.
    let scenario = match &args.scenario {
        Some(path) => Some(load_scenario(path)?),
        None => None,
    };
    let params = cli::run_params::resolve(&args, scenario.as_ref())?;
    let resolved = scenario.map(|scenario| scenario.resolve(params));
    if let Some(resolved) = &resolved {
        let content_hash = format!("{:#018x}", resolved.id.content_hash);
        info!(
            scenario = %resolved.id.name,
            content_hash,
            events = resolved.schedule.len(),
            "scenario loaded"
        );
    }
    let (handle, sim_task) = sim::spawn(params, resolved);

    info!(
        site = "GW-01",
        seed = params.seed,
        start_unix_s = params.start_unix_s,
        speed = params.speed,
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

/// Read and validate a scenario file against the reference site. The error
/// names the file, then the position and the reason as the parser gives
/// them.
fn load_scenario(path: &Path) -> Result<bess_scenario::Scenario, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("cannot read scenario {}: {err}", path.display()))?;
    bess_scenario::load(&bytes, &bess_core::PlantConfig::gw01())
        .map_err(|err| format!("scenario {}: {err}", path.display()))
}
