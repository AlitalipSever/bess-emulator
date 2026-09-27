# M2.5 phase 3: measurement imperfections, implementation plan

Two PRs, each landing green.

## PR1: the instrument inventory and error model

- Instrument inventory with classes and budgets (design D1);
  per-point instrument assignment; exact points listed as exact (D4).
- Error components: walk, S-curve, composed with existing noise and
  LSB (D2); derivation-not-state outcome decided and recorded (D5).
- One-sensor rule wired: estimator and telemetry share the rack
  current sensor parameters (D3).
- Tests: kernel-unaware digest, S-curve unit, determinism.

Accept: the two-meters-disagree integration test shows a bounded
wandering difference, and switching instrumentation off restores
pre-phase published bytes.

## PR2: gates, docs, release v0.6.0

- Class-compliance gate in bench and CI; reconciliation table in the
  M2.5 CALIBRATION.md entry; map documentation columns for instrument
  and class.
- COMPATIBILITY.md and release notes: the iteration's inventory
  (headline: published SoC is an estimate now), version to 0.6.0,
  annual record regenerated, ROADMAP.md flips M2.5 to done.

Accept: tag pushed, quickstart green, CALIBRATION.md carries the three
phase gates with sources and dates.

## Open questions

- Whether CT/VT chains at the substation get their own class rows in
  v1 or fold into one SCADA-analog chain per quantity; leans fold,
  split when a consumer asks for per-transformer detail.
