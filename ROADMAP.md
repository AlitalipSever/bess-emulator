# Roadmap

Order of work, not a schedule. Milestones are sequenced ("now / next / later");
dates are deliberately absent.

## Method

gw-emulator grows as a walking skeleton: the whole plant runs end to end from
the first milestone, with every layer at its simplest useful depth, and each
iteration deepens exactly one module. An iteration is done when all three hold:

1. One module got deeper (interfaces stay stable, no two modules at once).
2. One calibration target was hit against public field data and recorded in
   CALIBRATION.md.
3. One releasable artifact shipped.

Realism is never claimed, always measured: round-trip efficiency, auxiliary
share, failure-type distributions, and revenue must land in publicly
documented fleet bands.

## Milestones

### M0: Walking skeleton (now)

Everything stubbed, everything connected.

- 1-RC equivalent-circuit cells + OCV curve, lumped thermal, day-ahead dispatch
  over real prices
- Modbus TCP + MQTT surfaces, Docker image, minimal signal map (~100 points)
- 3D scene attached to the state tree; GL plumbing migrated to `glow`; egui
  panel skeleton
- Foundation decisions that cannot be retrofitted: checkpoint format, physics
  invariant tests in CI (energy conservation, SoC bounds, meter monotonicity),
  dataset licensing policy (bundle vs fetch script)
- First public release gate: COMPATIBILITY.md, health endpoint + Prometheus
  metrics, `examples/` clients (Python + TypeScript)
- Calibration: energy balance consistent; initial RTE in the 87-90% band

### M1: Thermal + weather

- Real historical weather drives HVAC, auxiliary consumption, container thermal
  behavior
- Calibration: annual RTE lands in the documented field band (80-85%),
  auxiliary share realistic

### M2: BMS + alarm tree + scenario engine

- Rack-level limits, derating, balancing, alarm chains
- Fault injection v1: dropouts, frozen values, timestamp drift, alarm storms
- Scenario types beyond faults: calendar events (DST days), planned
  maintenance / partial availability
- Calibration: failure type and frequency distribution follows the EPRI
  failure incident taxonomy

### M3: PCS + electrical

- Partial-load and voltage-dependent efficiency, P/Q capability, startup state
  machine, setpoint response, transformer losses, breaker state machines
- Calibration: efficiency surfaces match the public Sandia/CEC inverter
  database

### M4: EMS + market signals

- Balancing-market activation replay from public data, setpoint tracking,
  availability reporting, external curtailment commands
- Calibration: annual revenue lands in public fleet index bands

### M5: Degradation

- Empirical cycle + calendar aging, "play five years in five minutes" mode
  (built on kernel checkpointing)
- Calibration: capacity fade inside published LFP field curves

### M6+ (later, no promises)

- OPC UA surface
- Cell-group granularity (~5x signal count, Modbus map split across unit IDs)
- Native desktop viewer (same Rust view layer via glow)

## Out of scope

See the non-goals section in [ARCHITECTURE.md](ARCHITECTURE.md): waveform-level
power electronics, load flow, protection relay internals, multi-busbar
topologies, IEC 61850, plugin systems.
