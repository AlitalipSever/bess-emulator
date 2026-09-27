# M2 phase 3: availability and calendar, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, block mode is a two-state enum, not a flag.** `InService` and
  `Maintenance`, room for more later (the M3 electrical work will want at
  least a local/remote notion). Rack isolation stays the existing bool.
- **D2, maintenance keeps the HVAC running.** A maintained block reports
  zero capability but its containers hold setpoint. Rationale: real sites
  do not let cells soak in summer heat during a PCS service, and the
  auxiliary share gate would silently improve if maintenance switched
  the house load off, which would be tuning by accident.
- **D3, staggered return spacing is a plant parameter.** Default sized in
  the PR (tens of seconds per block, so a full-site return takes minutes),
  stated as an engineering default, labeled as such: no public source pins
  restart spacing at this granularity, and inventing a citation would be
  worse than owning the estimate.
- **D4, price source.** German day-ahead hourly prices for 2024, bidding
  zone DE-LU. Preferred source SMARD (Bundesnetzagentur, CC BY 4.0,
  redistributable with attribution), fallback ENTSO-E Transparency (free
  but with reuse conditions that likely mean fetch-script-only per
  DATA-LICENSES.md policy). Decided by license text in PR2, recorded in
  DATA-LICENSES.md either way; hash pinned like the weather artifact.
- **D5, timezone handling is a table, not a dependency.** The plan builder
  needs Europe/Berlin midnights and quarter-hours for one pinned year.
  Proposal: derive them from the price series itself (the 2024 series is
  8784 hours with one 23-hour and one 25-hour local day; the compiled
  artifact carries the local-day boundaries computed offline by
  `bess-data`). This keeps chrono-tz out of the kernel and keeps the
  kernel pure UTC. Revisit when a second year ships.
- **D6, revenue meter granularity.** Site level, import and export Wh,
  closed on local quarter-hours, `slow` class on the map plus a full
  series in Parquet and bench output. Not per block: revenue metering
  happens at the POI, and per-block meters would be an invented signal.
  It is a second meter beside `site.meter.*`, not a rename: the SCADA
  accumulators keep their addresses and their meaning.
- **D7, two checkpoint bumps, one per schema-changing PR.** Block mode
  and return-sequencer state land in PR1 (format 6), the revenue meter
  accumulators and current-period state in PR2 (format 7). The M1 rule
  is per change, not per phase: a file written between the two PRs must
  be rejectable by version, never deserialized with missing state.

## Signal map impact

Additions only, folded into the release's 0.3.0: block mode (u16 enum on
the new per-block range from phase 2), revenue meter period energy and
period index. The availability points are not among them: they exist
since M0 at inputs 8 and 10, and this phase changes their values, not
the map. Exact addresses pinned in PR1/PR2 against the live map, delta
table written into this file then.

## Checkpoint impact

Format 5 to 6 in PR1, 6 to 7 in PR2 (D7).

## Test plan

- **Property: capability accounting.** Any combination of isolation,
  maintenance, derating and trip satisfies architecture invariant 1:
  the derated rack-level sum over in-service blocks, and zero while the
  breaker is open.
- **Unit: staggered return.** Trip, reset, then blocks reconnect in order
  with the configured spacing, each through its operating-state
  transitions; POI power ramps in steps, not one jump.
- **Unit: DST days.** The 2024-03-31 plan has 23 hours, 2024-10-27 has
  25; the revenue meter closes 92 and 100 periods on those days; total
  annual periods equal 35,136.
- **Determinism:** dataset hash pinned; golden snapshot regenerated for
  the price switch (the dispatch plan changes on every day of the year),
  called out loudly in the PR since it is the largest trajectory change of
  the milestone.
- **Annual run:** re-measured with real prices; round-trip band must hold
  (efficiency is price-shape sensitive only weakly through utilization);
  revenue is now computable but stays unpublished until M4 sources its
  band, per "we never emit a signal we cannot calibrate".
