//! The simulation task: owns the kernel, applies control commands, paces
//! ticks against wall time, and publishes snapshots and events to all
//! surfaces.

use std::sync::Arc;
use std::time::Duration;

use bess_core::alarms::{AlarmNode, ResetError, ResetScope};
use bess_core::kernel::Event;
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::info;

use crate::cli::Args;
use crate::map::{self, Point};

/// Highest allowed acceleration factor.
pub const MAX_SPEED: f64 = 3600.0;

/// Ticks with events a surface may fall behind by before it starts losing
/// them. A quiet plant emits a few hundred events a simulated day, so this
/// covers days at full speed; a surface that still falls behind is told how
/// many batches it lost, and the event numbers show the gap.
const EVENT_BACKLOG: usize = 4096;

/// What an operator reset answers: the bits whose cause is still present,
/// or why the reset was refused.
pub type ResetReply = Result<Vec<(AlarmNode, u8)>, ResetError>;

/// Control commands from the surfaces into the simulation task.
#[derive(Debug)]
pub enum Command {
    /// Set (`Some`) or clear (`None`) the external site setpoint, W.
    SetSetpointW(Option<f64>),
    /// Change the acceleration factor.
    SetSpeed(f64),
    /// Operator reset of the latched alarms in a scope, between two ticks.
    ResetAlarms {
        /// Which words the reset reaches.
        scope: ResetScope,
        /// Where the answer goes.
        reply: oneshot::Sender<ResetReply>,
    },
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

/// The events one tick emitted, numbered by their place in the kernel's log.
#[derive(Debug)]
pub struct TickEvents {
    /// Simulation time of the snapshot that shows their effect, unix
    /// seconds: the timestamp telemetry from that snapshot carries.
    pub unix_time_s: i64,
    /// Log number of the first event, counting from 1.
    pub first_seq: u64,
    /// The events, in the order the kernel emitted them.
    pub events: Vec<Event>,
}

impl TickEvents {
    /// Each event with its log number.
    pub fn numbered(&self) -> impl Iterator<Item = (u64, &Event)> {
        (self.first_seq..).zip(&self.events)
    }
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
    /// Every tick's events, in order; `subscribe` for a receiver. Unlike
    /// the snapshot, which only ever holds the latest tick, this loses
    /// nothing a surface keeps up with.
    pub events: broadcast::Sender<Arc<TickEvents>>,
}

/// Spawn the simulation task for the reference site.
pub fn spawn(args: &Args) -> (SimHandle, JoinHandle<()>) {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    start(
        Simulation::new(cfg, models, args.seed, args.start_unix),
        args.speed,
    )
}

/// Run an already built simulation as the task. Tests start from a prepared
/// plant through it.
pub fn start(mut sim: Simulation, speed: f64) -> (SimHandle, JoinHandle<()>) {
    let points = Arc::new(map::build_points(sim.config()));
    let mut speed = speed.clamp(1.0, MAX_SPEED);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(64);
    let (snap_tx, snap_rx) = watch::channel(make_snapshot(&sim, &points, speed));
    let (events_tx, _) = broadcast::channel(EVENT_BACKLOG);

    let task_points = Arc::clone(&points);
    let task_events = events_tx.clone();
    let task = tokio::spawn(async move {
        let weather = gw01_weather();
        let mut next_tick = Instant::now();
        loop {
            while let Ok(cmd) = cmd_rx.try_recv() {
                apply(&mut sim, cmd, &mut speed);
            }

            let inputs = weather.inputs_at(sim.unix_time_s());
            let events = sim.step(&inputs).to_vec();
            // Snapshot first: a surface reacting to an event then reads a
            // snapshot at least as new as the event.
            let _ = snap_tx.send(make_snapshot(&sim, &task_points, speed));
            if !events.is_empty() {
                let count = sim.state().event_log.count;
                let batch = TickEvents {
                    unix_time_s: sim.unix_time_s(),
                    first_seq: count + 1 - events.len() as u64,
                    events,
                };
                // No subscriber is not an error: the events are in the log.
                let _ = task_events.send(Arc::new(batch));
            }

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
            events: events_tx,
        },
        task,
    )
}

fn apply(sim: &mut Simulation, cmd: Command, speed: &mut f64) {
    match cmd {
        Command::SetSetpointW(setpoint) => {
            info!(?setpoint, "external setpoint command");
            sim.set_external_setpoint_w(setpoint);
        }
        Command::SetSpeed(factor) => {
            *speed = factor.clamp(1.0, MAX_SPEED);
            info!(speed, "speed changed");
        }
        Command::ResetAlarms { scope, reply } => {
            let result = sim.reset_alarms(scope);
            info!(?scope, refused = result.is_err(), "alarm reset command");
            // The asker may have gone; the reset stands either way.
            let _ = reply.send(result);
        }
    }
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
