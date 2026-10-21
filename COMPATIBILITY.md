# Compatibility

This document is the stability contract for the external surfaces. If you
wire CI or a production pipeline to the emulator, this page tells you what
may change and when.

## Current status: pre-1.0

Everything below describes the contract that takes effect when the signal
map reaches version 1.0. Until then (0.x releases), any register, topic, or
endpoint may change in any release; breaking changes are called out in the
release notes.

One such change is still known and scheduled, which is the honest reason
this page still says pre-1.0: M3 replaces the three-value PCS state with a
five-state machine, which reinterprets a published point, a major change
under the rules below, so it happens before the contract takes effect
rather than after it. The other one announced since M1, the rack alarm
bits that read zero through map 0.2.0, shipped in M2 as map 0.3.0 (below).
[ROADMAP.md](ROADMAP.md) records what 1.0 has to pass.

## The signal map is an API

The reference is [refmodel/gw01-signal-map.csv](refmodel/gw01-signal-map.csv),
regenerated on every release with `bess-emulator --dump-signal-map`. It is
versioned with semver, independently of the crate versions:

- **Minor** (backward compatible): adding points at previously unused
  addresses or topics, adding new register blocks, widening documentation.
- **Major** (breaking): moving or removing a register, changing an
  encoding, scale, or unit, renaming an MQTT topic, changing the meaning of
  an enum value.

These two rules are about points. The reference file's own layout is a
separate contract, and it changed once: since map 0.2.0 the first line is a
comment carrying the version.

```
# signal-map-version: 0.6.0
```

Readers skip lines starting with `#`; a parser that does not will read that
line as a malformed row. A reference that cannot say which version of the
contract it is would be asking every consumer to guess, which is why the line
is worth the one-time break. Any further change to the file's layout will be
called out the same way, in the release notes and here.

The running process reports the same version at `/health`, so a client can
check which contract it is talking to without fetching this file.

### Version history

