# M5 phase 4: calibration and release, architecture

What exists after this phase: a gate on the trajectory in CI, the
milestone's record in CALIBRATION.md, and v0.10.0.

## The model

- **The gate.** `bess-bench/src/bands.rs` gains `AGING_SOH_YEAR5 { lo:
  0.82, hi: 0.93 }` and `AGING_EFC_TO_80_PCT { lo: 2500, hi: 9000 }`;
  `check_aging` reads `calibration/m5-aging.json` and holds the clean
  year's projected year-five mean against the first and the two-cycle
  laboratory trajectory's 80 % crossing against the second. The bands
  are written in CALIBRATION.md before the number, with their
  derivation (README).
- **CI.** The calibration job, after the clean year, projects the
  decade (seconds) and runs the gate; `--check` fails on drift of the
  trajectory file the way it fails on the annual record. No new job: the
  projection is cheap.
- **The documents.** CALIBRATION.md: "M5: the aging model", "M5: the
  projected decade", "M5: the trajectory against the bands", "M5: the
  annual record at release". ROADMAP.md marks M5 done. README.md's
  status sentence. The release note from the inventory.

## Invariants

- A gate that does not fail is not a gate: the band test includes a
  trajectory outside each band and sees the check refuse it.
- The release regenerates every record loudly (the engine version is in
  each).
