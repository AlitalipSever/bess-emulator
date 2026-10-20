# M2.5 phase 2: reported SoC, design

Each decision is proposed here and confirmed or revised in its PR.
As built in PR1 (2026-10-21): D2, D3, D5 confirmed; D1 and D4 are PR2's
and stand as proposed.

## Decisions

- **D1, the published SoC point becomes the reported value.** Same
  addresses, same topics, new meaning: what a real gateway would give
  you. The alternative, publishing `soc_reported` beside a still-public
  truth, keeps integrators comfortable and defeats the purpose: nobody
  hardens a pipeline against an estimate while the oracle sits one
  register over. Breaking meaning change, called out like M2's alarm
  bits; this is the iteration's headline break and the reason v0.6.0
  exists.
- **D2, drift parameters from sensor specifications, jump behavior
  from published observations.** The offset and gain error ranges come
  from public current-sensor accuracy specifications of the class used
  in rack BMS hardware; the resnap threshold and correction gain are
  tuned so drift-between-corrections and jump sizes land inside what
  published LFP estimation studies report. Where the literature gives
  laboratory rather than fleet numbers, the gate says so and holds a
  sanity band instead, the M1 aux-share precedent, stated in
  CALIBRATION.md. *As built:* offset and gain bounds from the IVT-S
  ±500 A column (75 mA, 0.4 %); knees at 10 and 90 % from the OCV table;
  threshold 0.5 points, residual a fifth, plateau pull ten days, all
  labeled estimates; the annual gate holds mean and worst error to
  sanity bounds whose ceiling is ACCURE's field range (15 points).
- **D3, resnap is an event.** `SocResnapped { rack, from, to }` in the
  kernel event log, so the sawtooth is observable as discrete facts and
  scenario assertions can count them. *As built:* `SocResnapped { block,
  container, rack, jump }` with the jump in per mille of SoC (integers,
  so the event stays `Eq` and the log comparable); MQTT topic
  `events/<rack>/soc_resnap`, payload `from_pct` / `to_pct`; a journal
  row in the viewer.
- **D4, no confidence signal in v1.** Real BMS SoC-quality indicators
  exist but their semantics are vendor-proprietary; emitting one we
  cannot calibrate violates the signal policy. Revisit if a public
  precedent (a standard or a published gateway spec) surfaces; the
  README's open question tracks it.
- **D5, checkpoint bump.** Estimator state per rack (reported value,
  accumulated sensor error walk) is state. *As built:* format 10;
  `soc_reported`, `sensor_offset_a`, `sensor_gain` on the rack, the two
  sensor parameters drawn from a stream of their own so no earlier draw
  moved.

## Signal map impact

Meaning change per D1, no address movement, no additions. Map version
moves at the v0.6.0 release with the change called out in
COMPATIBILITY.md's history table.

## Checkpoint impact

One format bump (D5).

## Test plan

- **Property: truth untouched.** Physics invariants on `soc` are
  unchanged by construction; a digest-equality test on the truth
  trajectory with the estimator on and off proves the decoupling.
- **Property: drift direction and rate.** With a seeded positive offset,
  reported walks away from truth at the predicted rate mid-plateau;
  correction never occurs mid-plateau under steady cycling within the
  window.
- **Unit: resnap.** Driving a rack to a window edge triggers the event
  and closes most of the accumulated error; magnitudes within the D2
  band.
- **Integration: the sawtooth.** A replayed cycling week produces the
  drift-and-snap shape on `soc_reported` while `soc` stays smooth; the
  chart lands in CALIBRATION.md's M2.5 entry.
- **Golden snapshot:** regenerated once for the new state, called out.
  *As built:* 0x95c3_aa5c_e7bb_3679 to 0xe46c_f1ee_8656_cf35, schema and
  event log only; the decoupling proof holds the physics.
