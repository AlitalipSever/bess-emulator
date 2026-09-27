# M2.5 phase 1: OCV hysteresis, implementation plan

One PR.

## PR1: the hysteresis state

- Pin the LFP charge/discharge curve-pair source; fit `h_max(soc)` and
  `gamma` (design D2, D3); record fit and gate in CALIBRATION.md.
- `h` on `RackState`, integration in the cell model, energy accounting
  extended exactly; checkpoint bump (D4).
- Tests per the design's plan, switch-off equivalence included.
- Golden snapshot and annual record regenerated, called out.

Accept: a charge-rest-discharge-rest cycle shows rested voltages on two
distinct curves with the pinned mid-plateau separation, and the energy
invariant holds to the same tolerance as before the phase.

## Open questions

- Source choice if no single publication offers both curves and a
  usable protocol description at 25 C; fallback is combining a curve
  pair with a separately sourced settling protocol, stated openly in
  CALIBRATION.md.
