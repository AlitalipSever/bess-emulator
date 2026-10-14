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

Scope, pinned on 2026-10-14 against the tree as PR1 left it (map 0.3.0
with input 44 the last site register, checkpoint format 6, `ems.rs` still
ranking the 24 synthetic prices of `default_profile`). Design D4 to D7,
each proposed there and confirmed or revised in this PR.

What the PR does, in one paragraph: the emulator's July 14th already
has July 14th's weather; after this PR it has July 14th's prices too.
`bess-data` gains a second compiled artifact, the 2024 German day-ahead
series, and the planner stops eating the synthetic curve. The series is
stored as the Europe/Berlin year, so a plan day is a local day and the
two DST days are 23 and 25 hours long with no timezone library anywhere
in the kernel. The substation gains a second, fiscal meter that closes
on quarter-hours. The EMS does not get smarter; market behaviour stays
in M4.

### The data (D4, D5)

- **Source.** SMARD (Bundesnetzagentur), chart-data endpoint, filter
  `4169` (wholesale day-ahead price, DE-LU), hourly resolution, region
  `DE`. Verified 2026-10-14: the index at
  `https://www.smard.de/app/chart_data/4169/DE/index_hour.json` lists
  weekly blocks starting at Monday 00:00 local time; the block file
  `4169_DE_hour_<ms>.json` carries `{"meta_data": {...}, "series":
  [[ms, EUR/MWh], ...]}` with 168 hourly entries and no nulls in the
  week checked. 2024-01-01 is a Monday, so the local year is exactly the
  53 blocks from `1704063600000` (2024-01-01 00:00 CET) to
  `1735513200000` (2024-12-30), the last one trimmed. The English
  download centre states the terms: "Under the CC BY 4.0 licence, data
  from SMARD can be downloaded, stored and used free of charge." The
  exact attribution wording comes from SMARD's data-use page and is
  copied into DATA-LICENSES.md in this PR; if that page turns out to
  qualify the licence, the fallback is ENTSO-E Transparency as a
  fetch-script-only source and the artifact does not land (policy
  rule 2).
- **Fetch.** `crates/bess-data/scripts/fetch-prices.sh [dest-dir]`
  downloads the 53 weekly JSON files with `curl`, same shape as
  `fetch-weather.sh`: no hard-coded file list beyond the two anchoring
  timestamps, the committed artifact is the reproducibility anchor.
- **Compile.** `crates/bess-data/src/bin/compile-prices.rs` reads the
  weekly files, concatenates by timestamp, checks the series is exactly
  8784 consecutive hours from 2023-12-31 23:00 UTC with no gap and no
  duplicate (a missing or repeated hour is an error, not a fill: prices
  have no physical continuity that would justify interpolation; if the
  2024 series turns out to have one, the compile stops and the gap
  becomes a decision), computes the local-day table, prints the
  FNV-1a hash, writes `crates/bess-data/data/dayahead-de-lu-2024.bin`.
  Price values are stored as `i32` in 0.01 EUR/MWh: 2024 has negative
  hours and peaks above 327.67 EUR/MWh, so `i16` at that scale will not
  do.
- **The local-day table (D5, pinned).** Inside the artifact, not beside
  it: one file, one hash, one row in DATA-LICENSES.md. The table is
  367 `u16` entries, `day_start[d]` = index of the first hour of local
  day `d` (0 = 2024-01-01), with `day_start[366] = 8784` as the end
  sentinel. Day lengths fall out as differences: 364 days of 24 hours,
  2024-03-31 of 23, 2024-10-27 of 25. The compiler derives the table
  from the SMARD block stamps (as built: a block starts at Monday 00:00
  local, so a DST week is 167 or 169 hours from one block start to the
  next, and its last day, the switch Sunday, absorbs the odd hour; the
  hours inside a block are plain consecutive UTC hours), so nothing in
  the repository hard-codes the two dates except the tests that pin
  them.
- **Why the local year, not the UTC year.** The weather artifact holds
  the UTC year (8784 hours from 2024-01-01 00:00 UTC). The price
  artifact holds the local year, 2023-12-31 23:00 UTC to 2024-12-31
  22:00 UTC, also 8784 hours (23 + 25 = 48). Storing the UTC year would
  leave the first local day one hour short and the last local day with
  an hour that belongs to 2025-01-01. The replay already wraps the year
  by month and day (`weather.rs` maps every sim timestamp onto the
  reference year, so sim 2026-12-31 23:00 UTC and sim 2027-01-01 00:00
  UTC are consecutive reference hours), so the local-year series wraps
  with it: UTC reference hour `h` (0 to 8783) reads price index
  `(h + 1) mod 8784`. The header records the series start as a UTC
  offset of +1 h so the mapping is data, not a constant in code.
