# M2 phase 4: scenario engine, architecture

What exists after this phase: a YAML file is a complete, reproducible
description of a run, including everything that goes wrong in it. The
determinism tuple (seed, scenario, dataset) gains its middle member for
real.

## The crate

`bess-scenario`, the crate the repository layout has reserved since M0:
schema types, parser, validation, and the player. It depends on
`bess-core` (to name fault actions and validate targets against the plant
topology) and on nothing else heavier than serde and the YAML parser. No
IO beyond parsing bytes it is handed, no clock, no threads: the same
purity rules as the kernel, which is also what keeps it WASM-clean for a
later browser iteration.

## Two injection sites, one file

The architecture has fixed this split since M0 and the crate honors it:

- **Physical faults execute in the kernel.** The player resolves a due
  event to a `FaultAction` (a new `bess-core` enum: HVAC failure and
  repair, PCS trip, protection trip, rack self-discharge, rack isolate
  and restore, block maintenance enter and exit, alarm reset) and hands
  it to the simulation, which applies it as a state change at the tick
  boundary. From that point the consequences emerge from the models; the
  scenario caused the HVAC failure, the physics produces the hot
  container.
- **Data faults execute in the shells.** The player only schedules them;
  the shell-side tap that corrupts projections is phase 5. The schema
  carries both kinds from the start so the library format does not churn
  between phases.

## The player

A deterministic cursor over a sorted event list. Time in scenarios is
expressed as offsets from scenario start, resolved to tick indices at
load; the player fires everything due at a tick before the kernel steps.
Any randomness a fault parameter allows (a self-discharge rate given as a
range) is drawn at load time from a PRNG stream seeded by (seed, event
index), never at fire time, so the schedule is fully determined before
the first tick and checkpoint resume cannot double-draw.

Player position is part of the checkpoint: a resumed run continues the
scenario mid-story, which is the point of checkpoints.

## The surfaces

- **CLI:** `--scenario path.yaml`. The file is authoritative for seed,
  start and speed when it states them; conflicting explicit flags are
  rejected rather than silently overridden, because a reproduction that
  quietly ran with different parameters is worse than an error.
- **REST:** `POST /api/v1/scenario` loads a scenario (resetting the
  simulation to its start), `GET /api/v1/scenario` reports the active
  scenario, its position, fired and pending events. This is the "load
  scenario" control surface ARCHITECTURE.md has described since M0,
  existing at last.
- **Kernel API:** `apply(action: FaultAction)` on the simulation, public,
  so kernel tests and phase 2/3 integration tests script faults without a
  YAML detour. The scenario engine is a front end to the same door.

## Validation

A scenario is rejected at load, with line and reason, when a target path
does not resolve against the plant topology, a fault does not apply to
its target's kind, times are not monotone per target, or the schema
version is unknown. The schema carries a version field from day one;
scenario files are going to outlive milestones, and the library in
phase 5 becomes a compatibility surface of its own.

## Invariants

- Same (seed, scenario, dataset): byte-identical output, event log
  included, with and without a checkpoint round-trip mid-scenario.
- A loaded scenario with zero events is exactly the plain run: the engine
  costs nothing when idle.
- Every fired physical-fault action appears in the kernel event log
  (scenario actions are events too). Data-fault firings deliberately do
  not: they execute in the shells, and phase 5's core property is that
  the kernel cannot tell. Their trace lives in the scenario status
  surface, so a full run is still readable back from the outputs, each
  fault class from the side that fired it.
