# M5 phase 2: time compression, architecture

What exists after this phase: a run that can skip days. A simulated
year leaves a stress year behind (one closed `StressDay` per rack per
day); the kernel can fast-forward through it as many times as asked,
advancing every rack's aging states and the clock while the rest of the
tree stands still. The bench runs the measured year and projects it over
a decade in seconds; the shell and a scenario file can start from a
plant that is N years old.

## The model

- **The stress year.** `bess-core/src/kernel/stress_year.rs`:
  `StressYear { racks: Vec<Vec<StressDay>> }`, indexed rack then day,
  collected by `StressYear::observe(&SiteState)` after each tick from
  the racks' `last_day` (a day is taken once: the collector keeps the
  count of days it has seen per rack). About 480 racks by 365 days by
  six numbers; it lives in the bench and in the shell's run, not in the
  checkpoint. A typical day for presets is a constant:
  `StressDay::gw01_typical()` (0.77 cycles a day, a mean state of charge
  of 0.5, 24 °C, 0.8 depth, 0.5C; labeled against the clean year's
  ledger in CALIBRATION.md).
- **Fast-forward.** `Simulation::fast_forward(&mut self, year:
  &StressYear, days: u64) -> &[Event]`: for each day, for each rack, the
  model ages the rack with the rack's own row (day modulo the year's
  length) scaled by `rate_scale`; `age.days` and `age.efc` advance; the
  clock moves by the days; `soh` and the lost stored energy are written
  as the daily pass writes them; one event `Event::FastForwarded { days
  }` closes it. A rack's open ledger is kept (a fast-forward between two
  ticks does not lose the part-day). The EMS, the thermal states, the
  reservoir and the meters are untouched: the skipped days are not
  metered, which the record says.
- **A plant aged at commissioning.** `Simulation::new_aged(cfg, models,
  seed, start_unix_s, years)`: `new` followed by a fast-forward over
  `years * 365` days of the typical day per rack, the clock left at
  `start_unix_s` (the age is in the racks, the calendar is the run's).
  Every rack's `rate_scale` applies, so the preset carries the spread.
- **Surfaces of the phase.** The bench: `--years N` runs the measured
  year, then projects `N - 1` more from its stress year, writing the
  trajectory (`calibration/m5-aging.json`: per year, the mean, minimum
  and maximum state of health, the cumulative cycles, the mean resistance
  growth) beside the annual record; `--resimulate` runs the physics every
  year instead (hours; never in CI). The shell: `--age-years N`. Scenario
  files: `run: { age_years: N }` in `RunParams`, carried in the headless
  record (`age_years`) so a record names the plant it was taken on.

## Data flow

```
year:    tick ... tick   -> racks close days -> StressYear::observe
project: fast_forward(&year, 365 * (N - 1))  -> aging states, clock
preset:  new_aged(.., years) = new + fast_forward(typical, years * 365)
```

## Invariants

- A fast-forward of zero days is a no-op on the tree.
- A fast-forward over a year of the measured stress year, applied to a
  fresh plant, lands within the model's integration error of the
  measured year's own aging states (the daily pass and the projection
  are the same arithmetic).
- Determinism: the projection's digest is identical on both platforms.
- The checkpoint carries the age of a projected plant like any other
  state; the stress year itself is never in the file.
