# M4 phase 2: external control and reporting, implementation plan

Two PRs, each landing green.

## PR1: the holding bank and the commands

- The limit in `EmsState`, its revert timer, the external-limitation
  bit, the reactive registers on the bank, the command register and the
  REST switchgear routes, `command_refused` event; checkpoint 18
  (partial); the `curtailment-order` case; Modbus end-to-end test.

Accept: every holding register exercised over TCP in one test; the
curtailment case's record shows the flag and the two ramps.

*As built (PR #50):* as the design's as-built notes say. The scenario
parser's physical table grew past clippy's line bound and its two
parameterized kinds moved into helpers.

## PR2: the quarter-hour KPIs

- `kernel/kpi.rs`, the eight points, the `schedule_deviation` bit, the
  bench's KPI summary (annual availability, deviation distribution);
  checkpoint 18 complete; CALIBRATION.md section naming the TSOs'
  document and the labeled frame.

Accept: the annual record publishes availability and the deviation
distribution; the staggered-return case raises the bit.

## Open questions

- Whether the command register should also carry maintenance mode and
  HVAC failure (REST has them): leans yes, as 9 and 10, so the Modbus
  surface and REST carry the same verbs.
- Whether availability should count the overload budget: leans no, the
  M3 rule (advertised capability bounded by the rating) stands.
