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
  pipeline meets in production, on demand.
- **Runs anywhere.** One Rust core; ships as a Docker service with protocol
  endpoints, and as a fully client-side browser build with an explorable 3D view
  of the plant.

## Quickstart

With Docker:

```sh
docker compose up
```

Then open <http://localhost:3000> (Grafana): the GW-01 dashboard shows a live
state-of-charge chart within a minute. The plant runs at 60x by default, so a
full market day plays out in 24 minutes.

Without Docker (Rust toolchain required):

```sh
cargo run --release -p bess-emulator
```

## Surfaces

| Surface | Where | What |
|---|---|---|
| Modbus TCP | `127.0.0.1:1502` | Input registers: telemetry. Holding registers: control (site setpoint, EMS mode). |
| MQTT | broker at `127.0.0.1:1883` | Topics under `bess/gw01/`, JSON payloads, decimated per publication class. |
| REST | `http://127.0.0.1:8080/api/v1/` | `state`, `summary`, `setpoint`, `speed`. |
| WebSocket | `ws://127.0.0.1:8080/api/v1/stream` | Tick summaries, 4 per second. |
| Prometheus | `http://127.0.0.1:8080/metrics` | Site KPIs for scraping. |
| Health | `http://127.0.0.1:8080/health` | Liveness + kernel version. |

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
browser build is `wasm-pack build crates/bess-wasm --target web`.

## Status

Pre-alpha, milestone M0 (walking skeleton). The whole plant runs end to end
with the simplest useful model at every layer: 1-RC cell model, lumped
container thermal with thermostat HVAC, flat-efficiency conversion, constant
transformer parameters, a placeholder daily price curve, and synthetic
weather. Physics invariants (energy conservation, SoC bounds, meter
monotonicity), byte-identical determinism, and the M0 round-trip-efficiency
gate are enforced in CI. Interfaces and the signal map may change without
notice until 1.0; see [COMPATIBILITY.md](COMPATIBILITY.md).

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
