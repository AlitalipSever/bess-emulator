//! The simulation task: owns the kernel, applies control commands, paces
//! ticks against wall time, and publishes snapshots to all surfaces.

use std::sync::Arc;
use std::time::Duration;

use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, SyntheticWeather};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::info;

use crate::cli::Args;
use crate::map::{self, Point};

/// Highest allowed acceleration factor.
pub const MAX_SPEED: f64 = 3600.0;

/// Control commands from the surfaces into the simulation task.
#[derive(Debug, Clone, Copy)]
pub enum Command {
    /// Set (`Some`) or clear (`None`) the external site setpoint, W.
    SetSetpointW(Option<f64>),
    /// Change the acceleration factor.
    SetSpeed(f64),
}

/// One published tick: the state tree plus its Modbus projection.
pub struct Snapshot {
    /// Full state tree at this tick.
    pub state: SiteState,
    /// Input register bank.
    pub input_regs: Vec<u16>,
    /// Holding register bank.
    pub holding_regs: Vec<u16>,
    /// Acceleration factor in effect.
    pub speed: f64,
}

/// Cloneable handle the surfaces use to observe and control the simulation.
#[derive(Clone)]
pub struct SimHandle {
    /// Latest snapshot (updated every tick).
    pub snapshot: watch::Receiver<Arc<Snapshot>>,
    /// Command channel into the simulation task.
    pub commands: mpsc::Sender<Command>,
    /// The signal map shared by Modbus, MQTT, and the CSV reference.
    pub points: Arc<Vec<Point>>,
}

/// Spawn the simulation task.
pub fn spawn(args: &Args) -> (SimHandle, JoinHandle<()>) {
    let cfg = PlantConfig::gw01();
    let points = Arc::new(map::build_points(&cfg));
    let models = gw01_models(&cfg);
    let mut sim = Simulation::new(cfg, models, args.seed, args.start_unix);

    let mut speed = args.speed.clamp(1.0, MAX_SPEED);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(64);
    let (snap_tx, snap_rx) = watch::channel(make_snapshot(&sim, &points, speed));

    let task_points = Arc::clone(&points);
    let task = tokio::spawn(async move {
        let weather = SyntheticWeather::default();
        let mut next_tick = Instant::now();
        loop {
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    Command::SetSetpointW(setpoint) => {
                        info!(?setpoint, "external setpoint command");
                        sim.set_external_setpoint_w(setpoint);
                    }
                    Command::SetSpeed(factor) => {
                        speed = factor.clamp(1.0, MAX_SPEED);
                        info!(speed, "speed changed");
                    }
                }
            }

            let inputs = weather.inputs_at(sim.unix_time_s());
            sim.step(&inputs);
            let _ = snap_tx.send(make_snapshot(&sim, &task_points, speed));

            next_tick += Duration::from_secs_f64(1.0 / speed);
            let now = Instant::now();
            if next_tick > now {
                tokio::time::sleep_until(next_tick).await;
            } else if now - next_tick > Duration::from_secs(1) {
                // Fell behind by more than a wall second (laptop slept,
                // debugger paused): resynchronize instead of bursting.
                next_tick = now;
            }
        }
    });

    (
        SimHandle {
            snapshot: snap_rx,
            commands: cmd_tx,
            points,
        },
        task,
    )
}

fn make_snapshot(sim: &Simulation, points: &[Point], speed: f64) -> Arc<Snapshot> {
    let mut input_regs = vec![0u16; map::INPUT_BANK_LEN];
    let mut holding_regs = vec![0u16; map::HOLDING_BANK_LEN];
    map::write_banks(points, sim.state(), &mut input_regs, &mut holding_regs);
    Arc::new(Snapshot {
        state: sim.state().clone(),
        input_regs,
        holding_regs,
        speed,
    })
}
