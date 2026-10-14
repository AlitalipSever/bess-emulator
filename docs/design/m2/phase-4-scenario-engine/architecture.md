# M2 phase 4: scenario engine, architecture

What exists after this phase: a YAML file is a complete, reproducible
description of a run, including everything that goes wrong in it. The
determinism tuple (seed, scenario, dataset) gains its middle member for
real.

Pinned on 2026-10-14 against the tree as phase 3 left it (PR #19 merged:
checkpoint format 7, map 0.3.0, the kernel's between-ticks door in
`kernel/commands.rs` with five commands and the reset in
`kernel/alarms.rs`, `Event` with three variants, `SiteMeta` carrying site
id, seed and start). Phases 2 and 3 built every operation a physical fault
needs except one; this phase builds the one, names them all under a single
enum, and gives the enum a file format, a player and two doors.

## The crate

`bess-scenario`, the crate the repository layout has reserved since M0:
schema types, the parser, validation against the plant topology, the
resolved schedule and the player. It depends on `bess-core` (to name fault
actions as `FaultAction`, validate targets against `PlantConfig`, draw
from the kernel's own `Rng`, and share the FNV-1a digest), on `bess-data`
for its `calendar` module alone (the `start` timestamp is a civil date,
and the workspace already refused a second copy of that arithmetic in
phase 3), on serde and the YAML parser, and on nothing else. No IO beyond
parsing bytes it is handed, no clock, no threads: the same purity rules as
the kernel, which is also what keeps it WASM-clean for a later browser
iteration. The bundled price and weather artifacts come along with
`bess-data` as they already do for every crate above `bess-models`; the
cost is binary size, which no consumer of this crate is sensitive to.

What the crate is not: a runtime. It never steps a simulation. It turns
bytes into a `Schedule` and hands a shell the actions that are due at a
tick; the shell owns the loop, exactly as it owns the loop today.

## Two injection sites, one file

The architecture has fixed this split since M0 and the crate honors it:

- **Physical faults execute in the kernel.** The player resolves a due
  event to a `FaultAction` (a `bess-core` enum: HVAC failure and repair,
  PCS trip, protection trip, rack self-discharge, rack isolate and
  restore, block maintenance enter and exit, alarm reset) and hands it to
  `Simulation::apply`, which applies it between ticks as the commands
  already do. From that point the consequences emerge from the models; the
  scenario caused the HVAC failure, the physics produces the hot
  container. `apply` is a dispatcher over the existing commands, not a
  second implementation of them: every variant but one calls a method
  phase 2 or 3 already tested.
- **Data faults execute in the shells.** The player only schedules them;
  the shell-side tap that corrupts projections is phase 5. The schema
  carries both kinds from the start so the library format does not churn
  between phases. In this phase a data fault that comes due is marked
  fired on the status surface and has no effect, and the documentation
  says so.

## The one new piece of physics

`rack: self_discharge` is the only fault no earlier phase built (phase 3
PR1 settled why: it is a cell fault, not an availability one). It is one
new field on `RackState`, an excess self-discharge rate that is zero on a
healthy rack, and one new term in the spread law of
`bess-models/src/bms/balance.rs`: the time term of `cell_dsoc` grows by
the healthy string's rate plus the rack's excess. One cell leaking faster
than its neighbours widens the rack's spread over days, outruns the bleed
at the top of the window, and surfaces through the phase 2 imbalance bits
(warning at 120 mV, trip at 500 mV, both already in the rack word). The
fault's state change is the rate; the consequences stay the BMS model's.
There is no repair action: a leaking cell is replaced, and that is M5
material; the operator's answer in M2 is `rack: isolate`.

## The player

A deterministic cursor over a schedule. Time in scenarios is expressed as
offsets from scenario start and resolved to tick indices at load (a tick
is one second, so the resolution is the offset in seconds). The player
fires everything due at a tick before the kernel steps that tick: an
event `at: PT0S` applies before the first step, which is exactly how the
phase 2 chain test prepares its plant.

Any randomness a fault parameter allows (a self-discharge rate given as a
range) is drawn at load time from an `Rng` stream seeded by (seed, event
index), never at fire time, so the schedule is fully determined before
the first tick and a checkpoint resume cannot double-draw. The schedule is
therefore a pure function of (file bytes, seed, plant configuration), and
the player's position is a pure function of the schedule and the current
tick: everything with a tick below the current one has fired. Nothing
about the player needs to be in the checkpoint; what the checkpoint
carries is the scenario's identity, so a resumed run can refuse a
different file (design D8, revised from the draft, which wanted the
cursor in the tree as well).

## The kernel door

`Simulation::apply(FaultAction) -> Result<bool, FaultError>`, public, so
kernel tests and the phase 2 and 3 integration tests script faults
without a YAML detour; the scenario engine is a front end to the same
door. `true` means the plant changed, `false` that it was already in the
requested state (a second trip of a tripped PCS, a repair of a working
unit), as `trip_pcs` and `trip_protection` already answer. A node the site
does not have is refused before anything changes, through the
`CommandError` and `ResetError` the commands already return.

Every applied action is also an event: `Event::FaultApplied(FaultAction)`
goes out with the next tick, ahead of the consequences it causes, through
`SiteState::pending_events` like a reset's clears. This is what puts the
scenario under the determinism contract: the event log's digest covers
what was done to the plant as well as what the plant did, and two runs of
one tuple produce one log. It also gives the one silent fault
(self-discharge, whose consequences take days) a mark in the log on the
tick it happened.

## The surfaces

- **CLI:** `--scenario path.yaml`. The file is authoritative for seed,
  start and speed when it states them; conflicting explicit flags are
  rejected rather than silently overridden, because a reproduction that
  quietly ran with different parameters is worse than an error.
- **REST:** `POST /api/v1/scenario` loads a scenario and resets the
  simulation to its start; `GET /api/v1/scenario` reports the active
  scenario, its resolved schedule, what has fired and what is pending.
  This is the "load scenario" control surface ARCHITECTURE.md has
  described since M0, existing at last.
- **Health:** `/health` gains the scenario's name and content hash, so a
  client reading the kernel version and the map version there also reads
  which story the plant is in.
- **MQTT and the stream:** `FaultApplied` publishes like every other event,
  under the node it acted on. The registers do not change: the plant's
  map does not know it is in a story, which is the point.

The bench does not take a scenario in this milestone. Calibration measures
the plant, not the narrator (phase 5 D6), and the annual record's identity
block stays as it is.

## Validation

A scenario is rejected at load, with position and reason, when the schema
version is unknown, a target path does not resolve against the plant
topology, a fault does not apply to its target's kind or its surface, a
parameter is missing or out of range, or two events on one target are out
of order or share a tick. Syntax and type errors carry the parser's line
and column; semantic rejections name the event by index and field
(`events[3].target`), which is unambiguous in a file the reader has open.
The schema carries a version field from day one; scenario files are going
to outlive milestones, and the library in phase 5 becomes a compatibility
surface of its own.

## What changes where

| Crate | Change |
|---|---|
| `bess-core` | `FaultAction`, `FaultError`, `Simulation::apply` (`kernel/fault.rs`); `Event::FaultApplied` (`Event` moves to `kernel/event.rs`); `RackState::self_discharge_excess_per_s`; `SiteMeta::scenario: Option<ScenarioId>`; `digest::fnv1a_64` shared by the checkpoint, the event log and the scenario hash; checkpoint format 8 |
| `bess-models` | the excess term in `Balancing::step`; the self-discharge rate's provenance in CALIBRATION.md |
| `bess-scenario` | new: schema, durations, target paths, validation, resolution, player |
| `bess-emulator` | `--scenario` and the precedence rule; the player driven from the simulation task; `POST` and `GET /api/v1/scenario`; the health fields; the `FaultApplied` topic and payload |
| `bess-bench` | nothing |
| `bess-scene`, `bess-wasm` | nothing; the crate compiles for `wasm32-unknown-unknown` and the viewer does not load scenarios in M2 |

## Invariants

- Same (seed, scenario, dataset): byte-identical state and event log,
  with and without a checkpoint round-trip mid-scenario.
- A loaded scenario with zero events is exactly the plain run: identical
  trajectory, identical event log, the state tree identical once the
  scenario identity in `SiteMeta` is set aside. The engine costs nothing
  when idle.
- A scenario scripted through `apply` and the same scenario played from
  YAML produce the same event log, and with the `FaultApplied` events
  removed that log is the one the direct commands produced before this
  phase: the engine adds sequencing and a mark, never physics.
- Every fired physical-fault action appears in the kernel event log.
  Data-fault firings deliberately do not: they execute in the shells, and
  phase 5's core property is that the kernel cannot tell. Their trace
  lives in the scenario status surface, so a full run is still readable
  back from the outputs, each fault class from the side that fired it.
- The schedule is fully determined at load; no draw happens at fire time.
- A checkpoint taken under one scenario refuses to resume under another
  (hash mismatch), the same pinned-artifact discipline as the datasets.
