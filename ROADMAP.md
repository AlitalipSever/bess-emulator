# Roadmap

This roadmap describes the order of work and the quality gate each step must
pass. It deliberately contains no dates: milestones are sequenced, not
scheduled. Architecture and rationale live in
[ARCHITECTURE.md](ARCHITECTURE.md).

## Status at a glance

| Milestone | Theme | Status |
|---|---|---|
| M0 | Walking skeleton, end to end | done (v0.1.0) |
| M0.5 | PCS partial-load efficiency curve (pulled forward from M3) | done (v0.2.0) |
| M1 | Thermal + weather | done (v0.3.0) |
| M1.5 | View mini-iteration: real solar position, weather-driven scenery, time controls | done (v0.4.0) |
| M2 | BMS, alarms, scenario engine | planned |
| M3 | PCS + electrical | planned |
| M4 | EMS + market signals | planned |
| M5 | Degradation | planned |
| M6+ | Grid protocols (IEC 60870-5-104, 61850), OPC UA, cell granularity, native viewer | ideas, no promises |

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

**Calibration sources.** All gates reference public data only. Used so far:
the Sandia/CEC inverter database for the conversion efficiency curve, EIA
Form EIA-923 fleet data and the NREL Annual Technology Baseline for the field
round-trip band, DWD station observations for weather, and a container
manufacturer's published project data for the HVAC. Planned: the EPRI failure
incident database (failure taxonomy), published LFP aging studies (capacity
fade), and public fleet revenue indices for the German market. A gate names
the source it actually used, never the source it was expected to use.

**Versioning.** Pre-1.0, minor versions may break anything; breaking changes
are called out in release notes. Once the signal map is published, its
stability is governed separately by COMPATIBILITY.md (adding registers is
minor, moving addresses is major).

**What earns 1.0.** In this project 1.0 is a promise rather than a badge: it
is the release where the stability contract in COMPATIBILITY.md takes effect
and addresses stop moving. It is therefore a test, not a decision, and three
things have to be true at once. The signal map has to survive M2 and M3
without an address moving or an enum value changing meaning. The checkpoint
format has to hold across two consecutive milestones. And no layer may still
be running on an unsourced placeholder.

None of the three holds today, and two are known not to: M2 gives meaning to
alarm bits that currently read zero, and M3 replaces a three-value PCS state
with a five-state machine. Both are major changes under the contract above,
which is exactly why they happen before 1.0 rather than after it.

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

## M0.5: PCS partial-load efficiency curve (done, v0.2.0)

A mini-iteration pulled forward from M3, decided 2026-08-10. It respects the
iteration discipline: exactly one module (PCS) got deeper, the `PcsModel`
trait did not change, and M1's thermal scope is untouched. Only the
partial-load dimension moved here; the V_dc dimension, reactive capability,
the operating state machine, and setpoint dynamics all stay in M3.

Scope:

- `CurvePcs` beside `FlatPcs`: one-way conversion loss
  `L(p) = (k0 + k1 p + k2 p^2) P_rated` with `p = |P_ac| / P_rated`, so
  `eta(p) = p / (p + k0 + k1 p + k2 p^2)`. The GW-01 model bundle switches
  to `CurvePcs`.
- New telemetry point `blockNN.pcs.efficiency_pct` (register base+7, minor
  change per COMPATIBILITY.md; derived from existing state, so the
  checkpoint format is unchanged).

**Calibration gate:** the fitted k0/k1/k2 reproduce the efficiency curve of a
real 1500 V utility-scale storage inverter (Sungrow SC2500UD-US, Sandia
coefficients from the CEC inverter database as distributed with NREL SAM)
within 0.3 percentage points at all six CEC load points, enforced as a CI
test. The M0 round-trip gate is re-measured with the curve in place and
recorded in CALIBRATION.md.

## M1: Thermal + weather (done, v0.3.0)

**Goal:** the plant starts feeling weather, and the efficiency story becomes
honest.

Detailed design: [docs/design/m1-thermal-weather.md](docs/design/m1-thermal-weather.md).

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

