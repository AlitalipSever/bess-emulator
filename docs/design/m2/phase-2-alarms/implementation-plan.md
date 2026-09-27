# M2 phase 2: alarm tree and event surface, implementation plan

Two PRs, each landing green.

## PR1: alarm semantics in the kernel

- Freeze the three bit layouts (design D1) in `bess-core`, documented on
  the types themselves.
- Rack evaluation in `BmsLogic` (D2), block and site evaluation in the
  kernel tick; hysteresis parameters (D3).
- Latching, `Command::ResetAlarms` (D4), `PcsOpState::Fault` exit path.
- `Event` enum widens (alarm raised/cleared); event log hash joins the
  determinism digest.
- Checkpoint format 4 to 5 (D7).
- Tests: raise/clear pairing property, no-chatter property, latching
  units, the causal chain integration test.

Accept: the causal chain test passes with alarms in physical order, and a
clean replayed year raises zero trips (a plant that false-alarms weekly
would fail the realism it claims).

As built:

- **Chain test (`tests/alarm_chain.rs`).** It fails both containers of one block, not one. With a single container the physics limits itself. Its racks derate, the block delivers less and makes less heat, and it settles near 59 C, about 1.6 MW short of the site's 2 MW threshold. With the whole block down the order holds: derate at about 8 minutes, setpoint miss at 21, site limited at 31. The test asserts that order and the minutes between the steps.
- **HVAC failure.** This needed a physical HVAC failure, which no earlier phase had built. `HvacState::failed` is now honored by the thermal model and set through `Simulation::set_hvac_failed`. That is the first physical fault the kernel accepts, and phase 4's player reaches it through `FaultAction`.
- **Zero-trip gate.** `bess-bench` counts raises by name, and a clean year with any trip fails the gate. Warnings are published but not gated. The reference plan overruns the energy window twice a day, and the block and site bits say so.
- **Code layout.**
  - `kernel.rs` was split into `kernel/block.rs` and `kernel/alarms.rs`.
  - The bench's `report.rs` was split into `report/render.rs`.
  - The alarm tally is new, in `bess-bench/src/alarms.rs`.
  - The new fields pushed `state.rs` past the 500-line hard limit. It was split into `state/energy.rs`, `state/plant.rs` and `state/init.rs`, all re-exported, so no path changed.
- **Speed.** Evaluating the rack words first cost 45 % of the annual run's speed. Two changes recovered it and more: the temperature curves cache their peak and skip the scan inside their full-rate span, and unchanged words skip the event diff. 30 replayed days now run at 56 k ticks/s, against 51.5 k before this PR. The shortcut returns exactly what the scan returns, and a test holds that at 0.01 K steps.

## PR2: publication

- Block word, site word, event counter, `cell_dv_mv`, derate status on
  the map; second per-block address range opened; delta table written
  into design.md with final addresses (D6).
- MQTT `events/` subtree, report by exception; WebSocket events;
  Prometheus events counter and alarm gauges.
- REST `POST /api/v1/alarms/reset`.
- Grafana: alarm panel (active alarms by severity, event rate); dashboard
  version bumped; metric-existence test extended.
- COMPATIBILITY.md: version history row drafted for 0.3.0 naming the
  meaning change (published at release, phase 5).

Accept: a Modbus poller sees base+6 go nonzero during the chain test
scenario, the event counter advances, and an MQTT subscriber receives the
raise and clear messages with the documented payload.

## Open questions

- ~~Whether `pcs.setpoint not met` needs a deadband time.~~ Yes, 10 s.
  The M0 PCS has no ramp, so there was no ramp lag to size it against;
  it keeps one tick of a block crossing its capability from reading as a
  failure to deliver. M3's ramps will revisit it.
- Whether the WASM viewer gets an alarm panel in this phase or in the next
  view iteration; leans next view iteration, the data is in the tree
  either way.
