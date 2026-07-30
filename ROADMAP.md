# Roadmap

This roadmap describes the order of work and the quality gate each step must
pass. It deliberately contains no dates: milestones are sequenced, not
scheduled. Architecture and rationale live in
[ARCHITECTURE.md](ARCHITECTURE.md).

## Status at a glance

| Milestone | Theme | Status |
|---|---|---|
| M0 | Walking skeleton, end to end | done (v0.1.0) |
| M1 | Thermal + weather | next |
| M2 | BMS, alarms, scenario engine | planned |
| M3 | PCS + electrical | planned |
| M4 | EMS + market signals | planned |
| M5 | Degradation | planned |
| M6+ | OPC UA, cell granularity, native viewer | ideas, no promises |

## How this roadmap works

**Walking skeleton.** The whole plant runs end to end from M0, with every layer
at its simplest useful depth. Each milestone deepens exactly one module;
interfaces stay stable, and no two modules are reworked at once.

**Definition of done.** A milestone is complete only when all three hold:

1. **Module:** the targeted module got deeper, behind its existing trait.
2. **Calibration gate:** a stated realism target was met against public field
   data and recorded in CALIBRATION.md. Realism is never claimed, always
   measured.
3. **Artifact:** a tagged release shipped, with docs updated.

**Calibration sources.** All gates reference public data only: the EPRI
failure incident database (failure taxonomy), CAISO/EPRI fleet reports (field
round-trip efficiency), the Sandia/CEC inverter database (efficiency
surfaces), published LFP aging studies (capacity fade), and public fleet
revenue indices for the German market.

**Versioning.** Pre-1.0, minor versions may break anything; breaking changes
are called out in release notes. Once the signal map is published, its
stability is governed separately by COMPATIBILITY.md (adding registers is
minor, moving addresses is major).

---

## M0: Walking skeleton (done, v0.1.0)

**Goal:** everything stubbed, everything connected. A plant you can run,
poll, and watch within a minute, honest about its simplicity.

Scope:

- **Kernel and state tree:** typed site tree for GW-01 (substation, EMS,
  weather, 20 blocks, containers, racks), fixed 1 s tick, seeded PRNG,
  checkpoint format designed now (it cannot be retrofitted later).
- **Models, simplest useful versions:** 1-RC equivalent-circuit cell + OCV
  curve from public datasheets, lumped container thermal, day-ahead dispatch
  plan over real historical prices, flat-efficiency PCS, transformer loss
  constants.
- **Surfaces:** Modbus TCP slave and MQTT publisher over a minimal signal map
  (~100 points), REST control (load scenario, set speed), WebSocket stream.
- **View layer foundation:** GL plumbing migrated to `glow`, 3D scene attached
  to the state tree, egui panel skeleton (one working panel is enough here).
- **Quickstart:** `docker compose up` to a live Grafana SoC chart in under 60
  seconds. The quickstart is a first-class product surface, not an
  afterthought.
- **Engineering foundations:** physics invariants as CI property tests
  (energy conservation, SoC bounds, meter monotonicity; a physics violation
  fails the build), a golden-snapshot determinism test (same seed + scenario +
  dataset = byte-identical output), and the dataset licensing policy decided
  and recorded in DATA-LICENSES.md (restricted sources ship as fetch scripts,
  never bundled).

First public release additionally requires: COMPATIBILITY.md, health endpoint
+ Prometheus metrics, `examples/` clients (Python + TypeScript: connect, read,
write a setpoint).

**Calibration gate:** energy balance consistent across a full simulated day;
initial round-trip efficiency in the 87-90% band (losses present, but thermal
and auxiliary effects not yet modeled).

**Out of scope here:** realistic thermal behavior, alarms beyond a stub, fault
injection, market signals beyond day-ahead dispatch.

## M1: Thermal + weather

**Goal:** the plant starts feeling weather, and the efficiency story becomes
honest.

Scope:

- Real historical weather (temperature, irradiance) drives container thermal
  behavior and HVAC duty
- HVAC model with staged operation and its auxiliary power draw
- Standby and night-time auxiliary consumption visible in the meters
- Thermal coupling into cell temperature (groundwork for derating in M2)

**Calibration gate:** annual round-trip efficiency lands in the documented
field band (80-85%, versus the 87-92% brochure band), and the auxiliary share
of throughput is realistic. The gap between nameplate and field efficiency
must be explainable component by component (conversion losses, transformer,
HVAC, standby).

## M2: BMS, alarms, and the scenario engine

**Goal:** the plant learns to misbehave, on demand and reproducibly.

