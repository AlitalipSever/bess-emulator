# M2: BMS, alarms, and the scenario engine

Status: draft for review, pre-implementation. ROADMAP.md defines the goal
and the calibration gate; this folder fixes the architecture, the decisions,
and the work breakdown, one phase per subfolder. The layout follows
[docs/design/README.md](../README.md), introduced with this milestone.

## Goal

The plant learns to misbehave, on demand and reproducibly. After M2 a
consumer polling the plant sees derating that follows temperature, alarms
with real causality behind them, blocks that go into maintenance, a site
that trips and comes back in a staggered sequence, and, when a scenario asks
for it, telemetry that lies the way real telemetry lies while the physics
underneath keeps running correctly.

## What M2 changes

Following the M1 pattern: capabilities first, then the one module that
deepens.

1. **Events become a surface.** The kernel emits events since M0, but only
   PCS transitions, and nothing publishes them. M2 gives the event stream
   consumers: MQTT report-by-exception, Modbus alarm registers with meaning,
   an event counter. The `event` publication class stops being a design-table
   entry and becomes code.
2. **The scenario engine.** A new crate, `bess-scenario`, turns a YAML file
   into a reproducible deviation from the happy path. The determinism tuple
   (seed, scenario, dataset) finally has its middle member.
3. **The module that deepens is the BMS**, behind the existing `BmsLogic`
   trait: temperature derating, a rack imbalance quantity with passive
   balancing, and the rack alarm word.

Two adjacent slices ride along because the alarm tree is hollow without
them, both deliberately minimal: from the protection domain, only the trip
and the staggered return (the single-line skeleton stays in M3); from cell
variation, only a per-rack spread statistic (per-cell state stays out, per
the non-goals). The SoC realism chain (OCV hysteresis, reported versus true
SoC, measurement imperfections) is deliberately not here: it is M2.5, a
mini-iteration in the M0.5/M1.5 tradition, designed in
[docs/design/m2_5/](../m2_5/).

## Phases

| Phase | Theme | Depends on |
|---|---|---|
| [1: BMS](phase-1-bms/) | temperature derating, rack spread, passive balancing | nothing |
| [2: Alarms](phase-2-alarms/) | alarm words, latching, events published on every surface | 1 (conditions to alarm on) |
| [3: Availability and calendar](phase-3-availability-calendar/) | maintenance, isolation, protection trip and staggered return, real prices, local time, DST | 2 (trip is an alarm) |
| [4: Scenario engine](phase-4-scenario-engine/) | `bess-scenario`, YAML schema, physical fault injection, CLI and REST | 2, 3 (the actions it triggers) |
| [5: Data faults and the library](phase-5-data-faults-and-library/) | shell-side fault tap, `scenarios/` library, EPRI gate, CI assertion mode, release | 4 |

Phases land in order. A phase is done when its PRs are merged, its tests are
green in CI, and its documents record what was actually built.

## Calibration gate

Injected failure types and frequencies follow the public EPRI BESS Failure
Incident Database taxonomy: balance-of-system and controls dominant, cells
rare. The gate is enforced as a distribution check over the scenario
library's taxonomy tags, with the database snapshot (shares and retrieval
date) pinned in CALIBRATION.md by phase 5. The M1 annual gates (round-trip
band, waterfall identity) keep running; derating and real prices will move
the annual numbers, so the committed record is regenerated and the move
explained in CALIBRATION.md.

## Versioning

- **Crate:** v0.5.0 at milestone end.
- **Signal map:** 0.2.0 to 0.3.0, one bump at release. This is the first of
  the two breaking changes COMPATIBILITY.md has announced since M1: the rack
  alarm bits stop reading zero and gain a documented layout, which
  reinterprets a published point. New points (block and site alarm words,
  the event counter, the rack spread `cell_dv_mv` and derate status,
  block mode, and the revenue-meter period points) are additions; the
  availability points already exist at inputs 8 and 10, and only their
  values change.
- **Checkpoint:** format 3 bumps as schema-changing PRs land (spread and
  balancing state in phase 1, alarm latches in phase 2, block mode and
  the sequencer then the revenue meter in phase 3's two PRs, scenario
  position in phase 4). Same rule as M1: bump when the schema changes,
  reject old files by version, no migration pre-1.0.

## Release-note inventory for v0.5.0

Filled as PRs land. Every entry is a change someone integrating against
v0.4.0 can trip over.

- **Rack limits now depend on cell temperature** (phase 1 PR1). A rack
  above 55 C loses power in both directions and none flows above 60 C;
  a rack below 15 C charges slower and none charges below 0 C. On the
  replayed reference year this never binds against dispatch, so v0.4.0
  annual figures are unchanged; an integrator driving cells out of the
  band (external setpoints on a cold site, a future HVAC-loss scenario)
  will see limits drop that never dropped before.

## Open questions

- ~~Which public 314 Ah class LFP datasheet pins the derating
  thresholds.~~ Resolved in phase 1 PR1: EVE MB31.
- Whether GW-01's rack rating (1C, an M0 parameter) should follow the
  pinned cell's 0.5P continuous rating. Phase 1 carries the table's shape
  only; aligning the rating changes full-power capability and the annual
  record, so it needs its own decision (see CALIBRATION.md, M2 derating).
- Final alarm bit layout of the three alarm words (phase 2 design proposes,
  its PR1 freezes).
- Day-ahead price source and license for the 2024 reference year, SMARD
  versus ENTSO-E transparency (phase 3 PR2).
- EPRI database snapshot: exact category shares and retrieval date
  (phase 5).
- Whether the WASM shell gets scenario loading in M2 or later. This
  README owns the question; the current lean is later, and the crate is
  WASM-clean either way.