| Map | Introduced | Change |
|---|---|---|
| 0.1.0 | crate v0.2.0 | The first published map. It carried no version number; it is recorded here as 0.1.0 so the sequence has a beginning. |
| 0.2.0 | crate v0.3.0 | Additions only: the five itemized house-load points at site 32 to 41, and per block `container.air_temp_c` and `hvac.state` in the two slots each block had free. Nothing moved, nothing was renamed. The file gained its version comment line. |
| 0.6.0 | crate v0.9.0 (unreleased) | **Additions** (M4 phase 1 PR1): `site.grid_code.state` (60: 0 normal, 1 lfsm_o, 2 lfsm_u, 3 fcr) and `site.fcr.output_kw` (61, signed, exact), the grid-code block's state and the FCR droop's delivery; site word bits 4 `lfsm_active` (warning, mirrors a limited-frequency-sensitive cut) and 10 `grid_frequency_trip` (trip byte, latched: the kernel opens the breaker and faults every converter outside 47.5 to 51.5 Hz, cleared by the site reset like `protection_trip`). **Changed values, same points:** `site.poi.frequency_hz` (5) carries a synthetic series with the Continental European statistics (20 mHz spread, hour-boundary steps) instead of a 10 mHz sinusoid, or a measured year when `--frequency` replays one; `blockNN.pcs.setpoint_followed_kw` and `blockNN.pcs.p_ac_kw` include the block's share of the plant's frequency response, which the follower does not ramp. Scenario files gain `site.grid` with `frequency_offset` (`mhz`). Checkpoint format 17. **Additions** (M4 phase 1 PR2): `blockNN.pcs.q_kvar` at 2000 + 10 x block + 8 (the converter's reactive power, signed, positive over-excited, SCADA analog class 0.5) and `site.poi.power_factor` (59, exact). **Changed values, same point:** `site.poi.reactive_power_var` (2) stops reading zero: the converters' vars in the mode the operator chose (fixed cos φ, the default at unity; fixed Q; Q(U)) less the transformer's reactive consumption (12 % reactance, labeled), so a plant at its rating at unity reads about −12 Mvar. Checkpoint format 17 (unreleased, widened: the mode, the settled target, each converter's vars, the power factor). **Additions** (M4 phase 2 PR1): the holding bank grows from 3 to 12 registers: `control.p_limit_pct` (3, tenths of a percent), `control.p_limit_enable` (4), `control.p_limit_revert_s` (5), `control.q_mode` (6: 0 fixed cos φ, 1 fixed Q, 2 Q(U)), `control.q_setpoint_kvar` (7 and 8, i32), `control.cos_phi_x1000` (9), `control.cos_phi_under_excited` (10) and `control.command` (11: 1 site reset, 2 breaker open, 3 close, 4 disconnectors open, 5 close, 6 earthing open, 7 close, 8 clear limit; reads zero), the SunSpec immediate-controls shape under this map's names; every register reads back from the state it commands, and a value a register does not accept is an illegal data value. Site word bit 5 `external_limitation` (warning, raised while a written limit binds the target). A new event `command_refused` (`events/site/command_refused`, the register value) on MQTT and the stream, and on the event counter. REST gains `POST /api/v1/limit`, `/api/v1/reactive` and `/api/v1/substation/{breaker,disconnectors,earthing}`. Scenario files gain `site.ems` with `limit` (`pct`, `revert`) and `limit_clear`. Checkpoint format 18. **Additions** (M4 phase 2 PR2): the quarter-hour report at site inputs 63 to 71, closed on the fiscal meter's boundary: `site.kpi.schedule_deviation_kw` (63 and 64, i32, metered mean minus scheduled mean of the last closed period), `site.kpi.schedule_deviation_pct` (65, signed, tenths), `site.kpi.availability_discharge_pct` and `_charge_pct` (66, 67, tenths: the available power over the rating, time-averaged), `site.kpi.tracking_rms_kw` (68 and 69, u32), `site.kpi.redispatch_up_mwh` and `_down_mwh` (70, 71, tenths: the energy the racks hold above the window's floor and below its ceiling, updated every tick). All exact. Site word bit 6 `schedule_deviation` (warning, raised while the last closed period's deviation exceeds a 5 % frame, a labeled estimate). Checkpoint format 18 (unreleased, widened: the report's accumulators and last figures). |
| 0.5.0 | crate v0.8.0 | **Breaking** (M3 phase 2): `blockNN.pcs.state` (base+2) carries SunSpec's operating-state numbering instead of the M0 enum: 1 off, 3 starting, 4 running, 7 fault, 8 standby (was 0 standby, 1 run, 2 fault). Behind it the converter has a state machine: a reset leaves it `off` with its contactors open, and a cleared block walks `precharge`, `contactor_close` and `synchronize` (16 s, `starting` on the wire, the finer name on MQTT `pcs/state` and the stream) before it `ramp`s and runs; `site.available_*_kw` and the partial-availability bit count a block only from `ramp`, so the staggered return takes its spacing plus the startup. MQTT and REST names: `off`, `precharge`, `contactor_close`, `synchronize`, `ramp`, `standby`, `run`, `fault`. **Addition** (M3 phase 1): `blockNN.pcs.dc_voltage_v` at 2000 + 10 x block + 4, the DC bus voltage the converter converts at, SCADA analog class 0.5. The efficiency behind `blockNN.pcs.efficiency_pct` now depends on it (the CEC record's own voltage terms), and `site.available_*_kw` fall toward the bottom of the SoC window, since rack limits are current-bounded at the voltage the rack presents. Phase 2 adds the major step on `blockNN.pcs.state`. **Addition** (M3 phase 2 PR2): `blockNN.pcs.setpoint_followed_kw` at 2000 + 10 x block + 5, the target the converter is actually pursuing, exact. **Changed behaviour, same points:** the converter follows its share through a ramp of 10 % of its rating per minute and a 0.5 s lag, so a site setpoint step is reached about ten minutes later with `blockNN.pcs.p_ac_kw` and `site.poi.active_power_w` climbing at the gradient; `blockNN.block_alarm_bits` bit 0 (`setpoint_not_met`) judges the followed target and `site.alarm_bits` bit `power_limited` discounts a ramp in progress, so neither raises on a legitimate step. Checkpoint format 13. **Additions** (M3 phase 3): `blockNN.pcs.temperature_c` at 2000 + 10 x block + 6 (the converter's internal temperature, Pt100 class B) and `blockNN.pcs.overload_budget_pct` at + 7 (its short-term overload allowance, exact); block word bits 4 `pcs_over_temperature` (warning) and 5 `pcs_derated` (mirrors a thermal derate). **Changed behaviour, same points:** a converter runs up to 110 % of its rating while its budget holds (ten minutes at 110 %, twenty at 105 %), so after a block trips `blockNN.pcs.p_ac_kw` on the others reads above the rating and the site's `power_limited` bit arrives when the budgets are spent, not at the trip; `site.available_*_kw` stay bounded by the ratings. Checkpoint format 14. **Additions** (M3 phase 4 PR1): `site.substation.sequence` (52, 0 idle, 1 closing), `site.substation.disconnectors_closed` (53) and `site.substation.earthing_closed` (54), exact; two events on MQTT and the stream, `breaker_sequence` (`events/site/substation/sequence`, from and to) and `switch` (`events/site/substation/disconnectors` or `/earthing_switch`, closed). **Changed behaviour, same points:** a site reset on a tripped site closes the breaker on the tick after the reset, through the sequence, so `site.substation.hv_breaker_closed` and the `hv_breaker_open` bit move one tick later than before; the staggered return's timing is unchanged. Checkpoint format 15. **Additions** (M3 phase 4 PR2): `site.transformer.top_oil_c` (55) and `site.transformer.hot_spot_c` (56), the main transformer's IEC 60076-7 temperatures through Pt100 class B thermometers; `site.transformer.tap_position` (57, signed, exact); `site.mv_bus.voltage_kv` (58, class 0.5); site word bits 3 `transformer_over_temperature` (warning, 120 °C) and 9 `transformer_trip` (trip byte, 140 °C: the kernel opens the breaker and faults every converter, cleared by the site reset like `protection_trip`). **Changed values, same point:** `site.poi.voltage_kv` (4) follows a ±2 % daily swing of the grid voltage (a new kernel input, synthetic until M4) instead of reading the nominal. Checkpoint format 16. |
| 0.4.0 | crate v0.7.0 | **Breaking, by meaning:** `site.soc_pct` (input 6) and `blockNN.soc_pct` (base+3), with the same points on MQTT (`site/soc_pct`, `blockNN/soc_pct`), the REST summary (`soc`) and Prometheus (`bess_site_soc_ratio`), stop carrying the kernel's true state of charge and carry what the racks' BMSs report: an estimate that drifts with the current sensor's error on the LFP plateau and corrects itself at the knees of the OCV curve, by jumps that are events (`events/<rack>/soc_resnap`). Nothing moved, nothing was renamed, no scale changed; a client that inferred SoC from these points gets the number a real gateway would give it, and the truth is reachable only on the research surfaces (`bess-bench`'s record and series). No point carries a SoC quality or confidence (M2.5 phase 2 design D4). **Additions to the file, not the registers** (M2.5 phase 3): two columns, `instrument` and `accuracy_class`, name the meter behind each point and the class it is built to, or `exact` for points without one; every instrumented value now carries that instrument's error (noise, a slow wander, and a limit that grows at low load), always on, switched off only by `--exact-instruments`. |
| 0.3.0 | crate v0.5.0 | **Breaking:** `blockNN.alarm_bits` (base+6) and `site.alarm_count` (input 31) stop reading zero. The rack words behind the first now have a layout (see Alarm words below), and the second counts every set bit on site, rack, block and site words alike. Both, being alarm points, move from class `medium` to the new class `event`. **Additions:** `site.alarm_bits` (42), `site.event_counter` (43), and a second per-block range at 2000 + 10 x block carrying `block_alarm_bits`, `racks_derated`, `cell_dv_mv` and `mode` (M2 phase 3); `site.meter.station_service_kwh` (44, M2 phase 3); the fiscal meter's `site.revenue.period_import_kwh`, `site.revenue.period_export_kwh` and `site.revenue.periods_closed` (46, 48, 50, M2 phase 3), the last closed quarter-hour and the period count. **Changed values, same points:** `site.available_discharge_kw` and `site.available_charge_kw` (8, 10) now exclude blocks in maintenance, in fault or not yet returned after a trip, and read zero while the HV breaker is open; `site.poi.active_power_w` and the two POI meters read zero and stand still while the breaker is open, the house load moving to `site.meter.station_service_kwh`. **Corrected:** the scale column of `site.available_discharge_kw`, `site.available_charge_kw`, `site.meter.export_kwh`, `site.meter.import_kwh` and `blockNN.pcs.p_ac_kw` / `p_dc_kw` read 0.001, relative to watts, while the unit column said kW or kWh; it now reads 1, relative to the named unit. The registers carry the same counts as before. The reference file now lists rows in address order, holding registers last. Drafted in M2 phase 2; phases 4 and 5 (the scenario engine, the data faults) added no point, since the plant's registers do not know they are in a story. |

