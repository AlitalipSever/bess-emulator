# M5 phase 2: time compression, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the projection repeats the measured stress, it does not re-run
  the physics.** The kernel ticks one simulated year in about twenty
  minutes on the CI runner; five years of physics do not fit the
  milestone's "five minutes" or the job's timeout. The published models
  are used the same way by their authors (BLAST-Lite's standard use is a
  one-year stress profile repeated for the life), so projecting the
  measured year's ledger is a method with a citation, not a shortcut.
  What it misses is the feedback (an aged rack cycles deeper for the
  same energy, so its later years age a little faster); `--resimulate`
  exists for anyone who wants it measured, and CALIBRATION.md reports
  the gap once from a local run. The alternative, a coarser kernel tick
  for aging runs, was rejected: every M3 dynamic (the follower's lag,
  the converter's startup, the RC branch) is tuned at one second, and a
  second kernel is a second plant.
- **D2, the clock moves, the meters do not.** A fast-forward advances
  `unix_time_s` by the days skipped so that the calendar (prices, the
  replayed weather, the plan) continues from the right date, but books
  no energy and no revenue for them: the record of a projected run says
  which days were ticked and which were skipped (`ticks` and
  `skipped_days`). The alternative, extrapolating the meters, would put
  numbers on a record that no meter measured.
- **D3, a preset is the typical day repeated.** A plant aged N years at
  commissioning is built by projecting a bundled typical stress day per
  rack, scaled by the rack's own `rate_scale`; no artifact is needed, the
  preset is a function of the seed and the years. The typical day's
  figures are the clean year's ledger means, rounded and labeled; the
  alternative, bundling the clean year's stress year as data, was
  rejected as a generated artifact that would need regenerating with
  every physics change.
- **D4, the scenario asks for age in `run`, not as an event.** Age is a
  property of the plant the case starts on, like the seed and the start
  time; `RunParams::age_years` sits beside them and the headless record
  carries it. A `site.age` event was rejected because an event on tick
  zero that rewrites every rack's state is a preset wearing a fault's
  clothes.
- **D5, the bench projects ten years by default when asked for the
  trajectory**, writing `calibration/m5-aging.json` with the per-year
  figures and the two laboratory trajectories (a rack at two cycles a
  day in a 25 °C container to its 80 % crossing, for the laboratory
  band; the same at 35 °C, reported). The gate itself is phase 4's.

## Signal map delta

None. The shell's `--age-years` changes the plant the map reads, not the
map.

## Checkpoint impact

Format 20 widened: `RackAge::skipped_days` (the days a rack aged without
being ticked, so a record can say it), no other field. The stress year
is not in the file.

## Test plan

- `fast_forward(&year, 0)` leaves the tree byte-identical.
- A fresh plant fast-forwarded over the measured stress year lands
  within 1e-6 of the ticked year's aging states (same arithmetic).
- `new_aged(.., 5)` yields a mean state of health inside the README's
  band and a spread across racks from `rate_scale`.
- A scenario with `run.age_years` resolves to the aged plant and its
  headless record carries the value; a record from a fresh plant does
  not change (the field is skipped when zero).
- The projection's digest is identical on macOS and Linux (CI's
  determinism job covers a projected decade).
