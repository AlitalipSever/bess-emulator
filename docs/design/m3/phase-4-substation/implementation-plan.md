# M3 phase 4: substation depth, implementation plan

Three PRs, each landing green.

## PR1: interlocks and the sequence

- `SubstationState` positions, sequence and timer; the HV breaker
  closes through the sequence; the return waits for it; points and
  checkpoint bump; the trip case regenerated.

Accept: after a protection trip the site's breaker closes only after
the sequence, and the journal shows the sequence states.

*As built (PR #44):* as the design's as-built notes say. Scenario kinds
for the switchgear and a maintenance case `substation-earthing-work`
came with it, so the sequence and the interlocks are visible in the
library and not only in a test. No shell command surface yet (REST and
Modbus keep reset, maintenance, HVAC and trip); the switchgear is driven
by scenarios and the kernel API, a candidate for the release PR.

## PR2: the transformer and the tap changer

- IEC 60076-7 model in `SimpleGrid` (behind `GridInterface`), the two
  temperatures and bits, the OLTC and the MV bus voltage, the new
  library case.

Accept: the IEC worked example reproduced; the hot-week record shows
the hot spot's daily swing; the MV bus voltage steps in the published
step.

*As built (PR #45):* as the design's as-built notes say. The worked
example is not reproduced (the standard's text, not public); the rated
rises are the gate. The hot-week case's header carries the hot spot's
swing; no new case. The MV bus steps in 1.25 % and the tap follows the
synthetic ±2 % daily swing of the grid voltage, a new kernel input.

## PR3: release v0.8.0

- Version bump, records regenerated (version only), ROADMAP M3 done,
  README status, COMPATIBILITY map 0.5.0 history row, release notes,
  CALIBRATION.md M3 gates and the annual record at release.

Accept: tag pushed by the maintainer, quickstart green, CALIBRATION.md
carries the five M3 gates with sources and dates.

## Open questions

- Which public transformer datasheet pins the OLTC (README open
  question).
- Whether the synthetic grid voltage input is enough to exercise the
  regulator, or whether phase 4 should pull a replayed voltage series
  forward from M4. Leans synthetic with a daily swing, stated; the
  replayed series is M4's.