## Deprecation process (from map 1.0)

A point scheduled for removal is first marked deprecated in the CSV and the
release notes, keeps working for at least one minor release, and is removed
only in the next major release.

## Conventions guaranteed by the map

- 32-bit values span two consecutive registers, high word first.
- Sign convention: active power is positive when discharging (exporting).
- Input registers are read-only telemetry; holding registers are the
  control surface.
- Timestamps are Unix seconds, UTC.

### Enumerations

Changing any of these values is a major change.

| Point | Values |
|---|---|
| `site.ems.mode`, `control.ems_mode` | 0 follow the internal dispatch plan, 1 follow an external setpoint |
| `blockNN.pcs.state` | 0 standby, 1 running, 2 tripped |
| `blockNN.hvac.state` | 0 off, 1 one cooling unit, 2 both cooling units, 3 electric heating |
| `blockNN.mode` | 0 in service, 1 maintenance |

### Alarm words

Three words, each published as a u16: the rack word (behind
`blockNN.alarm_bits`), the block word (`blockNN.block_alarm_bits`) and the
site word (`site.alarm_bits`). The low byte is warnings, which clear
themselves; the high byte is trips, which stay set until an operator reset.
A bit either follows a continuous quantity with hysteresis, mirrors a
discrete state exactly, or latches. Changing what a bit means is a major
change, and a bit is never reused; free positions are headroom.

