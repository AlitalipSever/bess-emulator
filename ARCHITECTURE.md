# Architecture

This document describes how gw-emulator is built and why. For what is planned
and in which order, see [ROADMAP.md](ROADMAP.md).

## Overview

gw-emulator is a synthetic grid-scale battery plant. One deterministic Rust
simulation core drives a fictional but internally consistent 100 MW / 200 MWh
plant. Everything observable from the outside, telemetry, alarms, control
responses, is produced by a closed-loop simulation and exposed through the same
kinds of surfaces a real site has.

```
                        ┌─────────────────────────────────────────┐
 Real historical data   │                gw-core                  │
 day-ahead prices  ──┐  │  ┌───────────────────────────────────┐  │
 weather           ──┼─►│  │ Kernel: step(state, inputs)       │  │
 grid frequency    ──┘  │  │ fixed tick, seeded PRNG, no IO    │  │
 (compiled offline      │  └───────────────┬───────────────────┘  │
  by gw-data)           │                  │                      │
                        │        State tree (single truth)        │
                        └──────────┬───────┴────────┬─────────────┘
                                   │  projections   │
              ┌────────────────────┼────────────────┼──────────────────┐
              ▼                    ▼                ▼                  ▼
        Modbus map           MQTT topics       in-memory read     Parquet/Arrow
     (SunSpec-based ref)    (tree paths)       (browser UI)       (research)
              │                    │                │                  │
   ┌──────────┴────────────────────┴───────┐  ┌─────┴──────────┐  ┌────┴──────┐
   │  gw-simd (native binary / Docker)     │  │ gw-wasm        │  │ gw-bench  │
   │  Modbus TCP slave, MQTT publisher,    │  │ (browser)      │  │ (CLI,     │
   │  REST control, WebSocket              │  │ kernel + scene │  │ calibration│
   │  audience: EMS/pipeline engineers     │  │ in one WASM    │  │ + CI)     │
   └───────────────────────────────────────┘  │ 3D + panels,   │  └───────────┘
                                              │ all Rust       │
                                              └────────────────┘
```

One core, multiple shells. The core is pure computation; all IO lives in the
shells.

## The reference plant: GW-01

GW-01 is a fictional site, fixed as the single supported configuration:

- 100 MW / 200 MWh (2h), LFP cells (314 Ah class), 1500 V DC
- 20 power blocks, each: one 5 MW PCS + two 5 MWh containers
- 110 kV grid connection with its own substation (main transformer, HV breaker,
  protection signals)
- Located in northeastern Germany, so that three real public data sources align
  on one site: day-ahead prices, weather, and grid frequency

The topology is generated from a site descriptor, but exactly one official site
is shipped. A fixed reference plant keeps the signal map, the scenarios, the 3D
scene, and the documentation all telling the same story.

## Design principles

**Model to the interface, not to the physics.** Every layer is exactly as deep
as the behavior observable at its boundary (what SCADA, Modbus, or an operator
would see), and no deeper. Nobody polling a Modbus register can tell a full
electrochemical model from a well-tuned equivalent circuit; the register is the
truth we are accountable to.

**Signals emerge, they are not scripted.** There is no per-signal script. Each
tick, the layers push on each other with real-plant causality: a hot afternoon
raises HVAC load, if cooling falls behind cell temperatures climb, the BMS
derates, the PCS misses its setpoint, and SCADA raises a power limitation flag.
That chain is produced, not authored. This is the difference between a signal
generator and a plant emulator.

**Real data drives the loop.** Market prices, weather, and grid frequency are
replayed from real historical records, not modeled. The emulator's July 14th is
the actual July 14th.

**Determinism is a contract.** The tuple (seed, scenario, dataset version)
fully determines the output, byte for byte. This enables golden-snapshot tests
in CI and copy-pasteable bug reproductions.

