# M5 phase 2: time compression, implementation plan

One PR.

## PR1: the stress year, the fast-forward and the presets

- `bess-core`: `kernel/stress_year.rs` (`StressYear`, `observe`),
  `Simulation::fast_forward`, `Simulation::new_aged`,
  `StressDay::gw01_typical`, `Event::FastForwarded`,
  `RackAge::skipped_days`, checkpoint 20 widened.
- `bess-scenario`: `RunParams::age_years`, the `run.age_years` key,
  validation (0 to 20).
- `bess-emulator`: `--age-years` on the shell; the headless record's
  `age_years`; `Record::skipped_days` where a case fast-forwards (none
  in the library yet).
- `bess-bench`: `--years N` and `--resimulate`; the `StressYear`
  collector in the run; `calibration/m5-aging.json` written with
  `--write`; the trajectory and the two laboratory trajectories in the
  report's render.
- CALIBRATION.md: "M5: the projected decade" with the typical day's
  derivation from the clean year's ledger and the feedback gap measured
  once with `--resimulate` over three years locally (stated as a local
  measurement).

Accept: the phase's tests pass; the bench's `--years 10` finishes in
under a minute after the year on the CI runner; the annual record does
not move (a projection reads the year, it does not change it); CI green.

*As built (PR #59):* as the design's as-built notes say. The golden
digest moved to 0xb468_2140_6b23_a368 and the library records
regenerated for `skipped_days` in the tree. `--years` is accepted beside
`--write` and `--check` (it does not change the published run); the
projection of nine more years took under a second after the year.

## Open questions

- Whether `--resimulate` should carry the aged racks through a
  checkpoint between years (a file per year, resumable) or run in one
  process; leans one process with the checkpoint written per year as a
  by-product, so a user can resume year six from year five's file.