| Word | Bit | Name | Behavior |
|---|---|---|---|
| rack | 0 | `over_temp_warning` | hysteresis |
| rack | 1 | `under_temp_warning` | hysteresis |
| rack | 2 | `soc_high` | hysteresis |
| rack | 3 | `soc_low` | hysteresis |
| rack | 4 | `imbalance_warning` | hysteresis |
| rack | 5 | `derate_active` | hysteresis |
| rack | 6 | `isolated` | mirrors state |
| rack | 8 | `over_temp_trip` | latched |
| rack | 9 | `imbalance_trip` | latched |
| rack | 10 | `under_temp_trip` | latched |
| block | 0 | `setpoint_not_met` | hysteresis, after a 10 s deadband |
| block | 1 | `container_over_temp` | hysteresis |
| block | 2 | `hvac_failure` | mirrors state |
| block | 3 | `maintenance` | mirrors `blockNN.mode` |
| block | 8 | `pcs_fault` | mirrors the PCS fault state, which only a reset leaves |
| site | 0 | `power_limited` | hysteresis |
| site | 1 | `partial_availability` | mirrors state: a PCS in fault, a block in maintenance, a block not yet back after a trip, the breaker open, or a rack isolated |
| site | 2 | `hv_breaker_open` | mirrors state |
| site | 8 | `protection_trip` | latched; a site reset clears it, closes the breaker and starts the staggered return |

The names are the ones events and metrics publish, prefixed with the word:
`rack.derate_active`. The thresholds behind them are documented in
CALIBRATION.md and are model parameters, not part of this contract.

`site.event_counter` counts every event the kernel emits (alarm raises and
clears, PCS state changes) and wraps at 65536. A poller that reads it twice
learns how many events happened in between, even when it missed which. It
equals the `seq` of the latest event (below) modulo 65536.

### Aggregation of per-block points

A block carries more than one container and gets one register per quantity,
so each such point states which container it speaks for:

- `blockNN.cell_temp_min_c`, `blockNN.cell_temp_max_c`: the extremes over the
  whole block.