- **Format.** Own magic `BPD1`, version `u16`, year `u16`, hours `u16`,
  days `u16`, start offset `i16` (hours, +1), reserved to a 32-byte
  header; then `hours` x `i32` prices and `days + 1` x `u16` day
  starts. Little-endian like `BWD1`. The weather codec is not widened
  to carry prices: the two artifacts have different years (UTC versus
  local), different gap policies and different licences, and a shared
  container would have to explain all three.
- **Loader.** `bess_data::prices::dayahead_de_lu_2024() -> &'static
  PriceYear`, `OnceLock` behind `include_bytes!`, hash asserted
  against `DAYAHEAD_DE_LU_2024_FNV1A` on first use exactly as
  `lindenberg_2024` does. `PriceYear` exposes `price_eur_mwh(idx)`,
  `day_of(idx) -> usize`, `day_hours(d) -> Range<usize>`, `days()`,
  `len()`, `start_offset_h()`.
- **Licence record.** DATA-LICENSES.md gains the SMARD row (dataset,
  used for, licence with the attribution wording, bundled: yes, path,
  pin, fetch script) and loses the "Synthetic daily price curve" row,
  since the constant it describes is deleted in this PR.

### The planner

- `bess-models/src/calendar.rs`, new: `reference_position_h(unix_time_s,
  year) -> f64` moved out of `HistoricalWeather::position_h` (which
  becomes a one-line call), plus `reference_hour(unix_time_s, year) ->
  usize` as its floor. `month_day_from_unix_days` and the leap-year
  helper move with it; `weather.rs` keeps re-exporting the name so
  callers do not notice. One calendar, used by the weather driver, the
  planner, the shell's price gauge and the bench, so the four cannot
  disagree about which reference hour "now" is.
- `DayAheadEms` keeps its trait and its rule (rank the day's prices,
  charge the 3 cheapest hours at full power, discharge the 2 dearest, a
  2 h plant needing the extra charge hour for losses) and changes what a
  day is. New constructor `DayAheadEms::from_price_year(&'static
  PriceYear, power_w, charge_hours, discharge_hours)` precomputes one
  plan per local day at construction: 366 small bitsets, well under
  20 KB, so `site_target_w` stays allocation-free and does one table
  lookup per tick (`reference_hour` -> `(h + 1) mod len` -> `day_of` ->
  the bit). The 23-hour and 25-hour days are planned over their own
  slice; the hour counts stay 3 and 2 on them, no proration. Ties still
  break toward the earlier hour.
- `default_profile` and its 24 constants are deleted. Tests that need a
  plan build a small `PriceYear` by hand (the type gets a `pub fn
  from_parts` behind `#[doc(hidden)]` or a test-only constructor; the
  codec round-trip test needs it anyway).
- `ems.rs` is 100 lines today; the day table plus the constructor will
  push it past the soft limit with tests. Split before it does:
  `ems.rs` holds the strategy (lookup and the trait impl), `ems/plan.rs`
  holds `DayPlan` (rank a slice of 23 to 25 prices into charge and
  discharge bits) with its own tests.
- `gw01_models` wires `DayAheadEms::from_price_year(
  bess_data::prices::dayahead_de_lu_2024(), cfg.grid.site_rated_w, 3,
  2)`. `lib.rs` gains `gw01_day_ahead_price_eur_mwh(unix_time_s) ->
  f64` beside `gw01_weather`, the one function the shell and the bench
  call for "the price now": it composes `reference_hour` and the
  artifact lookup and has no other logic.

### The revenue meter (D6, D7)