Measured by `bess-bench` over the full replayed year and recorded in
[CALIBRATION.md](CALIBRATION.md), with the band's floor sourced from the
measured EIA fleet average and its ceiling from the NREL Annual Technology
Baseline. The component-by-component requirement is met by the loss
waterfall: eight categories, each on its own meter, summing to what the POI
meters say crossed it.

## M1.5: View mini-iteration (done, v0.4.0)

A mini-iteration in the M0.5 pattern, decided 2026-08-19. It deepens the view
layer only; no kernel model changes, so M2's scope is untouched.

Detailed design: [docs/design/m1_5-view-scenery.md](docs/design/m1_5-view-scenery.md).

The principle is the one already stated in `bess-scene/src/sun.rs`: every
visual effect is driven either by a measured series or by pure mathematics on
position and time. No invented decoration. The boundary that makes this safe
stays where it is, in that the core physics never reads the scene, and the
only authority in the energy accounting remains the measured irradiance.

Scope:

- **Real solar position.** The current monthly sunrise/sunset table and
  synthetic arc give way to a solar position algorithm taking azimuth and
  elevation from UTC plus latitude and longitude, with the site fixed at the
  reference station. Dependency-free, so the browser build stays unaffected.
  Held by golden instants from a public calculator and by physical bounds.
- **Cloud and dimming.** Measured irradiance over a clear-sky expectation
  drives how bright the sun reads; the observed cloud series drives sky
  color. The two cross-check each other.
- **Precipitation and wind.** The precipitation series drives particle
  density, rain against snow follows the observed precipitation type, and
  wind speed and direction drive the drift.
- **Time controls.** A date picker that restarts the scenario on the chosen
  day, and a fast-forward that computes to a target date without rendering,
  keeping state continuity. No rewind: going back means restarting there.
  Plus stops computed from the dataset, such as the warmest day of the
  reference year.
- **Panels.** Weather and thermal signals in the egui panels: ambient,
  irradiance, container air, cell temperature, HVAC stage.

**Gate:** no calibration gate. This iteration adds no physics, so it is held
by the existing determinism and invariant tests plus the golden instants
above. A view iteration that needed a calibration gate would mean the scene
had started deciding something.

Shipped as v0.4.0 rather than a v0.3.x patch: the iteration is source-breaking
in four places, and the M0.5 precedent gives a mini-iteration a minor bump.
The kernel-side gates all came back unmoved, which was the point of running
them here: the signal map is byte-identical and the annual figures return to
the digit, so nothing leaked out of the view layer.

What it did not do, and one of them was found by looking rather than by
testing: the scene had projected its shadows along a hardcoded direction
since M0, so making the sun real did not move them until someone opened the
plant and said so. Every test in that crate asks whether a number is right;
none can ask whether anything is visible. A view layer's last gate is a
person looking at it.

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

- The V_dc dimension of the PCS efficiency map (the partial-load dimension
  shipped in M0.5)
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

- **Grid-facing protocol surfaces (IEC 60870-5-104, IEC 61850).** Modbus and
  MQTT cover the plant-internal and monitoring cases. They are not what a
  TSO-facing SCADA integration speaks, which is the one interface the emulator
  cannot currently stand in for. Two very different amounts of work, so they
  are listed separately rather than as one bucket:
  - *IEC 60870-5-104 slave:* tractable natively. APCI and ASDU framing over
    TCP, a small set of type IDs (short-float measured value, single point,
    single command, setpoint), and the existing signal map already supplies
    what becomes the information object addresses. No third-party dependency,
    so this repo's permissive licensing is unaffected. This is the one to do
    first.
  - *IEC 61850:* an MMS server plus SCL description is a project of its own,
    and the established C libraries are GPL with commercial dual-licensing,
    which does not fit an MIT/Apache repo. The realistic first step is naming
    rather than transport: the reference signal map already aligns with
    IEC 61850-7-420 logical-node naming, and publishing that mapping
    explicitly delivers most of the practical value to an integrator for a
    fraction of the work.
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
- **CALIBRATION.md:** its measured blocks regenerated by `bess-bench`, its
  parameter provenance written by hand; the running proof behind the realism
  claim, and CI fails when a block goes stale.
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
