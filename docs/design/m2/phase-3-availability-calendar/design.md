# M2 phase 3: availability and calendar, design

Each decision is proposed here and confirmed or revised in its PR. D1 to
D3 and D7 to D12 are PR1's and are pinned below against the tree as phase
2 left it (map 0.3.0, checkpoint format 5, `kernel/alarms.rs`); D1, D3
and D10 were confirmed on 2026-10-12, D3 and D10 in revised form. D4 to
D6 are PR2's, pinned on 2026-10-14 against the tree as PR1 left it and
confirmed the same day in PR2, D5 with a corrected derivation and D6 in
revised form; the pin and the outcome are marked in each.

## Decisions

- **D1, block mode is a two-state enum, not a flag.** `InService` and
  `Maintenance`, room for more later (the M3 electrical work will want at
  least a local/remote notion). Rack isolation stays the existing bool.
  **Pinned for PR1:** `BlockMode` lives in `state/plant.rs` beside
  `PcsOpState`, the field is `BlockState::mode`, and a plant starts with
  every block in service. The mode is published twice, on purpose: as the
  enum register `blockNN.mode` (0 in service, 1 maintenance; room for
  M3's values, which is why it is a register and not a bit), and as bit 3
  of the block word, `block.maintenance`, which mirrors the mode. The bit
  is what makes entering and leaving maintenance an event on every
  surface (MQTT `events/block07/maintenance`, the counter, the stream)
  without a new `Event` variant, exactly as `rack.isolated` already does
  for the contactor. The alternative, a `BlockModeChanged` event, would
  have been a third event kind for every shell to spell, for one bit of
  information the alarm path already carries. SCADA alarm lists carry
  "maintenance mode active" as an informational entry for the same
  reason an operator wants it there.
- **D2, maintenance keeps the HVAC running.** A maintained block reports
  zero capability but its containers hold setpoint. Rationale: real sites
  do not let cells soak in summer heat during a PCS service, and the
  auxiliary share gate would silently improve if maintenance switched
  the house load off, which would be tuning by accident.
  **Pinned for PR1:** in `step_block` a maintained block is outside the
  dispatchable set, so its target is zero, its DC request is zero, and
  `PcsModel::finalize` takes the PCS to `Standby` through the transition
  that already exists (one `PcsStateChanged` event if it was running).
  The BMS step, the cell step with a zero request and the thermal step
  all run as before. One refinement to the milestone text: the maintained
  PCS leaves `AuxDemand::pcs_not_converting`. A converter under service
  is locked out with its AC and DC disconnects open, not energized and
  idling, so its 339.8 W tare stops; the containers' HVAC and the rack
  electronics keep drawing. The clean year never maintains anything, so
  the annual record does not move.
- **D3, staggered return spacing is a plant parameter.** Default sized
  against the reconnection ramp the German connection codes ask of a
  plant coming back after a protective disconnection: active power may
  rise by at most 10 % of rated power per minute. VDE-AR-N 4105 states it
  for the low-voltage case in published summaries; the 110 kV text
  (VDE-AR-N 4120) is not freely available, so the default is labeled an
  estimate anchored to that gradient, not a cited 4120 figure. No public
  source pins restart spacing per block, and inventing one would be worse
  than owning the estimate.
  **Pinned for PR1:** `ReturnPolicy { spacing_s }` in `bess-core`, held by
  `Simulation` beside `BlockSiteThresholds` and defaulted the same way;
  default 30 s. One GW-01 block is 5 % of the site, so a block every 30 s
  averages 10 % per minute, and the twentieth block is released 570 s,
  nine and a half minutes, after a reset. The release is a 5 MW step per
  block, not a continuous ramp: the M0 PCS has none, and the average
  gradient is what the default matches. The state is on `EmsState`:
  `blocks_released: usize` and `next_release_s: f64`. Release order is
  block index, the simplest deterministic order and the one a plant
  controller with numbered feeders uses. A released block that is
  maintained or faulted stays out of the dispatchable set; the sequencer
  releases positions, not power. The row goes into CALIBRATION.md as an
  estimate, next to the thresholds.
- **D4, price source.** German day-ahead hourly prices for 2024, bidding
  zone DE-LU. Preferred source SMARD (Bundesnetzagentur, CC BY 4.0,
  redistributable with attribution), fallback ENTSO-E Transparency (free
  but with reuse conditions that likely mean fetch-script-only per
  DATA-LICENSES.md policy). Decided by license text in PR2, recorded in
  DATA-LICENSES.md either way; hash pinned like the weather artifact.
  **Pinned for PR2:** SMARD, chart-data filter `4169` (day-ahead
  wholesale price, DE-LU), hourly, region `DE`, weekly block files
  `4169_DE_hour_<ms>.json` of `[ms, EUR/MWh]` pairs. The download
  centre states "Under the CC BY 4.0 licence, data from SMARD can be
  downloaded, stored and used free of charge" (read 2026-10-14); the
  attribution wording is taken from SMARD's data-use page in the PR.
  Artifact `crates/bess-data/data/dayahead-de-lu-2024.bin`, pin
  `DAYAHEAD_DE_LU_2024_FNV1A`, prices as `i32` in 0.01 EUR/MWh (2024
  has negative hours and peaks past what `i16` holds at that scale).
  Gaps are errors, not fills: a price has no continuity to interpolate
  along.
  **Confirmed in PR2:** SMARD's data-use page licenses the market data
  under CC BY 4.0 and asks for the attribution "Bundesnetzagentur |
  SMARD.de"; nothing on it qualifies downloaded data, so the ENTSO-E
  fallback was not needed. The compiled year has no gap: 8784 hours,
  mean 78.51 EUR/MWh, minimum -135.45, maximum 936.28, 457 negative
  hours.
- **D5, timezone handling is a table, not a dependency.** The plan builder
  needs Europe/Berlin midnights and quarter-hours for one pinned year.
  Proposal: derive them from the price series itself (the 2024 series is
  8784 hours with one 23-hour and one 25-hour local day; the compiled
  artifact carries the local-day boundaries computed offline by
  `bess-data`). This keeps chrono-tz out of the kernel and keeps the
  kernel pure UTC. Revisit when a second year ships.
  **Pinned for PR2:** the table lives inside the artifact (367 `u16`
  day starts, the last an end sentinel), derived by the compiler from
  the SMARD timestamps, not typed in. The series is the Europe/Berlin
  year, 2023-12-31 23:00 UTC to 2024-12-31 22:00 UTC, 8784 hours like
  the UTC year (23 + 25 = 48), and the header records the +1 h start
  offset. The replay already wraps the year by month and day, so the
  local-year series wraps with it: UTC reference hour `h` reads index
  `(h + 1) mod 8784`, and sim 2026-12-31 23:00 UTC reads the first
  hour of local 2024-01-01. Two consequences, both deliberate: the
  emulator's DST days are 2024's (03-31 and 10-27) whatever year the
  simulated clock shows, as its weather is 2024's; and the revenue
  meter needs no table at all, because Europe/Berlin is a whole-hour
  offset and local quarter-hours are UTC quarter-hours (a `bess-data`
  test pins every day start to a whole hour so this stays true).
  Labeling a quarter-hour with its local day is the bench's job.
  **Confirmed in PR2, with the derivation corrected:** the hours inside a
  SMARD block are plain consecutive UTC hours with no kink; what marks a
  DST week is the block's length, 167 or 169 hours between two Monday
  00:00 block starts. The compiler puts a local midnight every 24 hours
  from each block start and lets the week's last day, the Sunday the EU
  switches on, absorb the odd hour. The result is the table the pin
  describes: day 90 (2024-03-31) of 23 hours, day 300 (2024-10-27) of 25.
- **D6, revenue meter granularity.** Site level, import and export Wh,
  closed on local quarter-hours, `slow` class on the map plus a full
  series in Parquet and bench output. Not per block: revenue metering
  happens at the POI, and per-block meters would be an invented signal.
  It is a second meter beside `site.meter.*`, not a rename: the SCADA
  accumulators keep their addresses and their meaning.
  **Pinned for PR2:** `RevenueMeter` in `state/revenue.rs`, a field of
  `SubstationState`, fed by the kernel right after `grid.step` with the
  same POI power the SCADA meters see (so the two agree to rounding
  until M2.5 gives them accuracy classes). Running fiscal totals, the
  open period (start, import, export), the closed-period count and the
  last closed period's two energies. Closes when the tick's end instant
  is a multiple of 900 s. The map publishes the last closed period and
  the count (inputs 45, 47, 49), not the running totals, which would
  read as inputs 12 and 14 do; the running totals go to Prometheus and
  the bench. "Parquet" in the milestone text means the bench's series
  output: the bench writes the quarter-hour series as CSV on request
  (`--periods <path>`), not into `calibration/`. Revenue in EUR is
  computed nowhere until M4 sources its band.
  **Revised in PR2:** the three points sit at inputs 46, 48 and 50, not
  45 to 49: the station-service meter at 44 is a u32 and holds 45 as
  well. The summary carries the last closed period like the map, so its
  fields are named after the map points (`revenue_period_import_kwh`,
  `revenue_period_export_kwh`, `revenue_periods_closed`); the running
  totals stay on Prometheus and in the bench.
- **D7, two checkpoint bumps, one per schema-changing PR.** Block mode
  and return-sequencer state land in PR1 (format 6), the revenue meter
  accumulators and current-period state in PR2 (format 7). The M1 rule
  is per change, not per phase: a file written between the two PRs must
  be rejectable by version, never deserialized with missing state.
  **Pinned for PR1:** format 6 carries `BlockState::mode`,
  `EmsState::blocks_released` and `EmsState::next_release_s`. Nothing
  else the trip touches is new to the tree: the breaker position, the
  latched site bit and the PCS states have been in it since M0 and
  phase 2. **Pinned for PR2:** format 7 carries
  `SubstationState::revenue` (the `RevenueMeter` fields of D6); the
  open period's running state is in the tree precisely so a resume
  closes the period on the same tick as an uninterrupted run.
- **D8, the protection trip is a command, applied between ticks.**
  `Simulation::trip_protection()`: the breaker opens, every PCS goes to
  `Fault`, `site.protection_trip` is set, `blocks_released` goes to zero,
  and the PCS transitions and the raise wait in `pending_events` for the
  next tick, which is also the tick whose site evaluation raises
  `hv_breaker_open` (it mirrors the breaker), so a subscriber sees the
  trip, the breaker and twenty converters in one batch, trip first. A
  second trip on a tripped site changes nothing and says so by returning
  `false`. The trip is never "still present" in the reset's sense: it is
  caused by a command, not by a condition the plant could still be in,
  so a reset always clears it. `trip_pcs(block)` is the one-block
  version: `Fault`, its transition pending, and `block.pcs_fault` mirrors
  it on the next tick; it refuses an unknown block.
- **D9, a site reset closes the breaker and starts the return.**
  `reset_alarms(ResetScope::Site)` on a tripped site does what it does
  today (clears the latched bits in scope, takes every PCS from `Fault` to
  `Standby`, queues the events) and in the same call closes the breaker
  and sets `next_release_s` to zero with `blocks_released` still zero, so
  the next tick releases block 0 and the sequencer takes it from there.
  `hv_breaker_open` clears on that tick. This is the operator pressing
  "reset lockout" and "close" as one action; M3's breaker state machine
  with interlocks is where they become two commands with a sequence
  between them. A block-scope reset during a site trip takes that PCS out
  of fault and nothing more: the breaker is open and the block is not
  released, so it cannot run, and `pcs_fault` clears while
  `protection_trip` and `hv_breaker_open` stay, which is the truth.
- **D10, station service: the dark site still has a house load, on its
  own meter.** Confirmed. While the breaker is open the auxiliary
  inventory keeps being evaluated and metered, but not at the POI: no
  current crosses an open breaker, so the POI reads exactly zero and the
  import and export meters stand still, as the milestone draft said. The
  house load is fed through a station-service supply (in practice a
  separate low-voltage connection to the local network, with its own
  meter, plus UPS for protection and controls) and is booked on a third
  substation accumulator, `SubstationState::station_service_wh`.
  `SimpleGrid::step` on an open breaker reports zero transformer loss,
  zero POI power, the aux total as the house load, and advances the
  station-service meter by it; on a closed breaker that meter does not
  move. The kernel stops zeroing the aux draw on an open breaker.
  Rationale: the alternative, a dark site, would be a thermal model
  cooling with no electricity (today's dead branch) or an HVAC that stops
  on every trip, and a 110 kV plant that loses cooling, fire detection
  and BMS supply whenever its main breaker opens is not how such plants
  are built. The site energy balance gains the station-service import as
  a third term (import plus station service minus export), and the
  waterfall identity is untouched because `aux_power_w` reports the house
  load on either feed. Losing the station-service supply as well stays an
  M3 fault.
- **D11, availability is the dispatchable set's capability.** The kernel
  zeros the AC capability of a block outside the dispatchable set
  (architecture, "The dispatchable set") before summing, and zeros the
  sum while the breaker is open. Today a PCS in fault still contributes
  its rack capability, which M0 never noticed because nothing faulted.
  The site target is not clamped to availability: the planner keeps
  asking for what the plan says, the plant delivers what it can, and the
  phase 2 bits say the rest, exactly as they do at the SoC window edges
  today. During a return `power_limited` therefore raises and clears on
  its own, which is the right story. An availability-aware planner is M4
  material.
- **D12, no new event kind.** Every state change this PR adds is
  observable through bits that mirror state (`rack.isolated`,
  `block.maintenance`, `block.pcs_fault`, `site.hv_breaker_open`), the
  latched `site.protection_trip`, and `PcsStateChanged`. The phase 4
  invariant that every fired physical action is in the event log holds
  for this phase's actions by construction, and `bess-bench`'s tally, the
  MQTT projection and the stream need no new arm.

## Signal map impact

Additions only, folded into the release's 0.3.0. The availability points
are not among them: they exist since M0 at inputs 8 and 10, and this
phase changes their values, not the map.

**Proposed for PR1**, against the live map (block second range at
2000 + 10 x block; slots 0 to 2 taken by phase 2):

| Address | Point | Encoding | Class | Change |
|---|---|---|---|---|
| 2000 + 10b + 3 | `blockNN.mode` | u16 enum: 0 in service, 1 maintenance | medium | new (D1) |
| base+6 and 2000 + 10b | the block word | bit 3 `maintenance`, mirrors state | event (unchanged) | new bit in an unreleased layout (D1) |
| 8, 10 | `site.available_discharge_kw`, `site.available_charge_kw` | unchanged | unchanged | values correct under maintenance, isolation, fault, trip and return (D11) |
| 0 | `site.poi.active_power_w` | unchanged | unchanged | reads zero while the breaker is open (D10) |
| 44 | `site.meter.station_service_kwh` | u32, as `site.meter.import_kwh` | slow, as the other meters | new (D10) |

`blockNN.mode` is class `medium` like `hvac.state`, not `event`: the
event class is the alarm points' and a test keeps it there. A mode
change is still an immediate event through `block.maintenance`. The
summary and the stream gain a per-block `mode` field, named as on the
map, and the summary a `station_service_kwh` beside `import_kwh`;
Prometheus gains `bess_station_service_watthours_total`.
COMPATIBILITY.md gets the enum row, the alarm-word row and the meter
row.

**Built in PR2**, additions only, class `slow`, input space, right
after the station-service meter (44 and 45):

| Address | Point | Encoding | Class | Change |
|---|---|---|---|---|
| 46 | `site.revenue.period_import_kwh` | u32, kWh, scale 1 | slow | new (D6): the last closed quarter-hour's import |
| 48 | `site.revenue.period_export_kwh` | u32, kWh, scale 1 | slow | new (D6): the last closed quarter-hour's export |
| 50 | `site.revenue.periods_closed` | u32, count | slow | new (D6): quarter-hours closed since the run began |

The price is not a map point: no public BESS register model (SunSpec
802, IEC 61850-7-420) carries a market price, and the map is the
plant's interface, not the EMS's input. It is published where inputs
are: `bess_day_ahead_price_eur_per_mwh` on Prometheus and
`day_ahead_price_eur_mwh` in the summary and the stream. The summary
also gains `revenue_period_import_kwh`, `revenue_period_export_kwh` and
`revenue_periods_closed`; Prometheus gains the three matching counters
(`bess_revenue_import_watthours_total`,
`bess_revenue_export_watthours_total`,
`bess_revenue_periods_closed_total`). The Grafana dashboard moves to
version 4 with one panel: the price and the site setpoint on two axes.
COMPATIBILITY.md's 0.3.0 row gains the three points.

## Checkpoint impact

Format 5 to 6 in PR1 (`BlockState::mode`, `EmsState::blocks_released`,
`EmsState::next_release_s`, `SubstationState::station_service_wh`), 6 to
7 in PR2 (`SubstationState::revenue`, D7). The golden snapshot
digest moves in PR1 because the tree gained fields, not because any
value did: the clean year never isolates, maintains or trips, and the
PR shows the annual record unchanged to the last digit. PR2 expected it
to move twice, once for the price switch and once for the meter. It
moved once, for schema: in the digest's four hours (2026-01-01 00:00 to
04:00 UTC) the replayed prices and the old synthetic curve both charge
from 03:00 UTC, so the trajectory there is unchanged, and with the meter
taken out the tree is identical to PR1's, value for value. The price
switch shows where every day is replayed: the annual record, which
moves with it and is set beside the old one in CALIBRATION.md.

## Test plan

PR1:

- **Property: capability accounting.** Random combinations of isolated
  racks, maintained blocks, faulted PCS, released count and breaker
  position on a reduced plant, against the architecture's predicate:
  availability equals the AC capability summed over dispatchable blocks
  of their in-service racks' derated limits, and zero while open.
- **Unit: isolation.** Isolating a rack mid-discharge drops its current
  to zero on the next tick, raises `rack.isolated` and
  `site.partial_availability`, moves its share to its neighbours;
  restoring clears both. An unknown rack is refused.
- **Unit: maintenance.** Entering maintenance on a running block:
  `Standby` on the next tick with its transition event,
  `block.maintenance` and `site.partial_availability` raised, zero
  capability, HVAC still drawing on a replayed July afternoon, the PCS
  tare gone from `aux.pcs_standby_w`, the racks still balancing; leaving
  maintenance reverses all of it.
- **Unit: PCS trip.** One block to fault: `pcs_fault` on the block word,
  availability down by one block, the site still delivering through the
  others; a block reset brings it back without touching the sequencer.
- **Integration: trip and return** (`tests/trip_return.rs`, the PR's
  acceptance). The replayed July afternoon at full discharge. Trip: the
  POI goes from near 100 MW to exactly zero in one tick, transformer
  loss reads zero, `site.protection_trip`, `site.hv_breaker_open` and
  twenty `Run` to `Fault` transitions are in that tick's batch, the
  availability points read zero, the import and export meters stand
  still while the station-service meter advances by the house load, the
  aux books stay balanced and the site energy balance closes with the
  station-service term. Reset: the breaker closes, the station-service
  meter stops, block 0 runs on the next tick, block k on tick k x 30
  after it, the POI climbs in twenty steps, `power_limited` raises during
  the return and clears after it, every PCS is in `Run` 570 s after the
  reset. A checkpoint taken mid-return resumes the same sequence and the
  same log.
- **Unit: checkpoint.** Format 6 round trip; a format 5 file is refused by
  version.
- **Bench:** the zero-trip gate still holds on the clean year, and the
  annual figures do not move.

PR2:

- **Unit: DST days.** The 2024-03-31 plan has 23 hours, 2024-10-27 has
  25; the revenue meter closes 92 and 100 periods on those days. Two
  totals, both pinned: the reference year's table sums to 35,136
  quarter-hours (366 local days, a `bess-data` fact), and the bench's
  365-day run closes 35,040 (Feb 29 is not replayed; a `bess-bench`
  exact check beside the 92 and 100).
- **Unit: the wrap.** Sim 2026-12-31 23:00 UTC reads the first price of
  local 2024-01-01; the day table's starts are all whole hours.
- **Unit: the meter.** Closes at every multiple of 900 s and not
  between; routes by sign; never decreases over 64 seeded random power
  profiles from the kernel's own PRNG (no property-testing crate);
  closed periods sum to the running total minus the open period.
- **Integration: the meter against the plant** (`tests/revenue_meter.rs`).
  One replayed day closes 96 periods; fiscal and SCADA totals agree to
  1e-9 relative every tick; the fiscal meter stands still exactly while
  the breaker is open in the PR1 trip fixture; a checkpoint taken
  mid-period resumes and closes on the same tick as the uninterrupted
  run.
- **Determinism:** dataset hash pinned; golden snapshot regenerated for
  the price switch (the dispatch plan changes on every day of the year),
  called out loudly in the PR since it is the largest trajectory change of
  the milestone.
- **Annual run:** re-measured with real prices; round-trip band must hold
  (efficiency is price-shape sensitive only weakly through utilization);
  revenue is now computable but stays unpublished until M4 sources its
  band, per "we never emit a signal we cannot calibrate".