Scope:

- Rack-level BMS: charge/discharge limits, temperature and SoC derating,
  passive balancing, alarm chains with realistic causality
- Fault injection v1, in two distinct classes:
  - physical faults inside the kernel (HVAC failure, PCS trip, protection
    trip, abnormal self-discharge)
  - data faults at the protocol layer (communication dropouts, frozen values,
    timestamp drift, unit errors, NaN bursts, alarm storms, restart backfill),
    while the physics underneath keeps running correctly
- Scenario types beyond faults: calendar events (DST days with 23/25 hours and
  92/100 quarter-hour market periods) and planned maintenance / partial
  availability (a block down, racks isolated)
- Scenario library in `scenarios/`, each file a reproducible YAML case
- CI assertion mode: run a scenario headless, compare against a snapshot,
  exit nonzero on drift

**Calibration gate:** injected failure types and frequencies follow the public
EPRI failure incident taxonomy (controls and balance-of-system dominant, cells
rare).

## M3: PCS + electrical

**Goal:** the electrical path stops being a constant and starts being a
character in the causal chain.

Scope:

- Two-dimensional PCS efficiency map f(P, V_dc) with partial-load behavior
- SoC-dependent power limits (fixed current limit against SoC-dependent DC
  voltage)
- Operating state machine: standby, precharge, contactor close, synchronize,
  ramp; a protection trip takes the site offline and blocks return in a
  staggered sequence
- Setpoint response: dead time, ramp limits, first-order settling
- Thermal derating from converter temperatures; short-term overload budget
- Substation depth: breaker/disconnector interlocks, transformer thermal
  model, OLTC tap behavior visible in voltage steps
- Separate 15-minute revenue meter series alongside SCADA telemetry

**Calibration gate:** efficiency surfaces match public Sandia/CEC inverter
database curves; the M1 round-trip efficiency gate still holds with the new
electrical losses in place.

## M4: EMS + market signals

**Goal:** the plant behaves like a market participant, and external control
becomes fully testable.

Scope:

- Balancing-market activation replay from public German market data
- Setpoint tracking quality and availability reporting
- External curtailment/redispatch commands: a written power limit is obeyed
  and flagged as an external limitation in telemetry
- Grid-code behaviors: P(f) droop response against replayed real grid
  frequency, Q(U)/cos-phi reactive support including at zero active power
- Control surface hardening: everything a dispatch application needs to write
  (setpoints, modes) exercised end to end over Modbus

**Calibration gate:** simulated annual revenue mix and magnitude land inside
public German fleet index bands.

## M5: Degradation

**Goal:** time becomes a simulated quantity; five years in five minutes.

Scope:

- Empirical cycle and calendar aging from published LFP data
- Accelerated multi-year runs built on kernel checkpointing
- Pre-aged plant presets (start from a year-5 plant)
- SoH trajectories per rack, capacity and resistance fade visible in telemetry

**Calibration gate:** capacity fade trajectories inside published LFP field
study bands.

## M6+ (ideas, explicitly unpromised)

- OPC UA surface
- Cell-group granularity (~5x signal count; Modbus map split across unit IDs
  per container, matching real BMS gateway topologies)
- `high_res` profile (100 ms fast class for frequency-response analysis)
- Native desktop viewer (same Rust view layer via glow)
- Community-requested scenarios and calibration targets

## Cross-cutting workstreams

These advance alongside every milestone rather than belonging to one:

- **Reference signal map (`refmodel/`):** grows with each module, versioned
  with semver, governed by COMPATIBILITY.md from first publication. Built only
  from public sources (SunSpec models, IEC 61850-7-420 naming, public vendor
  manuals).
- **Datasets (`bess-data`):** every source's license and redistribution status
  recorded in DATA-LICENSES.md before it enters the repo; restricted sources
  ship as fetch scripts.
- **CALIBRATION.md:** regenerated by `bess-bench` at every release; the
  running proof behind the realism claim.
- **Documentation:** ARCHITECTURE.md and examples updated in the same PR as
  the change; a milestone with stale docs is not done.

## Out of scope

Permanently out of scope, with reasons, in the non-goals section of
[ARCHITECTURE.md](ARCHITECTURE.md): waveform-level power electronics, load
flow, protection relay internals, multi-busbar topologies, IEC 61850, plugin
systems, and invented power-quality signals.

## Feedback

Once the repository is public: propose changes or additions through issues.
Scenario requests and calibration-source suggestions are especially welcome;
"you asked, we measured" is how this roadmap is meant to evolve.