**Calibrated realism.** "Looks realistic" is not a claim we make; it is a
number we publish. Yearly simulation KPIs (round-trip efficiency, auxiliary
share, revenue) must land inside publicly documented fleet bands. Results live
in CALIBRATION.md (added with the first calibrated release). We never emit a
signal we cannot calibrate; an absent signal is honest, an invented one is not.

## gw-core: the kernel

The kernel is a pure function: `step(state, tick_inputs) -> (state', events)`.
No wall clock, no IO, no threads; randomness only from a seeded PRNG.

- **Tick:** fixed 1 s simulation step. Power flow within a tick is quasi-static.
  Publication layers decimate per signal on top of this.
- **Time control lives in the shells:** real time (1x), accelerated (Nx), or
  as-fast-as-possible (CI and calibration). The kernel cannot tell the
  difference.
- **External world as data:** prices, weather, frequency, and balancing
  activations enter as pre-compiled, versioned time series. Fetching and
  parsing happen offline in gw-data.
- **Checkpointing:** kernel state is serializable. This enables "play five
  years, continue from year five", pre-aged plant presets, and shareable
  reproductions (snapshot + scenario + seed).
- **Physics invariants in CI:** energy conservation, SoC bounds, and meter
  monotonicity are enforced with property-based tests on every commit. A
  physics violation fails the build.

## Plant layers

The plant hierarchy mirrors a real site, each layer behind a Rust trait so it
can be deepened independently:

| Trait | Initial implementation | Upgrade path |
|---|---|---|
| `CellModel` | 1-RC equivalent circuit + OCV-SoC curve from public datasheets | 2-RC, temperature-dependent parameters |
| `ThermalModel` | Lumped container heat model coupled with HVAC | Rack-level gradients, HVAC staging |
| `BmsLogic` | Limits, derating, alarm tree, passive balancing | Behavior profiles from public vendor manuals |
| `PcsModel` | Efficiency map f(P, V_dc), P/Q capability, state machine, setpoint response | Thermal derating, overload budget, STATCOM mode |
| `EmsStrategy` | Day-ahead dispatch plan over real prices + setpoint tracking | Balancing-market activation replay, multi-market |
| `GridInterface` | Transformer losses, breaker state machine, real frequency replay, revenue meter | OLTC, transformer thermal, Q(U)/cos-phi, curtailment commands, P(f) droop |
| `Aging` | Empirical cycle + calendar curves from published data | Stress-factor models |

Signals originate in the layer they belong to. A BMS alarm comes from the rack
layer, an efficiency loss from the PCS layer, a curtailment flag from the EMS
layer. This is what keeps the data causally consistent, not just plausible in
isolation.

### Substation and grid side

Same principle: quasi-static P/Q/V/f, modeled to what SCADA shows. Included:
point-of-interconnection measurements with replayed real grid frequency,
transformer losses (a component of field round-trip efficiency), breaker and
disconnector state machines with interlocks (a protection trip takes the whole
site offline and it returns in a staggered sequence), and a separate 15-minute
revenue meter series alongside SCADA telemetry.

### Power electronics, at the seconds scale

Waveform-level power electronics is out of scope (see non-goals), but its
second-scale shadows are very much in scope: SoC-dependent power limits (fixed
current limit, SoC-dependent DC voltage), a two-dimensional efficiency map
sourced from the public Sandia/CEC inverter database, startup sequences
(precharge, contactor close, synchronize, ramp), setpoint response with dead
time and ramp limits, standby and night-time auxiliary losses, and reactive
support at zero active power.

## State tree: the single contract

The entire plant state is one addressable tree:

```
site/
  meta
  substation/   (110 kV: main transformer, HV breaker, protection, POI P/Q/V/f, meter)
  ems/          (mode, active plan, setpoints, availability)
  weather/      (from real data: temperature, irradiance, wind)
  block[0..19]/
    pcs/        (state, P, Q, efficiency, temperatures, alarms)
    container[0..1]/
      hvac/  thermal/
      rack[0..N]/   (SoC, SoH, V, I, T, alarms, balancing)
```

