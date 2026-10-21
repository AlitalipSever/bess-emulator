# bess-emulator

A synthetic grid-scale battery plant (BESS) emulator.

bess-emulator simulates a realistic 100 MW / 200 MWh battery plant, from cells and
battery management up to power conversion, plant control, and the grid connection,
and exposes it the way a real site does: as live telemetry behind industrial
protocol surfaces. Point your EMS, SCADA integration, monitoring platform, or data
pipeline at it and develop against a plant you can actually reach.

Think of it as a local, controllable stand-in for a battery plant:

- **Behavioral realism.** Signals are not scripted; they emerge from a closed-loop
  simulation (dispatch -> converter limits -> battery management -> cell state ->
  thermal), driven by real historical market prices, weather, and grid frequency.
- **Deterministic.** Same seed, same scenario, same dataset: byte-identical output.
  Built for CI regression tests and reproducible bug reports.
- **Fault and scenario injection.** Communication dropouts, frozen values, alarm
  storms, timestamp drift, protection trips, maintenance outages: the data your
  pipeline meets in production, on demand, from a YAML file, with the failure
  mix held to the public incident record.
- **Runs anywhere.** One Rust core; ships as a Docker service with protocol
  endpoints, and as a fully client-side browser build with an explorable 3D view
  of the plant.

## Quickstart

With Docker:

```sh
docker compose up
```

Then open <http://localhost:3000> (Grafana): the GW-01 dashboard shows a live
state-of-charge chart within a minute, plus the plant's temperatures against
the replayed weather, what its cooling is doing, the house load item by item,
and where the energy that never reached the meter went. The plant runs at 60x
by default, so a full market day plays out in 24 minutes.

Without Docker (Rust toolchain required):

```sh
cargo run --release -p bess-emulator
```

Then make something go wrong:

```sh
cargo run --release -p bess-emulator -- --scenario scenarios/hvac-failure-both-units.yaml
```

Block 2 loses both its cooling units on the hottest afternoon of the year,
under full discharge: within minutes the derate bits raise, the block
misses its setpoint and the site reports its limit, on MQTT, on Modbus, in
the stream and on `GET /api/v1/scenario`, which narrates the story. The
[scenario library](scenarios/README.md) has fifteen such cases, each a
reproducible YAML file with its taxonomy tag and its committed digest;
`--help` lists the seed, start and speed flags, which a file's own values
win over, and `--assert` plays a case headless against its record.

## Surfaces

