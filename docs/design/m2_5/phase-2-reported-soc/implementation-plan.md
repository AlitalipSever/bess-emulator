# M2.5 phase 2: reported SoC, implementation plan

Two PRs, each landing green.

## PR1: the estimator

- Estimator state and step in the BMS layer: imperfect coulomb count,
  weak plateau correction, knee resnap with `SocResnapped` events
  (design D2, D3); parameters sourced and recorded.
- `soc_reported` in the tree; research surfaces carry both; checkpoint
  bump (D5).
- Tests: decoupling digest, drift property, resnap unit, sawtooth
  integration.

Accept: the sawtooth chart from a replayed cycling week, truth smooth
underneath.

*As built (feature/m2_5-phase2-pr1, 2026-10-21):* `bms/estimator.rs`,
`BasicBms::estimator` with `with_perfect_estimator()`; `BmsFlows::soc_resnap`
carries the jump to the kernel, which emits `Event::SocResnapped`; the
shell's MQTT topic and payload, and the viewer's journal row; bench gains
an estimator section (mean and worst error, corrections) with two sanity
gates; checkpoint 10; tests `tests/reported_soc.rs` (decoupling, plateau
drift, a replayed week's sawtooth) and unit tests in the estimator. The
acceptance chart is the week test's printed line and the annual gate
figures in CALIBRATION.md; a drawn chart is left for the release piece.

## PR2: the switch

- Published SoC points and aggregates carry the reported value (D1);
  the truth leaves SCADA surfaces.
- COMPATIBILITY.md history row drafted; release-note inventory entry
  written (the headline one).
- Grafana: the SoC panel gains the reported-versus-truth overlay in the
  research view only; the default dashboard shows what SCADA would see.
- Examples and README updated where they said "SoC" and meant truth.

Accept: a Modbus poll and an MQTT subscribe both return the estimate;
bench output still measures estimator error against truth.

*As built (feature/m2_5-phase2-pr2, 2026-10-21):* the four site/block
points, REST and Prometheus switched; map 0.4.0 with its COMPATIBILITY.md
row; the viewer's site bar and rack rows show the reported value; the
study series carries `soc_true_pct` / `soc_reported_pct` and the rack 0
pair, regenerated; Python example comment. Grafana is left as the SCADA
view on purpose: Prometheus carries no truth, so an overlay there would
have meant publishing the oracle one metric over, which D1 forbids; the
overlay is the series' job.

## Open questions

- Whether aggregates should be capacity-weighted or plain means of
  reported values; decided by what the M0 aggregation already does for
  truth (mirror it, whichever it is). *Resolved in PR1:* plain means over
  in-service racks, `average_soc_reported` beside `average_soc`.
- Confidence signal precedent (README open question, D4).