Every node has a fixed schema: value, unit, type, source layer, publication
class, access (RO/RW). Every external surface is a projection of this tree:

- **Modbus register map:** generated deterministically from the tree
  (SunSpec-802-style blocks plus extensions), published as versioned CSV/JSON.
  This artifact is the reference model itself. Its stability contract lives in
  COMPATIBILITY.md: adding registers is minor, moving addresses is major, with
  a defined deprecation process. If people wire their CI to this map, the map
  is an API and is managed like one.
- **MQTT topics:** tree paths.
- **Browser UI:** reads the tree directly from Rust memory (see shells).
- **Parquet/Arrow export:** the flattened tree as time series.

Writable nodes (EMS setpoints, HVAC mode, breakers) define the control surface:
a dispatch application writes a setpoint over Modbus and the plant responds.
That closed loop is what makes the emulator a test target rather than a data
generator.

### Signal resolution model

Resolution is defined per signal in three dimensions:

**Time (publication classes).** The kernel computes everything at 1 Hz; the
surfaces decimate per class. Decimation is sample-and-hold, matching real SCADA
behavior (including its aliasing patterns).

| Class | Examples | MQTT | Modbus refresh | Rationale |
|---|---|---|---|---|
| fast | POI P/Q/V/f, PCS power/setpoints | 1 s | every tick | TSO prequalification and FCR metering use 1 s |
| medium | SoC, rack V/I/T, HVAC | 10 s + on-change with deadband | 10 s | typical BMS-to-SCADA reporting |
| slow | energy meters, SoH, availability | 60 s | 60 s | historian practice; meters monotonic |
| event | alarms, state transitions | immediate (report by exception) | alarm registers + event counter | alarm storms are only realistic event-driven |
| static | nameplate, config, model version | retained | fixed blocks | discovery |

**Amplitude.** Each signal carries an LSB (quantization step), a scale factor
(16-bit Modbus with SunSpec-style scale factors), a seeded noise sigma, and a
range clamp. Even float-capable surfaces round to the LSB: in the field, data
is produced by sensors, not by formats.

**Timestamps.** Every measurement has a source timestamp (device clock) and a
server timestamp (SCADA receive time). Device clocks drift slowly and
independently; scenarios can amplify the drift (NTP failure). Clock skew is one
of the most common real-world pipeline traps, so it is a first-class feature.

**Scale.** GW-01 yields roughly 10,000 signal points. The kernel computes all
of them every tick (~10k values/s, fully written in the research profile);
the default SCADA profile publishes ~1,200 values/s. Cell-group granularity
(later) multiplies this by ~5 and splits the Modbus map across unit IDs per
container, which matches real per-container BMS gateway topologies anyway.

## Scenario engine

Scenarios are YAML files. Two fault classes are injected at two different
places, deliberately:

- **Physical faults (inside the kernel):** HVAC failure, PCS trip, protection
  trip, abnormal cell self-discharge. Frequency and type distributions follow
  the public EPRI failure incident taxonomy.
- **Data faults (in the shell, at the protocol layer):** communication
  dropouts, frozen values, timestamp drift, unit errors, NaN bursts, alarm
  storms, restart backfill. The physics keeps running correctly; only the
  observation is corrupted, exactly as in reality.

Beyond faults: calendar events (DST days with 23/25 hours and 92/100
quarter-hour market periods, a classic pipeline killer that replay makes almost
free) and planned maintenance / partial availability (a block in maintenance,
racks isolated), because a plant where everything always works reads as fake.

```yaml
seed: 42
day: 2026-07-14        # real prices + real weather from that day
speed: 60x
events:
  - at: "14:00", target: block[2].container[0].hvac, fault: failure
  - at: "14:20", surface: mqtt, fault: dropout, duration: 20m
  - at: "14:40", surface: mqtt, fault: backfill_burst
```

Scenario file + seed = reproduction. In CI:
`gw-simd --scenario s.yaml --assert snapshot.json`.

