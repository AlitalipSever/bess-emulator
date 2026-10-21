# M2.5: SoC realism mini-iteration

Status: in progress. Phase 1 landed 2026-10-21 (PR1); phase 2 PR1 and PR2
the same day; phase 3 PR1 the same day; PR2 is the v0.7.0 release. Designed alongside M2, implemented after it. A
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

- **Crate:** v0.7.0 at iteration end. (v0.6.0 went to M2.6, the alarm
  view mini-iteration, which shipped first.)
- **Signal map:** 0.4.0 as built in phase 2 PR2. The headline change is a meaning change: the published
  SoC becomes the BMS's reported estimate, the internal truth leaves the
  SCADA surfaces (research profile and bench keep it). That is the
  honest-interface decision this iteration exists for, it is breaking,
  and it is called out the way M2's alarm change was. Additions: SoC
  confidence/quality if phase 2's sources justify one.
- **Checkpoint:** bumps as state lands: hysteresis state (phase 1) and
  estimator state (phase 2). Phase 3 targets no bump: its offset walks
  are derived, not stored (its design D5).

## Release-note inventory for v0.7.0

Filled as PRs land.

- **Rested voltage depends on history** (phase 1 PR1). Every rack
  carries an OCV hysteresis state; after charging a rested rack reads up
  to 19 mV per cell above the mean curve at the low knee and about
  10 mV on the plateau, after discharging the same below. Rack and cell
  voltage points keep their addresses and units; any baseline recorded
  against v0.6.1 voltages will differ, and a client that inferred SoC
  from rested voltage now gets a path-dependent answer, which is the
  point. The 0.5C cycle RTE moved 0.917 to 0.9129.
- **Checkpoint format 9** (phase 1 PR1). Racks carry `hysteresis_v`;
  v8 files are refused by version.
- **A new event, `soc_resnap`** (phase 2 PR1). A rack's BMS correcting
  its reported SoC at a knee of the OCV curve is an event on every
  surface: MQTT `events/<rack>/soc_resnap` with `from_pct` and `to_pct`,
  the event counter, the viewer's journal. On the replayed year it fires
  about once per rack and day, so an event consumer that counted only
  alarm edges now sees a steady stream beside them.
- **Checkpoint format 10** (phase 2 PR1). Racks carry `soc_reported`,
  `sensor_offset_a` and `sensor_gain`; v9 files are refused.
- **The research record gains an estimator section** (phase 2 PR1):
  mean and worst reported-SoC error and the correction count, with two
  sanity gates.
- **The published SoC is an estimate now, map 0.4.0** (phase 2 PR2, the
  headline). `site.soc_pct`, `blockNN.soc_pct`, their MQTT mirrors, the
  REST summary's `soc` and `bess_site_soc_ratio` carry what the BMSs
  report: on the replayed year it sits 0.25 points from the truth on
  average, up to 1.1 on a rack, and corrects itself by jumps at the
  knees. Same addresses, units and scales. A client that treated the
  point as an oracle, for a round-trip figure or a schedule, now sees
  what a real gateway gives it; the truth is on the research surfaces
  only (the annual record and the study series).
- **Every published value passes through an instrument** (phase 3 PR1).
  Fifteen site points and five per block carry their meter's error,
  always on: noise, a slow wander and a limit that grows at low load,
  inside IEC classes 0.2S (revenue), 0.5S (house load), 0.5 (SCADA
  analogs, PCS power) and Pt100 class B (temperatures). The signal map
  CSV names the instrument and class per point in two new columns;
  `exact` points (counters, states, the BMS's own estimates, frequency,
  irradiance) are unchanged. Two readings of one quantity no longer
  agree to the digit; a pipeline that reconciled them exactly will see
  a bounded, wandering difference. `--exact-instruments` restores the
  pre-phase bytes for research.
- **Library records regenerated** (phase 1 PR1): all fifteen digests
  moved with the physics; `hvac-failure-both-units`,
  `hvac-failure-hot-week` and `rack-self-discharge-drift` changed their
  event counts by a handful of derate events.

## Open questions

- ~~Which published LFP curve pair pins the hysteresis gate (phase 1).~~
  Barai et al. 2015; the plateau level stays a labeled estimate until a
  source prints it in text (CALIBRATION.md, M2.5 known gaps).
- Whether reported SoC replaces the published point or ships beside it
  for one release before the switch (phase 2 design argues replacement;
  the two-point alternative is the fallback if review disagrees).
- Whether a SoC quality/confidence signal has a public precedent to
  model against, or stays out per "never emit what we cannot calibrate"
  (phase 2).