- `bess-core/src/state/revenue.rs`, new: `RevenueMeter` with
  `import_wh` and `export_wh` (monotonic running totals, the fiscal
  series), `period_start_s`, `period_import_wh`, `period_export_wh`
  (the open period), `periods_closed: u32`, `last_import_wh`,
  `last_export_wh` (the most recently closed period, what the map
  publishes). One method, `observe(poi_w: f64, dt_s: f64, now_s: i64)`:
  books the tick's energy to the open period by the sign of `poi_w`,
  then closes the period when `now_s` (the tick's end instant) is a
  multiple of 900 s. Pure arithmetic, no clock knowledge beyond that
  modulus: Europe/Berlin is a whole-hour offset from UTC, so local
  quarter-hours and UTC quarter-hours are the same instants, and a
  test in `bess-data` pins that every `day_start` in the table is a
  whole hour so this stays true (D5). Labeling a period with its local
  day and quarter is a reader's job (the bench), not the kernel's.
- `SubstationState` gains `revenue: RevenueMeter`. `SimpleGrid` is not
  touched: it owns the physics and the SCADA meters; the kernel calls
  `self.state.substation.revenue.observe(poi_w, dt_s, now)` right after
  `grid.step`, in step 5. The POI power the two meters see is the same
  number, so the SCADA and fiscal totals agree to rounding until M2.5
  gives them different accuracy classes; a test holds them equal now.
- An open breaker leaves the POI at zero (PR1, D10), so the fiscal
  meter stands still through a trip without a special case, and the
  station-service feed is not revenue metered: it is not at the POI.
- Checkpoint format 6 to 7 (D7): the `RevenueMeter` fields join
  `SubstationState`; format 6 files are rejected by version; the format
  comment in `checkpoint.rs` gains its v7 line. A run resumed from a
  checkpoint continues the open period, which is why the period's
  running state is in the tree and not in the kernel.
- A run that starts off a quarter-hour boundary has a short first
  period; `period_start_s` makes that visible and nothing hides it. The
  bench and the determinism test both start on 2026-01-01 00:00 UTC.

### Surfaces

- **Map** (0.3.0, additions, class `slow`, input space, right after
  PR1's station-service meter, a u32 at 44 and 45; the draft had 45 to
  49, which overlapped it):

  | Address | Point | Encoding | What it reads |
  |---|---|---|---|
  | 46 | `site.revenue.period_import_kwh` | u32, kWh, scale 1 | `last_import_wh / 1000` |
  | 48 | `site.revenue.period_export_kwh` | u32, kWh, scale 1 | `last_export_wh / 1000` |
  | 50 | `site.revenue.periods_closed` | u32, count | `periods_closed` |

  Period energies rather than the running fiscal totals, because the
  fiscal totals would duplicate inputs 12 and 14 to the last digit until
  M2.5 and a SCADA integrator reading a 15-minute meter expects the
  period value. The running totals are published where a time series
  lives (Prometheus counters, bench). The price itself is not on the
  map: the map is the plant's interface and no public BESS register
  model (SunSpec 802, IEC 61850-7-420) carries a market price; the
  price is the EMS's input and is published as such below. `refmodel/
  gw01-signal-map.csv` regenerated (CI diffs it), the map digest test
  moves, COMPATIBILITY.md's 0.3.0 row gains the three points.
- **REST summary and the stream:** `revenue_period_import_kwh`,
  `revenue_period_export_kwh`, `revenue_periods_closed` (the last closed
  period, named as on the map) and
  `day_ahead_price_eur_mwh` (the price the planner is acting on this
  hour, from `gw01_day_ahead_price_eur_mwh`).
- **Prometheus:** counters `bess_revenue_import_watthours_total`,
  `bess_revenue_export_watthours_total`,
  `bess_revenue_periods_closed_total`; gauge
  `bess_day_ahead_price_eur_per_mwh`.
- **MQTT:** the three map points become topics by construction; no
  new event kind. A closed period is not an alarm and not a transition.
- **Grafana:** dashboard version 3 to 4, one new panel at the bottom
  (`y` 57, full width, height 8): "Day-ahead price (DE-LU 2024,
  replayed) and site setpoint", `bess_day_ahead_price_eur_per_mwh` on
  the left axis, `bess_site_setpoint_watts` on the right. Nothing else
  on the dashboard moves.
- **Not published anywhere:** revenue in EUR. The energy is metered and
  the price is known, so the product is one multiplication away for
  anyone who wants it, but M4 sources the band first ("we never emit a
  signal we cannot calibrate"); the bench does not compute it either.

### Bench

- `bess-bench/src/periods.rs`, new: a collector that watches
  `periods_closed` tick over and records each closed period with its
  UTC start, its local day and quarter (looked up through the same
  `PriceYear` day table, via `reference_hour`), import and export kWh.
  Written as CSV on `--periods <path>` only; 35,040 rows are about a
  megabyte and do not belong in `calibration/`. The committed study
  series does not grow.
- `Kpis` gains `periods_closed`, `periods_on_short_dst_day` and
  `periods_on_long_dst_day`; `bands.rs` checks them as exact values
  (35,040, 92, 100), not bands: they are arithmetic, not calibration.
- The annual run is 365 simulated days (Feb 29 is not replayed), so
  the bench count is 35,040. The reference year itself has 366 local
  days and 35,136 quarter-hours; that number is a `bess-data` fact and
  is pinned there. The milestone design's "35,136 over the year" meant
  the dataset; the design's test plan now says both.
- The record and the series are regenerated (`--write`) because the
  dispatch changed on every day of the year. `ANNUAL_RTE` and
  `AUX_SHARE_OF_IMPORT` keep their bands. A reading outside either is a
  finding to explain in CALIBRATION.md, never a band to widen.

### Documents

- CALIBRATION.md: a section "M2 (v0.5.0): real prices and the revenue
  meter" with the dataset row (source, licence, hours, the two DST
  days), the annual figures before and after the price switch side by
  side (RTE, aux share, import and export MWh, reported not tuned), the
  period counts, and the statement that EUR stays unpublished until M4.
- ARCHITECTURE.md line 176 and the `grid.rs` module doc stop promising
  the revenue meter for later; the `ems.rs` module doc stops saying
  "until bess-data ships real day-ahead series".
- docs/design/m2/README.md: release-note inventory entries (the price
  switch as the largest trajectory change of the milestone, checkpoint
  7, the three map points, the Prometheus families, dashboard 4, the
  summary fields) and the SMARD-versus-ENTSO-E open question struck
  through with its outcome.
- This file and design.md: D4 to D6 marked confirmed or revised, the
  open questions below closed.

### Code layout (AGENTS.md: one file, one concern)

- `bess-data/src/prices.rs` (type, loader, pin) and
  `bess-data/src/prices/codec.rs` (`BPD1`). `lib.rs` gains the module
  declaration and a re-export and nothing else: it is already over the
  hard limit and is listed as such.
- `bess-data/src/bin/compile-prices.rs`, `bess-data/scripts/
  fetch-prices.sh`. `days_from_civil` is needed by both compilers; it
  moves into `bess-data/src/calendar.rs` (crate-public) rather than
  being copied.
- `bess-models/src/calendar.rs`, `bess-models/src/ems.rs`,
  `bess-models/src/ems/plan.rs`.
- `bess-core/src/state/revenue.rs`; `state.rs` gains the field;
  `kernel.rs` gains one call; `checkpoint.rs` gains the version line.
- `bess-emulator/src/map/revenue.rs` (as built: `map/site.rs` was at
  284 lines, and the fiscal meter is its own subject, as the alarm
  points are), `http.rs`, `http/metrics/market.rs` (the fiscal counters
  and the price gauge, beside `metrics/alarms.rs`, for the same reason);
  `deploy/grafana/dashboards/gw01.json`.
- `bess-bench/src/periods.rs`, `run.rs`, `bands.rs`, `main.rs` (the
  flag). As built, `run.rs` (496 lines) is split first: the published
  record types move to `run/record.rs`, the walk that fills them stays.
- `compile-weather.rs` loses `days_from_civil` to `calendar.rs` and
  drops under the hard limit; AGENTS.md's table loses its row.
- Struct literals of `SubstationState` in test fixtures (`grid.rs`
  tests, `bess-emulator/src/fixtures.rs` if it builds one) gain the
  `revenue` field; `RevenueMeter` derives `Default` so that is one
  line each.

### Tests

- `bess-data`, unit, in `prices.rs` and `codec.rs`: codec round trip;
  bundled hash matches the pin; 8784 hours and 366 days; day lengths
  are 24 everywhere except 23 on day 90 (2024-03-31) and 25 on day 300
  (2024-10-27); every `day_start` is a whole hour; `day_of` is the
  inverse of `day_hours`; sum of quarter-hours over the table is
  35,136.
- `bess-models/tests/day_ahead_plan.rs`: on a hand-built year, the
  plan charges the three cheapest and discharges the two dearest hours
  of each local day; the 23-hour day's plan has no 02:00 and the
  25-hour day's has two 02:00s (in local terms: the slice has 23 and
  25 entries and the bits cover exactly those); the wrap: sim
  2026-12-31 23:00 UTC reads the first price of local 2024-01-01;
  `reference_position_h` moved without changing a value (the weather
  tests already cover its arithmetic and keep passing untouched).
- `bess-core`, unit, in `revenue.rs`: a period closes at every multiple
  of 900 s and not between; sign routing (export books export, import
  books import, zero books nothing); totals never decrease over 64
  seeded random power profiles drawn from the kernel's own
  `xoshiro256++` (no property-testing crate: the workspace has none and
  this needs none); the sum of closed-period energies equals the running
  total minus the open period, exactly (as built, the profiles are whole
  watt-hours per tick so the sums are exact in floating point and a
  dropped or doubled tick cannot hide behind rounding).
- `bess-models/tests/revenue_meter.rs`: one replayed day closes 96
  periods; the fiscal import and export totals equal the SCADA
  `import_wh` and `export_wh` to 1e-9 relative at every tick; through a
  protection trip and return (the PR1 fixture) the fiscal meter stands
  still exactly while the breaker is open; a checkpoint taken mid-period
  resumes with the same `period_start_s` and closes the period at the
  same tick as the uninterrupted run.
- `bess-emulator`: map tests for the three points (address, unit,
  value), summary and metrics tests for the new fields and families,
  the CSV-in-sync check.
- `bess-bench`, unit, in `periods.rs`: the collector labels a period
  start on each DST day with the right local day and quarter; the
  check mode reports 35,040 / 92 / 100.
- Determinism: `GOLDEN_DIGEST` regenerated once, its comment naming two
  causes (the tree gained the meter fields, and the dispatch changed
  because the prices did). Checkpoint: format 7 round trip, format 6
  refused.

### Commits, in order

1. This document and design.md (the pin).
2. `bess-data`: script, compiler, artifact, pin, codec, calendar
   helper, DATA-LICENSES.md.
3. `bess-models`: `calendar.rs`, the planner over real prices,
   `gw01_day_ahead_price_eur_mwh`; the determinism digest was expected
   to move here (prices). As built it did not: in its four hours both
   price curves charge from 03:00 UTC.
4. `bess-core` meter and checkpoint 7, the shell surfaces, the Grafana
   panel; the digest moves (schema), and the constant's comment says
   why it moved once and not twice.
5. Bench collector and counts, `--write` regeneration of the record and
   the series, CALIBRATION.md section, the README inventory, the two
   module docs, the ARCHITECTURE.md line.

Each commit leaves `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace` and `cargo run
-p bess-bench -- --check-docs` green. Run `rustup update stable` first:
CI's clippy floats and has been ahead of this machine in the last two
PRs (`assert_is_empty` twice).

Accept: `docker compose up`, and the new Grafana panel shows the real
2024 price shape with the setpoint following it (charging in the
troughs, discharging in the evening peak); on the two DST days the
bench reports 92 and 100 periods; `tests/revenue_meter.rs` is the code
side of the acceptance. The compose run needs a machine with Docker,
which the development machine is not; it is the reviewer's step.

Called out in the PR: the price switch is the first trajectory change
since M1 PR2's weather switch; the annual record and every bench KPI
move with it, the before and after side by side in CALIBRATION.md. The
golden digest moves once, for the meter's schema, because its four hours
dispatch the same under both curves.

Not in this PR: revenue in EUR (M4); a second reference year (D5
says revisit then; the 15-minute day-ahead market since October 2025
makes 2025 a resolution question as well as a data question); any
per-block metering; the price on the Modbus map; scenario-driven
calendar events (phase 4 drives the calendar, this PR gives it one).

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

PR2, settled on 2026-10-14:

- **D4:** SMARD, CC BY 4.0, artifact bundled. The data-use page asks
  for "Bundesnetzagentur | SMARD.de" and does not qualify downloaded
  data; DATA-LICENSES.md carries the row and a compilation record.
- **D5:** the local-day table lives inside the price artifact; the
  series is the local year and wraps with the replay. The emulator's
  DST days are the reference year's (2024-03-31 and 2024-10-27), not
  the simulated calendar's, exactly as its weather is the reference
  year's.
- **D6, revised:** the map points sit at 46, 48 and 50; the summary
  names follow the map.
- **Availability as percent:** no. Watts only.
- **Quarter-hour series storage:** bench CSV on `--periods`, not a
  committed file.
- **The price on the map:** no; Prometheus gauge and summary field only.
- **The aux identity tolerance** (found while building): the January
  day's itemized and metered house load differed by 1.4e-12 relative
  after the dispatch changed, against a 1e-12 bound. Both sum the same
  86,400 ticks in different orders; the bound is now 1e-10 with that
  reasoning in the test, far below the 1e-3 a forgotten item would
  leave.
- **The monthly round trip** (found while building, Ali chose the
  correction on 2026-10-14): under real prices the plain monthly ratio
  carries month-end state of charge and lost the January-over-August
  spread the series test holds (0.8319 against 0.8303). The series now
  also publishes `stored_delta_mwh` and
  `round_trip_efficiency_endpoint_corrected`; corrected, January reads
  0.8481 against August's 0.8303, and the test reads the corrected
  figure.