| Surface | Where | What |
|---|---|---|
| Modbus TCP | `127.0.0.1:1502` | Input registers: telemetry. Holding registers: control (site setpoint, EMS mode). |
| MQTT | broker at `127.0.0.1:1883` | Topics under `bess/gw01/`, JSON payloads, decimated per publication class; alarm words retained and published on change; every alarm raise, clear and PCS transition under `bess/gw01/events/`. |
| REST | `http://127.0.0.1:8080/api/v1/` | `state`, `summary`, `setpoint`, `speed`, `alarms/reset`, `scenario` (load a file, read the story's progress). |
| WebSocket | `ws://127.0.0.1:8080/api/v1/stream` | Tick summaries, 4 per second, with the events since the last one. |
| Prometheus | `http://127.0.0.1:8080/metrics` | Site KPIs, temperatures, HVAC staging, house load by item, cumulative losses by category, cell spread, active alarms and the event count, the fiscal (revenue) meter, and the day-ahead price the planner acts on. |
| Health | `http://127.0.0.1:8080/health` | Liveness, kernel and map versions, the scenario in play. |

The full register and topic reference is
[refmodel/gw01-signal-map.csv](refmodel/gw01-signal-map.csv); its stability
rules live in [COMPATIBILITY.md](COMPATIBILITY.md).

Drive the plant from your own code:

```sh
# Charge at 30 MW (negative = charge, positive = discharge):
curl -X POST localhost:8080/api/v1/setpoint \
     -H 'content-type: application/json' -d '{"watts": -30000000}'

# Hand control back to the internal day-ahead plan:
curl -X POST localhost:8080/api/v1/setpoint \
     -H 'content-type: application/json' -d '{"mode": "plan"}'

# Run a day in 24 seconds:
curl -X POST localhost:8080/api/v1/speed \
     -H 'content-type: application/json' -d '{"factor": 3600}'

# Reset latched alarms on block 2 (the answer lists what is still present):
curl -X POST localhost:8080/api/v1/alarms/reset \
     -H 'content-type: application/json' -d '{"scope": "block", "block": 2}'
```

Complete clients (connect, read, write a setpoint) live in
[examples/python](examples/python) and [examples/typescript](examples/typescript).

**A note on exposure.** Modbus has no authentication by design of the
protocol, and the sandbox MQTT broker allows anonymous access. All defaults
bind to localhost; if you expose these ports beyond your machine, that is a
deployment decision with the same implications it has for real plant
equipment.

## The 3D view

The product UI is written entirely in Rust (a 3D site scene plus egui
panels over one `glow` context) and runs both natively and in the browser
from the same code. Try it in a window:

```sh
cargo run --release -p bess-scene --features sim --example viewer
```

Orbit with the mouse (right-drag or shift-drag pans, scroll zooms), switch
to fly mode to move freely over the site (WASD + QE, shift = fast), and
click a container, a PCS skid, or the transformer to open its panel. The
side panel lists the active alarms by node and severity and the journal
of what raised and cleared; a row selects its object in the scene, and
the scene shows severity where the cause lives. The scenario library is
compiled in: pick a case, start it, and follow it on a progress strip,
running to the next event or to the end. The browser
build is `wasm-pack build crates/bess-wasm --target web`; the npm
package is built and published by `scripts/publish-npm.sh`, which renames it
from the crate name to the product name on the way out.

A page that embeds the browser build can play the library itself:

```js
const handle = new WebHandle();
await handle.start(canvas);
handle.start_scenario("pcs-trip-and-reset");
const status = JSON.parse(handle.scenario_status()); // GET /api/v1/scenario's shape
```

`scenarios()` lists the cases (it works before `start`), and
`load_scenario_yaml(text)` plays a file of the page's own.
[COMPATIBILITY.md](COMPATIBILITY.md) states the rules for these methods.

## Status

Pre-alpha, and measured rather than described. Seven milestones have shipped:
M0, the walking skeleton, as v0.1.0; the M0.5 mini-iteration, a partial-load
conversion efficiency curve fitted to the CEC inverter database, as v0.2.0;
M1, thermal and weather, as v0.3.0; the M1.5 view mini-iteration as
v0.4.0, which put M1 on the screen; M2, BMS, alarms and the scenario
engine, as v0.5.0; the M2.6 view mini-iteration as v0.6.0, which put
M2's alarms and its scenario library on the screen, in the browser too
(v0.6.1 re-timed three library cases so they show what their headers
say); and the M2.5 SoC realism mini-iteration as v0.7.0, which took three
kinds of truth off the published surfaces: a rested LFP rack now carries
the hysteresis of where it has been, the published SoC is what the BMS
estimates rather than what the kernel knows, and every published value
passes through an instrument with an IEC class; and M3, PCS and
electrical, as v0.8.0, which gave the electrical path its own physics
and its own order of events: the converter converts on a surface at the
racks' voltage, walks real startup states, follows a setpoint through a
ramp, gets warm and spends an overload budget, and the substation has
interlocked switchgear, a transformer with IEC 60076-7 temperatures and
a tap changer; and M4, EMS and market signals, as v0.9.0, which made the
plant a market participant: the grid's frequency has a shape and the
plant answers it, a dispatch application writes limits and reactive
modes the plant obeys and flags, the plant reports its tracking and
availability per quarter-hour, holds FCR and aFRR on the 2024 German
prices, books EUR, and a strategy that decides what to sell earns inside
the band the public index gave.

M2.6 added no physics. The viewer shows what is in alarm, by node and
severity, and the journal of what raised and cleared, numbered as MQTT
numbers it; the scene colours the container, the converter skid and the
substation by the word that speaks for them; the operator reset is a
button. The fifteen library cases are compiled into the browser module,
and a case played there lands on the digest CI holds it to.

What M2 changed is that the plant can misbehave, on demand and
reproducibly. Racks derate with cell temperature, drift apart and balance;
three alarm words with real causality behind them publish on every
surface, report by exception; a block goes into maintenance, a converter
trips, the site trips and comes back block by block; the planner ranks
real day-ahead prices over real local days, DST days included, and a
fiscal meter closes the quarter-hours. A YAML scenario schedules any of
it, and the data faults with it: dropouts with backfill, frozen values,
unit errors, alarm chatter, clock drift and NaN bursts, on MQTT and
Modbus, while REST and the stream keep telling the truth. The library's
failure mix is held to the public incident record. The annual round trip
moved from **84.22%** to **83.66%**, almost all of it the market rule
meeting real prices, itemized in [CALIBRATION.md](CALIBRATION.md).

What M1 changed is the efficiency story. The site replays a full year of
hourly DWD observations from Lindenberg (Mark), 2024. Heat leaves a rack into
its own cell mass before it reaches container air, so cell temperature lags by
tens of minutes instead of tracking. Cooling is staged against a container
manufacturer's published capacity, with electric heating and compressor
protection. The house load is an inventory of five named items rather than a
constant, each on its own meter.

The result is one number and its decomposition. Run over the whole replayed
year, the plant returns **84.17%** of the energy it takes in, measured at the
point of interconnection with everything it spends on itself already inside
that ratio. The gap to nameplate is not asserted, it is itemized: battery,
conversion, transformer, and five auxiliary items, each metered separately and
summing to what crossed the meter. The band it is held against comes from
public data, its floor from the measured EIA fleet average and its ceiling
from the NREL Annual Technology Baseline. Every figure is in
[CALIBRATION.md](CALIBRATION.md), with its sources and its known gaps.

M1.5 added no physics and put what was already there on the screen. The sun
is computed from the site's coordinates rather than tabulated, so shadows
shorten at noon and lengthen toward evening and a December day stays low. The
sky carries the measured cloud cover, and rain and snow fall when the
replayed hour had them. The clock can jump to any day of the year, either
restarting there or running the plant forward to it tick by tick.

Still at its simplest useful depth: a 1-RC cell model (with the LFP
OCV hysteresis that M2.5 added), a transformer with two temperatures but
unity power factor, synthetic grid frequency and voltage, no degradation.
Those are M4 and M5. The placeholder price curve is already gone on the way to M2:
the planner ranks the real 2024 German day-ahead prices (SMARD) one local
day at a time, DST days included.

CI enforces the physics invariants (energy conservation, SoC bounds, meter
monotonicity), byte-identical determinism, the curve-fit and single-cycle
calibration gates, and the annual gate: `bess-bench` runs the plant across the
whole replayed year on every pull request and fails the build if the
round-trip figure leaves its sourced band or the published record goes stale.

Interfaces and the signal map may change without notice until 1.0, and
[ROADMAP.md](ROADMAP.md) says what 1.0 has to pass before it arrives; the
stability contract itself is [COMPATIBILITY.md](COMPATIBILITY.md).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): how it is built and why, including the
  reference plant, the deterministic kernel, the state tree contract, and the
  explicit non-goals.
- [ROADMAP.md](ROADMAP.md): order of work, milestone by milestone, with the
  calibration target each one must hit.
- [COMPATIBILITY.md](COMPATIBILITY.md): what you can safely wire CI to.
- [DATA-LICENSES.md](DATA-LICENSES.md): license and provenance of every
  bundled dataset.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
