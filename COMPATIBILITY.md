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
that read zero today, and M3 replaces the three-value PCS state with a
five-state machine. Both reinterpret a published point, which is a major
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
# signal-map-version: 0.2.0
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

## Other surfaces

- **REST (`/api/v1/...`):** versioned by URL path. Fields may be added to
  responses at any time; fields are only removed with a path version bump.
- **Checkpoint format:** a tagged, versioned envelope. Loaders reject
  unknown versions loudly; a version bump is documented in release notes.
- **Determinism:** within one release, (seed, config, input series) is
  byte-identical. Model improvements legitimately change trajectories
  between releases; the golden snapshot in CI documents each change.
