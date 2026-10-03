# Compatibility

This document is the stability contract for the external surfaces. If you
wire CI or a production pipeline to the emulator, this page tells you what
may change and when.

## Current status: pre-1.0

Everything below describes the contract that takes effect when the signal
map reaches version 1.0. Until then (0.x releases), any register, topic, or
endpoint may change in any release; breaking changes are called out in the
release notes.

Two of those changes are already known and scheduled, which is the honest
reason this page still says pre-1.0. M2 gives meaning to the rack alarm bits
that read zero through map 0.2.0 (map 0.3.0, below), and M3 replaces the
three-value PCS state with a five-state machine. Both reinterpret a published point, which is a major
change under the rules below, so both happen before the contract takes effect
rather than after it. [ROADMAP.md](ROADMAP.md) records what 1.0 has to pass.

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
# signal-map-version: 0.3.0
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
| 0.3.0 | crate v0.5.0 (unreleased) | **Breaking:** `blockNN.alarm_bits` (base+6) and `site.alarm_count` (input 31) stop reading zero. The rack words behind the first now have a layout (see Alarm words below), and the second counts every set bit on site, rack, block and site words alike. Both, being alarm points, move from class `medium` to the new class `event`. **Additions:** `site.alarm_bits` (42), `site.event_counter` (43), and a second per-block range at 2000 + 10 x block carrying `block_alarm_bits`, `racks_derated` and `cell_dv_mv`. **Corrected:** the scale column of `site.available_discharge_kw`, `site.available_charge_kw`, `site.meter.export_kwh`, `site.meter.import_kwh` and `blockNN.pcs.p_ac_kw` / `p_dc_kw` read 0.001, relative to watts, while the unit column said kW or kWh; it now reads 1, relative to the named unit. The registers carry the same counts as before. The reference file now lists rows in address order, holding registers last. Drafted in M2 phase 2; later M2 additions join this row until the release. |

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
| block | 8 | `pcs_fault` | mirrors the PCS fault state, which only a reset leaves |
| site | 0 | `power_limited` | hysteresis |
| site | 1 | `partial_availability` | mirrors state |
| site | 2 | `hv_breaker_open` | mirrors state |
| site | 8 | `protection_trip` | latched (laid out; raised from M2 phase 3) |

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
  `fault`. `seq` is the event's number in the kernel's log, from 1, so a gap
  means a lost message. A word and the event that changed it are not
  ordered relative to each other: the retained word may arrive first. These
  topics and fields follow the same rules as points: renaming is major,
  adding a field is minor.
- **REST (`/api/v1/...`):** versioned by URL path. Fields may be added to
  responses at any time; fields are only removed with a path version bump.
  `POST /api/v1/alarms/reset` takes `{"scope": "site"}`,
  `{"scope": "block", "block": N}` or
  `{"scope": "rack", "block": N, "container": C, "rack": R}` and nothing
  else. It answers 200 with `{"node": "<path>", "still_present": [{"node",
  "alarm", "bit"}]}`, the bits whose cause is still present (they raise
  again on the next tick); 400 for a body it cannot read, an unknown scope,
  or a field no scope has; 422 for a node the site does not have.
- **REST summary (`/api/v1/summary`) and the stream:** the site's word is
  `alarm_bits` and each block's own word `block_alarm_bits`, the names the
  same words have on Modbus and MQTT.
- **WebSocket (`/api/v1/stream`):** each message is the summary plus
  `events`, every event since the previous message in the MQTT payload
  shape, and `events_lost_ticks` when the stream fell further behind than the
  emulator buffers.
- **Checkpoint format:** a tagged, versioned envelope. Loaders reject
  unknown versions loudly; a version bump is documented in release notes.
- **Determinism:** within one release, (seed, config, input series) is
  byte-identical. Model improvements legitimately change trajectories
  between releases; the golden snapshot in CI documents each change.
