# gw-emulator

A synthetic grid-scale battery plant (BESS) emulator.

gw-emulator simulates a realistic 100 MW / 200 MWh battery plant, from cells and
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

## Status

Pre-alpha. The workspace currently contains the kernel crate skeleton; interfaces
and signal maps are still in flux and everything may change without notice.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
