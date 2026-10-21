//! The simulation task: owns the kernel, applies control commands, plays
//! the scenario in force ahead of every tick, paces ticks against wall
//! time, and publishes snapshots and events to all surfaces.

mod scenario;

use std::sync::Arc;
use std::time::Duration;

use bess_core::alarms::{AlarmNode, ResetError, ResetScope};
use bess_core::kernel::{CommandError, ControlCommand, Event};
use bess_core::state::ReactiveMode;
use bess_core::{PlantConfig, Simulation, SiteState};
use bess_models::{gw01_models, gw01_weather, HistoricalWeather};
use bess_scenario::{Resolved, RunParams};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::info;

use crate::instruments::{Instruments, ReadingIndex, Readings};
use crate::map::{self, Point};
use crate::tap::Taps;
use scenario::Active;

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
    /// Write the external active-power limit (M4 phase 2): each field
    /// `Some` is written, the others keep their value; a `None` limit
    /// with `enable: Some(false)` clears it.
    SetLimit {
        /// Percent of the rating.
        pct: Option<f64>,
        /// In force or not.
        enable: Option<bool>,
        /// Seconds until it clears itself.
        revert_s: Option<f64>,
        /// Where the answer goes, when the asker wants one.
        reply: Option<oneshot::Sender<Result<(), CommandError>>>,
    },
    /// Write the reactive settings (M4 phase 2): the mode and the values
    /// the modes read, each `Some` written.
    SetReactive {
        /// 0 fixed cos φ, 1 fixed Q, 2 Q(U).
        mode: Option<u16>,
        /// Fixed reactive power, var, positive over-excited.
        q_var: Option<f64>,
        /// Displacement factor, 0 to 1.
        cos_phi: Option<f64>,
        /// Under-excited.
        under_excited: Option<bool>,
        /// Where the answer goes, when the asker wants one.
        reply: Option<oneshot::Sender<Result<(), CommandError>>>,
    },
    /// A command-register command (M4 phase 2): the site reset, the
    /// switchgear, clearing the limit. A refusal is answered and recorded
    /// as an event.
    Control {
        /// The command.
        command: ControlCommand,
        /// Where the answer goes, when the asker wants one.
        reply: Option<oneshot::Sender<Result<bool, CommandError>>>,
    },
    /// Operator reset of the latched alarms in a scope, between two ticks.
    ResetAlarms {
        /// Which words the reset reaches.
        scope: ResetScope,
        /// Where the answer goes.
        reply: oneshot::Sender<ResetReply>,
    },
    /// Start over under a scenario, between two ticks (design D6): a fresh
    /// plant at the schedule's seed and start, at its speed, the log back
    /// at `seq` 1, and no external setpoint, which went with the old plant.
    /// Answers the scenario's status at tick 0, once the switch is made and
    /// the tick 0 snapshot is out.
    LoadScenario {
        /// The schedule, fixed by the parameters the caller resolved.
        resolved: Arc<Resolved>,
        /// Where the status goes.
        reply: oneshot::Sender<Arc<Value>>,
    },
}

