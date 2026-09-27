# M2 phase 3: availability and calendar, implementation plan

Two PRs, each landing green.

## PR1: maintenance, isolation, trip and return

- Block mode enum (design D1), rack isolation command, maintenance
  semantics (D2).
- Protection trip command and the staggered return sequencer (D3);
  site alarm bits from phase 2 wired to both.
- Availability values corrected under maintenance, isolation and trip
  (the map points exist since M0 at inputs 8 and 10; no addition);
  capability-accounting property test.
- Checkpoint fields for block mode and sequencer state; format 5 to 6
  (D7).

Accept: trip during a replayed afternoon takes POI power to zero in one
tick, reset brings blocks back minutes apart through their operating-state
transitions, and the availability points tell the story throughout.

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

- SMARD versus ENTSO-E license outcome (D4).
- Whether the local-day boundary table lives inside the price artifact or
  beside it as its own compiled table (D5, decided by what keeps the
  artifact hash story simplest).
- Whether availability should be published as percent of nameplate as
  well as watts; leans watts only, percent is a consumer's division.