- `blockNN.container.air_temp_c`: the hottest container in the block.
- `blockNN.hvac.state`: the mode of the container drawing the most HVAC
  power, ties going to the lower index. It reports what the block's heaviest
  consumer is doing, so a block with one container heating while another runs
  both compressors reads as cooling.
- `blockNN.alarm_bits`: the rack words of the block folded with OR. It says
  some rack has the condition; the MQTT events subtree says which. The
  block's own word is `blockNN.block_alarm_bits`.
- `blockNN.cell_dv_mv`: the widest cell voltage spread among the block's
  racks, at 1 mV.
- `blockNN.racks_derated`: how many of the block's racks have
  `derate_active` set.
- `site.alarm_count`: every set bit on site, across all three words.

## Other surfaces

- **MQTT:** every point publishes under `bess/gw01/` at its name with dots
  as slashes (`bess/gw01/site/poi/active_power_w`), payload
  `{"ts": <unix s>, "value": <number>, "unit": "<unit>"}`, the value in the
  unit the point names (kW for a `_kw` point). Points of class `fast`,
  `medium` and `slow` publish on that cadence in simulation time, QoS 0, not
  retained. Points of class `event` publish when their value changes, and
  all of them again after every reconnect, QoS 1, retained, so a late
  subscriber gets the current word from the broker.