/// One published tick: the state tree plus its Modbus projection.
pub struct Snapshot {
    /// Full state tree at this tick.
    pub state: SiteState,
    /// Every point's published value, in map order, as its instrument read
    /// it (`instruments`): what the banks and the topics carry before any
    /// scenario data fault.
    pub measured: Vec<f64>,
    /// The handful of instrumented readings the REST summary and the
    /// metrics name without going through the map.
    pub readings: Readings,
    /// Input register bank.
    pub input_regs: Vec<u16>,
    /// Holding register bank.
    pub holding_regs: Vec<u16>,
    /// Acceleration factor in effect.
    pub speed: f64,
    /// The scenario's status as of this tick, `GET /api/v1/scenario`'s
    /// answer but for the tick, which `state` carries; `None` without a
    /// scenario. The same `Arc` from tick to tick until something fires,
    /// so the per-tick clone stays one pointer.
    pub scenario: Option<Arc<Value>>,
    /// The scenario's data faults as the surfaces read them (`tap.rs`);
    /// empty without a scenario. Replaced when a window opens.
    pub taps: Arc<Taps>,
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

/// Spawn the simulation task for the reference site: a fresh plant at
/// `params`, carrying the scenario's identity and playing its schedule when
/// the command line loaded one.
pub fn spawn(
    params: RunParams,
    scenario: Option<Resolved>,
    instrumented: bool,
) -> (SimHandle, JoinHandle<()>) {
    let cfg = PlantConfig::gw01();
    let models = gw01_models(&cfg);
    let sim = match &scenario {
        Some(resolved) => resolved.simulation(cfg, models),
        None => Simulation::new(cfg, models, params.seed, params.start_unix_s),
    };
    start_with(sim, params.speed, scenario, instrumented)
}

/// Run an already built simulation as the task, under a scenario if one is
/// given, with the instrument inventory on. Tests start from a prepared
/// plant through it.
#[cfg(test)]
pub fn start(
    sim: Simulation,
    speed: f64,
    scenario: Option<Resolved>,
) -> (SimHandle, JoinHandle<()>) {
    start_with(sim, speed, scenario, true)
}

/// `start`, choosing whether the surfaces publish instrumented readings or
/// the kernel's exact values.
pub fn start_with(
    sim: Simulation,
    speed: f64,
    scenario: Option<Resolved>,
    instrumented: bool,
) -> (SimHandle, JoinHandle<()>) {
    let points = Arc::new(map::build_points(sim.config()));
    let instruments = Instruments::new(&points, sim.config(), sim.state().meta.seed, instrumented);
    let readings = ReadingIndex::new(&points);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(64);
    let (events_tx, _) = broadcast::channel(EVENT_BACKLOG);
    let story = scenario.map(|resolved| Active::new(Arc::new(resolved), Arc::clone(&points)));
    let mut task = Task {
        status: story.as_ref().map(|active| Arc::new(active.status())),
        sim,
        speed: speed.clamp(1.0, MAX_SPEED),
        story,
        points: Arc::clone(&points),
        events: events_tx.clone(),
        instruments,
        readings,
    };
    let (snap_tx, snap_rx) = watch::channel(task.snapshot());

    let handle = tokio::spawn(async move {
        let weather = gw01_weather();
        let mut next_tick = Instant::now();
        loop {
            while let Ok(cmd) = cmd_rx.try_recv() {
                task.command(cmd, &snap_tx);
            }
            task.tick(weather, &snap_tx);

            next_tick += Duration::from_secs_f64(1.0 / task.speed);
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
        handle,
    )
}

/// What the task owns between ticks.
struct Task {
    sim: Simulation,
    speed: f64,
    /// The scenario in force, if any.
    story: Option<Active>,
    /// Its status, rebuilt when something fires.
    status: Option<Arc<Value>>,
    points: Arc<Vec<Point>>,
    events: broadcast::Sender<Arc<TickEvents>>,
    /// The meters every published value passes through.
    instruments: Instruments,
    /// Where the summary's handful of readings sit in the map.
    readings: ReadingIndex,
}

impl Task {
    /// Apply one command between two ticks.
    fn command(&mut self, cmd: Command, snap_tx: &watch::Sender<Arc<Snapshot>>) {
        match cmd {
            Command::SetSetpointW(setpoint) => {
                info!(?setpoint, "external setpoint command");
                self.sim.set_external_setpoint_w(setpoint);
            }
            Command::SetSpeed(factor) => {
                self.speed = factor.clamp(1.0, MAX_SPEED);
                info!(speed = self.speed, "speed changed");
            }
            Command::SetLimit {
                pct,
                enable,
                revert_s,
                reply,
            } => {
                let result = self.apply_limit(pct, enable, revert_s);
                info!(
                    ?pct,
                    ?enable,
                    ?revert_s,
                    refused = result.is_err(),
                    "limit command"
                );
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            Command::SetReactive {
                mode,
                q_var,
                cos_phi,
                under_excited,
                reply,
            } => {
                let result = self.apply_reactive(mode, q_var, cos_phi, under_excited);
                info!(?mode, refused = result.is_err(), "reactive command");
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            Command::Control { command, reply } => {
                let result = self.sim.control_command(command);
                info!(
                    command = command.name(),
                    refused = result.is_err(),
                    "control command"
                );
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            Command::ResetAlarms { scope, reply } => {
                let result = self.sim.reset_alarms(scope);
                info!(?scope, refused = result.is_err(), "alarm reset command");
                // The asker may have gone; the reset stands either way.
                let _ = reply.send(result);
            }
            Command::LoadScenario { resolved, reply } => {
                let cfg = self.sim.config().clone();
                let models = gw01_models(&cfg);
                self.sim = resolved.simulation(cfg, models);
                self.speed = resolved.params.speed.clamp(1.0, MAX_SPEED);
                let content_hash = format!("{:#018x}", resolved.id.content_hash);
                info!(
                    scenario = %resolved.id.name,
                    content_hash,
                    seed = resolved.params.seed,
                    speed = self.speed,
                    "scenario loaded; the run starts over at tick 0"
                );
                let active = Active::new(resolved, Arc::clone(&self.points));
                let status = Arc::new(active.status());
                self.story = Some(active);
                self.status = Some(Arc::clone(&status));
                // Every surface sees tick 0 next, before anything fires.
                let _ = snap_tx.send(self.snapshot());
                let _ = reply.send(status);
            }
        }
    }

    /// The limit registers (M4 phase 2): each `Some` written, the rest
    /// kept; an enable of `false` clears the limit, a write of the
    /// percent alone enables it.
    fn apply_limit(
        &mut self,
        pct: Option<f64>,
        enable: Option<bool>,
        revert_s: Option<f64>,
    ) -> Result<(), CommandError> {
        let current = self.sim.state().ems.external_limit;
        if enable.unwrap_or(current.enabled || pct.is_some()) {
            self.sim.set_external_limit(
                pct.unwrap_or(current.pct_of_rated),
                revert_s.unwrap_or(current.revert_s),
            )
        } else {
            self.sim.clear_external_limit();
            Ok(())
        }
    }

    /// The reactive registers (M4 phase 2): the settings a mode does not
    /// read are kept for the bank to read back, so every write goes
    /// through the kernel's mode setter once per setting.
    fn apply_reactive(
        &mut self,
        mode: Option<u16>,
        q_var: Option<f64>,
        cos_phi: Option<f64>,
        under_excited: Option<bool>,
    ) -> Result<(), CommandError> {
        let ems = &self.sim.state().ems;
        let cos_phi = cos_phi.unwrap_or(ems.reactive_cos_phi);
        let under_excited = under_excited.unwrap_or(ems.reactive_under_excited);
        let var = q_var.unwrap_or(ems.reactive_q_var);
        let code = mode.unwrap_or(ems.reactive_mode.code());
        if !(0.0..=1.0).contains(&cos_phi) || !var.is_finite() {
            return Err(CommandError::OutOfRange(
                "cos phi 0 to 1, a finite setpoint",
            ));
        }
        let mode = match code {
            0 => ReactiveMode::FixedCosPhi {
                cos_phi,
                under_excited,
            },
            1 => ReactiveMode::FixedQ { var },
            2 => ReactiveMode::QofU,
            _ => return Err(CommandError::OutOfRange("reactive mode 0, 1 or 2")),
        };
        self.sim.set_reactive_mode(ReactiveMode::FixedCosPhi {
            cos_phi,
            under_excited,
        });
        self.sim.set_reactive_mode(ReactiveMode::FixedQ { var });
        self.sim.set_reactive_mode(mode);
        Ok(())
    }

    /// Fire what the scenario has due, step one tick, publish.
    fn tick(&mut self, weather: HistoricalWeather, snap_tx: &watch::Sender<Arc<Snapshot>>) {
        if let Some(active) = self.story.as_mut() {
            if active.fire_due(&mut self.sim) {
                self.status = Some(Arc::new(active.status()));
            }
        }
        let inputs = weather.inputs_at(self.sim.unix_time_s());
        let events = self.sim.step(&inputs).to_vec();
        // Snapshot first: a surface reacting to an event then reads a
        // snapshot at least as new as the event.
        let _ = snap_tx.send(self.snapshot());
        if !events.is_empty() {
            let count = self.sim.state().event_log.count;
            let batch = TickEvents {
                unix_time_s: self.sim.unix_time_s(),
                first_seq: count + 1 - events.len() as u64,
                events,
            };
            // No subscriber is not an error: the events are in the log.
            let _ = self.events.send(Arc::new(batch));
        }
    }

    fn snapshot(&self) -> Arc<Snapshot> {
        let state = self.sim.state();
        // Every point read once, through its instrument: the one vector the
        // banks, the topics and the summary all publish from.
        let measured: Vec<f64> = self
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| self.instruments.measure(i, state.tick, (p.extract)(state)))
            .collect();
        let mut input_regs = vec![0u16; map::INPUT_BANK_LEN];
        let mut holding_regs = vec![0u16; map::HOLDING_BANK_LEN];
        map::write_banks(&self.points, &measured, &mut input_regs, &mut holding_regs);
        let readings = self.readings.read(&measured);
        Arc::new(Snapshot {
            state: state.clone(),
            measured,
            readings,
            input_regs,
            holding_regs,
            speed: self.speed,
            scenario: self.status.clone(),
            taps: self.story.as_ref().map_or_else(Arc::default, Active::taps),
        })
    }
}
