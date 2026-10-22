# M5 phase 1: the aging model, implementation plan

Two PRs, each landing green.

## PR1: the states, the ledger and the model

- `bess-core`: `traits::AgingModel`, `state/aging.rs` (`RackAge`,
  `StressLedger`, `StressDay`), `kernel/aging.rs` (observe and the
  daily pass), `Models::aging`, `EnergyAccounting::aging_wh`, the
  fourth PRNG stream for `rate_scale`, checkpoint 20.
- `bess-models`: `aging.rs` with `LfpAging` and `NoAging`,
  `aging/nrel.rs`, `aging/naumann.rs`, `aging/power_law.rs`; `math.rs`
  gains public `exp_pos` and `tanh_pos`; `gw01_models` installs
  `LfpAging`; the cell model reads the resistance growth.
- `bess-bench`: the `aging` section of the annual record (mean, minimum
  and maximum state of health at the end, mean cycles, the calendar and
  cycle shares, the mean resistance growth, the mean cell temperature of
  the ledger), the `aging_wh` row in the loss waterfall and the residual.
- CALIBRATION.md: "M5: the aging model" with the two sources, the
  constants, the labeled resistance law and the year-one figures;
  DATA-LICENSES.md rows for BLAST-Lite and SimSES.
- Records regenerated (library, golden, the two annual records) and
  called out.

Accept: the aging tests pass; the no-aging proof holds; the clean year's
record moves only through the fade and the resistance (export falls by
the order of the year's mean capacity loss, the residual stays at zero
with the `aging_wh` row); the M1 round-trip band still holds for both
years; CI green on both platforms.

*As built (PR #58):* as the design's as-built notes say. Both PRs of the
phase landed as one: `site.soh_pct` was a one-line change folded in
here. The kernel's tick and the tree's constructor each crossed clippy's
line bound and gained a helper (`step_blocks`, `new_blocks`). `exp_det`
replaced the private `exp_rational` with an adaptive range reduction
that takes the old path for arguments within one, so the thermal
records did not move.

## PR2: `site.soh_pct` reads the racks

- `map/site.rs`: point 7 computed from the racks; the map's contract
  tests; COMPATIBILITY.md notes the changed value on the same point.
- The rack word's bit is phase 3's; nothing else on the surfaces.

Accept: the point moves over a long headless run; the map digest is
unchanged (a value, not a layout).

## Open questions

- Whether the ledger's depth of discharge should be the day's range or
  the largest single excursion; leans the range (D3), to be revisited if
  the market year's small FCR cycles are found to widen it.
- Whether `aging_wh` belongs in the loss waterfall or in a row of its
  own beside `stored_delta_mwh`; leans the waterfall with the label.
