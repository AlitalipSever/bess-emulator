# M4 phase 4: the market EMS, implementation plan

Two PRs, each landing green.

## PR1: the market EMS

- `ems/market.rs`, the ordering rule on `DayPlan` behind a switch, the
  block decision, headroom, mode value 2, point 80; checkpoint 20; the
  annual record with `MarketEms`; CALIBRATION.md revenue gate section
  with the band written first.

Accept: the ordering unit tests pass; the annual revenue is inside the
band or the section says by how much it is not and why.

*As built (PR #55):* as the design's as-built notes say. The market year
runs beside the clean one in the bench (`--market`, its own record
`calibration/m4-market.json`, the revenue band as its gate, a second CI
step); the shell's default strategy stays the day-ahead plan so the
library's stories keep their timing.

## PR2: release v0.9.0

- Version bump, records regenerated, ROADMAP M4 done, README status,
  COMPATIBILITY map 0.6.0 row and checkpoint line, release notes from
  the inventory, CALIBRATION the annual record at release.

Accept: tag pushed by the maintainer, quickstart green, CALIBRATION.md
carries the five M4 gates with sources and dates.

## Open questions

- Whether the default EMS of the shell becomes `MarketEms` or stays
  `DayAheadEms`: leans `MarketEms` with the markets on, since the release
  is about the market participant, with the old plan reachable by
  config.
- Whether the gate should also report the index's monthly path (the
  Apr to Dec 2024 fall from 220 to 100 kEUR/MW/year annualized) against
  the plant's monthly revenue: leans yes as a figure, not a gate.
