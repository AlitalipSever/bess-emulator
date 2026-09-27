# M2 phase 1: BMS deepening, implementation plan

Two PRs, each landing green.

## PR1: temperature derating

- Pin the LFP datasheet (design D1) and record it in CALIBRATION.md
  sources with retrieval date.
- Temperature factors in `BasicBms::rack_limits`, multiplied onto the SoC
  taper (D2). Parameters on `BasicBms` with the datasheet values as
  defaults.
- Tests: derate monotonicity property, threshold fidelity unit test,
  existing BMS tests still green.
- Regenerate the annual record; explain the throughput delta in the PR;
  round-trip band holds.

Accept: a replayed July heat week shows limits dipping below rated while
cell temperatures peak, and nothing else about the plant changed.

As built: the acceptance criterion assumed a warm knee near 45 C. The
pinned table (EVE MB31) keeps full power to 55 C, and July cells peak at
38 C, so July leaves the factor at 1.00. January cells dip to 13.6 C and
move the charging factor to 0.89, which does not bind: the plant draws
0.498 of rack rating. The annual record re-ran unchanged, so there was no
throughput delta to explain and nothing was regenerated. The criterion
was replaced by what can be held: `bms_derating.rs` pins "moves in
winter, never binds, untouched in summer", and a July rack pushed onto the
hot shoulder halves its limits and loses them past 60 C. Derating reaches
the plant through phase 4 scenarios (HVAC loss, cold start).

## PR2: spread and balancing

- `RackState` gains `cell_dv_v` and `balancing_active`; checkpoint format
  3 to 4 (D6), old files rejected by version.
- `BmsLogic::step_bms` with `BmsFlows`; kernel calls it in the tick
  before limits; balancing heat enters the rack thermal node; energy
  invariant extended.
- Spread dynamics and balancing policy (D3, D4) with sourced growth and
  bleed parameters.
- Prometheus: spread min/max gauges, balancing count.
- Tests: energy-over-balancing property, spread dynamics property, policy
  unit tests (bleed only near top of window, discharge interrupts).
- Golden snapshot regenerated once, called out.

Accept: a long replayed stretch shows spread sawtoothing (grows through
cycling, shrinks during top-of-window idle), and the waterfall identity
still closes.

As built: 90 replayed days from 11 April show a fresh plant reach the
threshold in about four weeks, then saw between 2.2 and 2.75 % SoC with
50 to 90 racks bleeding at once; the numbers are in CALIBRATION.md. That
run is too long for a debug test, so `tests/balancing.rs` starts a day
with every rack past the threshold and holds one tooth: balancing runs,
the spread narrows at the top and widens through the evening, the energy
balance closes. The state is `cell_dsoc` plus its voltage reading, not a
voltage with dynamics (D3 revised). The Prometheus families went into
`http/metrics.rs`, which this PR split out of the over-limit `http.rs`.

## Open questions

- ~~Whether a 314 Ah datasheet with explicit taper bands is publicly
  retrievable.~~ Resolved in PR1: EVE MB31 Tables 5 and 7, through a
  distributor mirror (CALIBRATION.md).
- Whether spread growth should also widen with temperature spread across
  the container (racks near the HVAC run cooler). Deferred unless the
  imbalance alarm in phase 2 turns out to be too uniform across the plant
  to be believable.
