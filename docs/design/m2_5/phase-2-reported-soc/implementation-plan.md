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

## Open questions

- Whether aggregates should be capacity-weighted or plain means of
  reported values; decided by what the M0 aggregation already does for
  truth (mirror it, whichever it is).
- Confidence signal precedent (README open question, D4).
