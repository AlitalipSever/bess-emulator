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

- Whether `pcs.setpoint not met` needs a deadband time (alarm only after
  N seconds of miss) to avoid flagging normal ramp lag; proposed yes,
  sized in PR1 against the M0 ramp constants.
- Whether the WASM viewer gets an alarm panel in this phase or in the next
  view iteration; leans next view iteration, the data is in the tree
  either way.
