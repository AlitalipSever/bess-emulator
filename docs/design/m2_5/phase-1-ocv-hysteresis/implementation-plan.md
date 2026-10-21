# M2.5 phase 1: OCV hysteresis, implementation plan

One PR. Landed as PR1 (feature/m2_5-phase1-pr1, 2026-10-21); the
as-built notes are at the end.

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
  CALIBRATION.md. *Resolved:* one publication (Barai 2015) offers the
  protocol and the peak in text; the plateau level is an estimate under
  a sanity bound, with the open gap recorded in CALIBRATION.md.

## As built

- `crates/bess-models/src/cell/hysteresis.rs`: `Hysteresis` (envelope +
  rate), `LFP_HALF_GAP_V`, exact per-tick solution with envelope clamp;
  `Ecm1Rc` gains the `hysteresis` field and
  `lfp_314ah_rack_without_hysteresis()`.
- `RackState::hysteresis_v`, checkpoint format 9.
- Tests: envelope and sign-chasing, rest holds, switch-off, transition
  rate (unit); two-branch gate, coincidence when off, energy closure
  through a cycle, switch-off digest proof (`tests/ocv_hysteresis.rs`).
- Golden digest regenerated; all fifteen library records regenerated
  (physics change): three thermal cases moved their event counts by a
  few derate events, the extra hysteresis heat.
- Cycle RTE gate 0.917 to 0.9129, inside its band; annual record
  regenerated, delta in CALIBRATION.md.
