# M2.5: SoC realism mini-iteration

Status: draft, designed alongside M2, implemented after it. A
mini-iteration in the M0.5/M1.5 tradition: three small phases, each
deepening exactly one thing, so the iteration discipline holds where a
combined M2 would have bent it (the hysteresis work touches the cell
model, not the BMS).

## Goal

Today the plant publishes three kinds of truth no real plant can offer:
cell voltages too clean to be measured, a SoC that is exact by
construction, and sensors without error. After M2.5 the published plant
is honest about all three, and anyone testing estimation or
data-quality software against it faces the same signals the field would
give them. This is the realism backlog's cheapest credibility per line
of code, which is why it earns its own iteration instead of waiting
inside a bigger milestone.

The three phases are a chain, in dependency order: hysteresis makes
voltage-based SoC estimation genuinely hard, the estimator then has a
real problem to be honestly bad at, and the measurement layer makes
everything the plant says pass through instruments.

## Phases

| Phase | Deepens | Depends on |
|---|---|---|
| [1: OCV hysteresis](phase-1-ocv-hysteresis/) | `CellModel` | nothing |
| [2: Reported SoC](phase-2-reported-soc/) | `BmsLogic` (the estimator) | 1 |
| [3: Measurement imperfections](phase-3-measurement-imperfections/) | the projection layer's error model | 2 (the estimator's current sensor is a measurement) |

## Calibration gates

Per phase, each against public data, recorded in CALIBRATION.md:

1. The modeled charge/discharge OCV separation matches a published LFP
   curve pair within a stated tolerance at stated SoC points.
2. Estimator drift rates and correction-jump magnitudes fall inside
   bands from published field and laboratory observations, or, where no
   source exists at plant scale, are published with a sanity bound that
   says what it is (the M1 aux-share precedent).
3. Metering errors stay inside the accuracy classes the plant claims
   (0.2S revenue, 0.5S auxiliary), per the IEC error budgets.

## Versioning

- **Crate:** v0.6.0 at iteration end.
- **Signal map:** the headline change is a meaning change: the published
  SoC becomes the BMS's reported estimate, the internal truth leaves the
  SCADA surfaces (research profile and bench keep it). That is the
  honest-interface decision this iteration exists for, it is breaking,
  and it is called out the way M2's alarm change was. Additions: SoC
  confidence/quality if phase 2's sources justify one.
- **Checkpoint:** bumps per phase as state lands (hysteresis state,
  estimator state, sensor offset walks).

## Release-note inventory for v0.6.0

Filled as PRs land.

- (none yet)

## Open questions

- Which published LFP curve pair pins the hysteresis gate (phase 1).
- Whether reported SoC replaces the published point or ships beside it
  for one release before the switch (phase 2 design argues replacement;
  the two-point alternative is the fallback if review disagrees).
- Whether a SoC quality/confidence signal has a public precedent to
  model against, or stays out per "never emit what we cannot calibrate"
  (phase 2).
