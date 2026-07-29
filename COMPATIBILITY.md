# Compatibility

This document is the stability contract for the external surfaces. If you
wire CI or a production pipeline to the emulator, this page tells you what
may change and when.

## Current status: pre-1.0

Everything below describes the contract that takes effect when the signal
map reaches version 1.0. Until then (0.x releases), any register, topic, or
endpoint may change in any release; breaking changes are called out in the
release notes.

## The signal map is an API

The reference is [refmodel/gw01-signal-map.csv](refmodel/gw01-signal-map.csv),
regenerated on every release with `bess-emulator --dump-signal-map`. It is
versioned with semver, independently of the crate versions:

- **Minor** (backward compatible): adding points at previously unused
  addresses or topics, adding new register blocks, widening documentation.
- **Major** (breaking): moving or removing a register, changing an
  encoding, scale, or unit, renaming an MQTT topic, changing the meaning of
  an enum value.

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

## Other surfaces

- **REST (`/api/v1/...`):** versioned by URL path. Fields may be added to
  responses at any time; fields are only removed with a path version bump.
- **Checkpoint format:** a tagged, versioned envelope. Loaders reject
  unknown versions loudly; a version bump is documented in release notes.
- **Determinism:** within one release, (seed, config, input series) is
  byte-identical. Model improvements legitimately change trajectories
  between releases; the golden snapshot in CI documents each change.
