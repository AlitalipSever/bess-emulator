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

## Open questions

- Whether a 314 Ah datasheet with explicit taper bands is publicly
  retrievable, or the bands must be stated as engineering defaults labeled
  as such (D1 fallback).
- Whether spread growth should also widen with temperature spread across
  the container (racks near the HVAC run cooler). Deferred unless the
  imbalance alarm in phase 2 turns out to be too uniform across the plant
  to be believable.