## Shells

**gw-simd (native, Docker).** Modbus TCP slave, MQTT publisher, REST control
API (load scenario, change speed, query status), WebSocket stream. The Docker
compose bundle includes Grafana with a ready dashboard; the target is a live
SoC chart within 60 seconds of `docker compose up`. The emulator is itself
observable: health endpoint, structured logs, Prometheus metrics. Safe
defaults: ports bind to localhost; MQTT auth is available; Modbus has no auth
by nature and the README says so plainly.

**gw-wasm (browser).** gw-core and gw-scene compile into a single WASM module.
No protocols, no JS bridge for data: the scene and panels read the state tree
from Rust memory. Datasets for selected days ship as static files. A full
simulation with zero installation.

**gw-scene (the view layer, all Rust).** The entire product UI lives in one
canvas: a 3D instanced-rendering scene of the site plus egui panels for BMS,
EMS, PCS, and substation views (precedent: the Rerun viewer). The DOM is only a
thin HTML shell. The GL plumbing targets `glow`, so the same view layer can run
in the browser (WebGL2) and natively (OpenGL). Clicking geometry selects a tree
node and opens its panel; the scene is a view over the state tree and never
touches the kernel directly.

**gw-bench (CLI).** Runs long simulations as fast as possible, extracts KPIs
(annual RTE, auxiliary share, cycles, revenue), and compares them against
public fleet bands. Its output is CALIBRATION.md.

## Repository layout

```
gw-emulator/
  crates/
    gw-core/       kernel + state tree + layer traits
    gw-models/     default model implementations
    gw-scenario/   YAML scenarios + fault injection
    gw-data/       dataset compilers (prices, weather, frequency, activations)
    gw-proto/      Modbus map generator + MQTT projection
    gw-scene/      view layer: 3D scene + egui panels
    gw-simd/       native shell
    gw-wasm/       browser entry: gw-core + gw-scene in one WASM module
    gw-bench/      calibration CLI
  refmodel/        published signal map (JSON + CSV, semver)
  scenarios/       scenario library (EPRI taxonomy + calendar + maintenance)
  examples/        minimal Python + TypeScript clients (connect, read, write a setpoint)
  ARCHITECTURE.md  this file
  ROADMAP.md       order of work
  CALIBRATION.md   gw-bench output (arrives with the first calibrated release)
  COMPATIBILITY.md register map stability contract (arrives with the first public map)
  DATA-LICENSES.md license and redistribution status of every bundled dataset
```

Datasets with restricted redistribution are never bundled; gw-data ships a
fetch script instead, and DATA-LICENSES.md records the status of each source.

## Non-goals

Deliberately not built, with reasons:

- **Load flow, short circuit, harmonics, EMT.** Microsecond physics needs a
  different kind of kernel (and would end browser and CI performance), its
  results cannot be validated without proprietary vendor control models, and
  mature certified tools own that domain. We model the second-scale
  consequences that SCADA actually shows.
- **Fault-ride-through waveforms.** Certification happens in accredited labs
  with vendor firmware; a software approximation has no evidentiary value. The
  operational consequence (dip event, ride-through or trip, alarms, recovery)
  is modeled.
- **Protection relay internals.** Real relay behavior is defined by
  project-specific proprietary setting files. A generic relay model would be
  confidently wrong; a black box that trips from scenarios or simple thresholds
  produces identical observable data and is honest about what it is.
- **Multi-busbar topologies.** Typical BESS sites are single-bus radial. One
  configuration keeps the schema, map, scenes, and docs coherent.
- **IEC 61850.** The reference implementation is GPL-licensed and the
  open-source demand signal is near zero. Modbus TCP and MQTT first, OPC UA
  later.
- **A plugin system.** Layer traits are the extension mechanism. Generic
  frameworks get designed after at least two real uses demand them, not
  before.
- **Invented power-quality signals (THD etc.).** We cannot calibrate them, so
  we do not emit them.
