# M2.5 phase 1: OCV hysteresis, design

Each decision is proposed here and confirmed or revised in its PR.
As built in PR1 (2026-10-21): D1, D4 and D5 confirmed as proposed; D2
and D3 confirmed with the source situation stated under each.

## Decisions

- **D1, Plett one-state, not zero-state, not multi-state.** The
  zero-state variant (instant flip on sign change) is free but wrong at
  rest, and rest is where phase 2's estimator drama happens. Multi-state
  and Preisach-style models chase minor loops nobody can observe through
  a register. One state per rack is the observable-behavior optimum.
- **D2, the envelope comes from a published curve pair.** `h_max(soc)`
  is fitted to the gap between charge and discharge OCV curves in a
  published LFP characterization (the dossier behind this phase lists
  candidates; typical mid-plateau separation is 20 to 30 mV). The gate
  holds the modeled gap to the source's gap at stated SoC points within
  a stated tolerance, both fixed when the source is pinned in PR1,
  before measurement, per the band discipline.
  *As built:* Barai et al. 2015 (J. Power Sources 295) pinned, a 20 Ah
  LFP pouch at 25 C. Its text gives the peak (38 mV near 5 % SoC) and
  where the maximum lies; the plateau level is not in text, so the 50 %
  gate is a sanity bound and says so. Bands: 32 to 44 mV at 5 %, 10 to
  25 mV at 50 %. Measured 35.9 and 19.9.
- **D3, `gamma` from settling, not guessed.** The rate constant is set
  so the model's transition throughput (how much charge moves `h` most
  of the way across the envelope) matches the source publication's
  protocol; recorded with the fit in CALIBRATION.md.
- **D4, checkpoint bump.** `h` is state; format bumps, old files
  rejected by version, release notes say so. *As built:* format 9,
  `RackState::hysteresis_v`, rack-level volts.
- **D5, no signal map change.** Voltage points keep address, unit,
  meaning; their values simply become path-dependent. Trajectory
  baselines recorded against v0.5.0 will differ, which goes in the
  release-note inventory, not the map version.

## Signal map impact

None (D5).

## Checkpoint impact

One format bump (D4).

## Test plan

- **Property: envelope containment and sign-chasing** (architecture
  invariants).
- **Property: energy conservation exact** over random cycle sequences.
- **Unit: switch-off equivalence,** `gamma = 0` digest equals baseline.
- **Gate: curve-pair fidelity** at the pinned SoC points (CI test
  against the fitted constants and the source table).
- **Golden snapshot:** regenerated once (state field added), called
  out. *As built:* 0xf3fb_8ad4_75c9_980f to 0x95c3_aa5c_e7bb_3679, a
  physics and schema change at once; the switch-off proof holds the old
  value with the field stripped.
- **Annual run:** regenerated; round-trip moves at most marginally
  (hysteresis dissipation is real but small); the delta is explained in
  the PR.