- **MQTT events:** every kernel event publishes once, QoS 1, not retained,
  under `bess/gw01/events/`, mirroring the telemetry tree:
  `events/block02/container1/rack05/derate_active`,
  `events/block02/setpoint_not_met`, `events/site/power_limited`, and
  `events/block02/pcs/state` for a PCS transition. An alarm payload is
  `{"seq", "ts", "node", "event": "raised" | "cleared", "alarm", "bit",
  "severity": "warning" | "trip"}`; a PCS transition is `{"seq", "ts",
  "node", "event": "pcs_state", "from", "to"}` with states `standby`, `run`,
  `fault`. A fault applied to the plant (a scenario's physical event) is
  `{"seq", "ts", "node", "event": "fault", "fault"}` under
  `events/<node>/fault`, ahead of what it causes on the same tick; `fault`
  is the name the scenario file uses, an HVAC unit's node is
  `block02/container0/hvac`, a leak adds `rate_ppm_per_day` and a reset
  `scope`. `seq` is the event's number in the kernel's log, from 1, so a gap
  means a lost message. A word and the event that changed it are not
  ordered relative to each other: the retained word may arrive first. These
  topics and fields follow the same rules as points: renaming is major,
  adding a field is minor.
- **Data faults, MQTT and Modbus:** a scenario's data events corrupt what
  these two surfaces say, never the plant: REST, the WebSocket stream,
  Prometheus and the checkpoint stay truthful, so the gap between what
  the plant did and what SCADA saw is visible. On MQTT a `dropout` stops
  publishing for its duration while the client stays connected (the
  retained words stay in the broker), and the messages the device would
  have sent are stored in order; when the dropout ends every `event`
  word is published again, as after a reconnect, so a word that changed
  in the silence reaches the broker; a `backfill_burst` replays that
  store ahead of anything live (live messages wait behind it), each
  message with the `ts` and the `seq` it was born with, none retained,
  a few hundred per second. A `freeze` repeats the value
  a selected point had when it began, with the current `ts`; a
  `unit_error` multiplies a selected quantity by `10^exponent`; a
  `nan_burst` writes the literal `NaN` as the value, which is not JSON,
  as a device's printf would; a `timestamp_drift` adds the drift so far
  to the `ts` of every message, cadence points, words and events alike,
  and steps back to the plant's time when it ends; a `chatter` flaps the
  lowest bit of the selected alarm words, retained like any change,
  while the events subtree carries only what the kernel did. On Modbus a
  `dropout` resets every session on its next request and accepts and
  closes new connections at once for its duration; a `freeze` holds the
  selected registers; a `unit_error` scales the quantity before
  encoding, saturating at the register's range; a `chatter` flips bit 0
  of the selected alarm-word registers. `points` selects `*`, `site`,
  `block[N]` or `block[N].pcs`; the control registers are never touched,
  a unit error and a NaN burst reach only the cadence points, a chatter
  only the alarm words.
- **REST (`/api/v1/...`):** versioned by URL path. Fields may be added to
  responses at any time; fields are only removed with a path version bump.
  `POST /api/v1/alarms/reset` takes `{"scope": "site"}`,
  `{"scope": "block", "block": N}` or
  `{"scope": "rack", "block": N, "container": C, "rack": R}` and nothing
  else. It answers 200 with `{"node": "<path>", "still_present": [{"node",
  "alarm", "bit"}]}`, the bits whose cause is still present (they raise
  again on the next tick); 400 for a body it cannot read, an unknown scope,
  or a field no scope has; 422 for a node the site does not have.
  `POST /api/v1/scenario` takes a scenario file as its body,
  `Content-Type: application/yaml` or `text/yaml`, and starts the plant
  over under it between two ticks: a fresh plant at the file's seed and
  start (the plant's current ones where the file states none), at the
  file's speed if it states one, the event log back at `seq` 1, every
  surface at tick 0 next, and no external setpoint, which went with the
  old plant. It answers 200 with the status below at tick 0; 400 with
  `{"error", "position"}` for a body that does not parse or validate;
  422, the same two fields, for a well-formed file naming a node the site
  does not have; 415 for another content type; 503 when the simulation
  task is gone. `GET /api/v1/scenario` answers `{"loaded": false}` when
  no scenario is in force, otherwise `{"loaded": true, "name",
  "content_hash", "seed", "start_unix_s", "speed", "length_s", "tick",
  "fired", "pending", "next", "events"}`: `content_hash` is the FNV-1a 64 of the
  file's bytes as `0x` and sixteen hex digits, `fired` and `pending`
  count events, `next` is the next event or null, and `events` is the
  whole schedule in firing order, each `{"index", "at_s", "tick",
  "kind": "physical" | "data", "fault", "fired"}` with `target` and, for
  a leak, `rate_ppm_per_day` on a physical event, and `surface`,
  `duration_s` and its parameters on a data event. The schedule is the
  reproduction record: a drawn value is visible nowhere else.
- **Health (`/health`):** `scenario` is `{"name", "content_hash"}` for a
  run started under a file and null otherwise, beside `kernel_version`
  and `signal_map_version`.
- **REST summary (`/api/v1/summary`) and the stream:** the site's word is
  `alarm_bits` and each block's own word `block_alarm_bits`, the names the
  same words have on Modbus and MQTT. Each block's `mode` carries the
  `blockNN.mode` values, and `station_service_kwh` sits beside
  `import_kwh` and `export_kwh`.
- **WebSocket (`/api/v1/stream`):** each message is the summary plus
  `events`, every event since the previous message in the MQTT payload
  shape, and `events_lost_ticks` when the stream fell further behind than the
  emulator buffers.
- **Browser module (npm `bess-emulator`, from v0.6.0):** the package
  exposes `WebHandle`, and its methods and their JSON shapes are the
  contract: `new()`, `start(canvas)`, `destroy()`, and the scenario door
  `scenarios()` (a JSON array of `{"name", "taxonomy", "length_s",
  "headline"}`, the embedded library in its order, callable before
  `start`), `start_scenario(name)` and `load_scenario_yaml(text)` (they
  start the plant over under a library case or a file's text, with the
  same semantics as `POST /api/v1/scenario`, and throw a string naming
  the refusal and its position), and `scenario_status()` (the
  `GET /api/v1/scenario` object above, built by the same function, as a
  JSON string). Adding a method or a JSON field is minor; changing a
  signature, removing a method or removing a field is major. The egui
  panels, their layout and the 3D scene are not a contract and change
  without notice; the module never hands plant data to the page.
- **Checkpoint format:** a tagged, versioned envelope, format 18 since M4
  phase 2 (v0.8.0 wrote format 16, v0.7.0 format 10, v0.4.0 format 3). Loaders reject unknown versions
  loudly; a version bump is documented in release notes.
- **Determinism:** within one release, (seed, config, input series) is
  byte-identical. Model improvements legitimately change trajectories
  between releases; the golden snapshot in CI documents each change.
