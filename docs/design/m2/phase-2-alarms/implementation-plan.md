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
- **Speed.** Evaluating the rack words first cost 45 % of the annual run's speed. Two changes recovered it and more: the temperature curves cache their peak and skip the scan inside their full-rate span, and unchanged words skip the event diff. 30 replayed days now run at 52 to 56 k ticks/s across runs, against 51.5 k before this PR. The shortcut returns exactly what the scan returns, and a test holds that at 0.01 K steps.

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

As built:

- **Acceptance, over real sockets.** `modbus::tests` runs the chain plant
  at full speed and polls it: base+6 of the hot block is nonzero from the
  first read, the block word shows the failed HVAC, derate, setpoint miss
  and power limited appear on their three words in that order and at least
  a minute apart, and the event counter moves. It takes about a second.
  `mqtt::tests` runs the same plant against a broker, waits for the hot
  block's `setpoint_not_met` raise, drops the setpoint to zero, and checks
  the raise and the clear against the documented payload, field for field.
  There is no broker in CI and adding one would mean a new dependency, so
  the test carries a minimal MQTT 3.1.1 server of its own: connect, publish
  with QoS 1 acknowledgement, ping. The real client talks to it over TCP.
- **A bug the MQTT test found.** The publisher marked "never published"
  with `i64::MIN` and subtracted it from the clock. That overflows: debug
  builds panic, release builds wrap to a negative age, so no cadence point
  was ever due and **MQTT has published no telemetry since M0**. Nothing
  had exercised the publisher against a broker before this PR. "Never" is
  now `None`.
- **Events reach the surfaces on a channel.** The simulation task
  broadcasts each tick's events, numbered by their place in the log, after
  the snapshot that shows their effect. The snapshot only ever holds one
  tick, and at full speed a surface wakes once per thousands of them, so
  reading events off it would lose most. A surface that falls more than
  4096 eventful ticks behind is told how many it lost; the sequence numbers
  show the gap to its consumers too.
- **One projection of an event.** `events.rs` names nodes, alarms, topics
  and payloads once; MQTT, the WebSocket stream and the REST reset all use
  it, so the three surfaces cannot spell a node two ways.
- **Reset over REST.** `POST /api/v1/alarms/reset` sends the kernel a
  command with a reply channel and answers when the reset has run between
  two ticks: 200 with the bits still present, 422 for a node the site does
  not have (the body is wrong, not the path, so not 404). A Modbus reset
  register was considered and left until someone asks; the holding bank is
  the control surface and one write there is easy to add.
- **Prometheus and Grafana.** `bess_events_total` and
  `bess_alarms_active{alarm, severity}` in `http/metrics/alarms.rs`; the
  dashboard (version 3) gains active alarms by severity, events per minute
  and active alarms by name, and its test now also fails if no panel
  queries the alarm families.
- **Code layout.** `map.rs` (946 lines, on AGENTS.md's over-limit list)
  gained a new concern and was split: `map/site.rs`, `map/control.rs`,
  `map/block.rs`, `map/alarms.rs`, `map/encode.rs`, `map/csv.rs`, contract
  tests in `map/tests.rs`. The table is sorted by address after it is
  built, so the parts can be cut by subject without reordering the CSV by
  accident. Shared test plants live in `fixtures.rs`.
- **Not in this PR.** The WASM viewer's alarm panel stays with the next
  view iteration, as the open question below leans.
- **Review findings, fixed in the PR.** Each was confirmed by running it
  before it was fixed; a suspected MQTT test race was tried with the window
  forced open, did not fail, and was dropped.
  - *Units on MQTT.* The `_kw` and `_kwh` points extracted watts under kW
    labels, and the new `cell_dv_mv` volts under mV, so payloads read 1000
    times off, a bug as old as the map that MQTT's silence had hidden.
    Every extract now returns its value in the point's own unit, the scale
    column changed with it, the registers did not, and a test holds name
    suffix, label and value together.
  - *The Modbus acceptance test under load.* At full speed its first poll
    landed after the derate and 35 ms before the miss; 16 busy processes
    on 8 cores failed it 2 runs in 10. At 600x it passed 5 of 5 under 32.
  - *The reset read bodies loosely.* A rack reset sent with the block tag
    reset the whole block, and a body axum rejected came back 422, the
    status meant for an unknown node. Unknown fields are now refused, and
    a body the endpoint cannot use is a 400 with a JSON reason.
  - *One name, two words.* The summary called the block's word
    `alarm_bits`, the name of the rack fold everywhere else; it is
    `block_alarm_bits` now, and the reset's answer says `node`, not
    `scope`, which the request uses for its tag.
  - *Retained words after a broker restart.* They were sent only on
    change, so a broker without persistence lost them for good. Every
    reconnect now sends them all again, with a test that hangs up on the
    client and fails without the fix.
  - *Lost events* were logged at debug; they are counted and logged as
    warnings now.

## Open questions

- ~~Whether `pcs.setpoint not met` needs a deadband time.~~ Yes, 10 s.
  The M0 PCS has no ramp, so there was no ramp lag to size it against;
  it keeps one tick of a block crossing its capability from reading as a
  failure to deliver. M3's ramps will revisit it.
- Whether the WASM viewer gets an alarm panel in this phase or in the next
  view iteration; leans next view iteration, the data is in the tree
  either way.
