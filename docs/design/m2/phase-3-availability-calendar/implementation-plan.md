# M2 phase 3: availability and calendar, implementation plan

Two PRs, each landing green.

## PR1: maintenance, isolation, trip and return

Scope, pinned against the tree as phase 2 left it (design D1 to D3, D7 to
D12):

- `BlockMode` and `BlockState::mode` (D1); `ReturnPolicy` and the two
  sequencer fields on `EmsState` (D3); `SubstationState::station_service_wh`
  (D10); checkpoint format 5 to 6 (D7).
- The dispatchable set in the tick: allocation over it, availability
  from it, zero while the breaker is open (D11).
- Commands: `set_rack_in_service`, `set_block_mode`, `trip_pcs`,
  `trip_protection`; `reset_alarms(Site)` closes the breaker and starts
  the return (D8, D9). Unknown nodes refused before any change.
- Alarm wiring: `block.maintenance` (bit 3, mirrors the mode) in
  `alarms::layout`, `site.partial_availability` reads maintained blocks
  too, `site.protection_trip` raised by the trip and cleared by the
  reset, `block.pcs_fault` and `site.hv_breaker_open` keep mirroring
  (D12).
- Station service: `SimpleGrid::step` on an open breaker meters the
  house load on the station-service accumulator and leaves the POI at
  zero, the kernel's aux step no longer gated on the breaker (D10); the
  energy-balance closures (`tests/invariants.rs`, `tests/balancing.rs`,
  `tests/aux_inventory.rs`, `bess-bench`'s residual) gain the
  station-service term.
- Map and surfaces: `blockNN.mode` at 2000 + 10b + 3, class medium;
  `site.meter.station_service_kwh` at input 44, class slow; per-block
  `mode` in the summary and the stream, `station_service_kwh` in the
  summary; `bess_station_service_watthours_total` on Prometheus;
  COMPATIBILITY.md rows for the enum, the alarm bit and the meter; a
  CALIBRATION.md section for the spacing default and the station-service
  statement.
- Tests per the design's PR1 list; `tests/trip_return.rs` is the
  acceptance.

Code layout (AGENTS.md: one file, one concern):

- `bess-core/src/kernel/commands.rs`: the between-ticks door, every
  command of this phase plus `set_hvac_failed`, which moves there from
  `kernel.rs` (same inherent method, no caller changes). The reset stays
  in `kernel/alarms.rs` with the alarm vocabulary it belongs to and calls
  the sequencer.
- `bess-core/src/kernel/restart.rs`: the staggered-return sequencer, its
  policy type, the per-tick release and the dispatchable predicate, with
  its unit tests. `kernel.rs` (280 lines) stays under the soft limit by
  calling it rather than growing.
- `bess-core/src/state/plant.rs`: `BlockMode` beside `PcsOpState`;
  `state.rs`: the two `EmsState` fields.
- `bess-models/src/grid.rs`: the open-breaker branch;
  `state.rs`: the meter field on `SubstationState`.
- `bess-emulator/src/map/block.rs`: the mode point; `map/site.rs`: the
  meter point; `http.rs` and `http/metrics.rs`: the summary fields and
  the counter.
- As of this writing the only struct literals of `BlockState` and
  `EmsState` are in `state/init.rs`; test fixtures build racks,
  containers, PCS and substation states, not blocks, so the block and
  EMS fields touch no test by construction. `SubstationState` literals
  in test fixtures (`grid.rs` tests among them) gain the meter field.

Accept: trip during a replayed afternoon takes the POI to zero in one
tick while the station-service meter takes over the house load, reset
brings blocks back minutes apart through their operating-state
transitions (block k at k x 30 s, the whole site in 570 s), and the
availability points tell the story throughout.

Called out in the PR: the golden snapshot digest moves for the schema
change (new tree fields), with the annual record and every bench KPI
unchanged.

Not in this PR: any REST, Modbus or CLI surface for the commands (the
phase 4 player is the door; phase 4 D6 rejected a "fire one action now"
endpoint); the WASM viewer's
maintenance and trip rendering, which waits with the alarm panel for the
next view iteration; the M3 breaker interlocks and PCS startup states.

## PR2: real prices, local days, revenue meter

- `bess-data`: day-ahead 2024 fetch script, compiled artifact with
  local-day boundary table (D5), pinned hash, DATA-LICENSES.md entry
  (D4).
- Plan builder consumes per-local-day price slices; DST days get 23 and
  25 hour plans.
- Revenue meter accumulators, quarter-hour closing, map points, Parquet
  and bench output (D6).
- Checkpoint: meter and period state; format 6 to 7 (D7).
- Tests: DST units, meter monotonicity property, 35,136 periods over the
  year.
- Golden snapshot regenerated (price switch), annual record regenerated,
  both called out.

Accept: `docker compose up`, and the Grafana price panel (added here,
one panel, no dashboard redesign) shows the real 2024 shape driving
dispatch; the DST unit tests pin the two odd days.

## Open questions

PR1, settled on 2026-10-12:

- **D10:** station service on its own meter. The POI reads zero while
  the breaker is open, as the draft said; the house load moves to a
  third accumulator. The earlier proposal, booking it on the POI import
  meter, was a shortcut no real breaker allows.
- **D3:** 30 s per block, so the return averages the 10 % per minute
  reconnection gradient; labeled an estimate anchored to that gradient.
- **D1:** `block.maintenance` as a mirror bit, with `blockNN.mode` the
  primary record.
- **Rack self-discharge:** not this phase. It is a cell fault that
  accelerates the phase 1 spread and surfaces through the phase 2
  imbalance bits; phase 4's design now names it as the one fault action
  that brings its own state change (D3 there).

PR2, as before:

- SMARD versus ENTSO-E license outcome (D4).
- Whether the local-day boundary table lives inside the price artifact or
  beside it as its own compiled table (D5, decided by what keeps the
  artifact hash story simplest).
- Whether availability should be published as percent of nameplate as
  well as watts; leans watts only, percent is a consumer's division.
