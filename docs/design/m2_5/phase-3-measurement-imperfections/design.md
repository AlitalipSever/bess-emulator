# M2.5 phase 3: measurement imperfections, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, classes from the IEC families the plant would be built to.**
  Meter classes per IEC 62053 (0.2S revenue, 0.5S auxiliary),
  instrument transformer contributions per IEC 61869; the combined
  error budget per measurement chain is the documented sum, and the
  emulator's parameters are sized to consume a stated fraction of the
  budget, never more. The class limit values are public standard
  content and go in CALIBRATION.md sources with the edition cited.
- **D2, error shape: white noise plus bounded offset walk plus the
  S-curve.** Noise sigma stays per point as in M0; the walk is
  Ornstein-Uhlenbeck-style with a long time constant, clamped to the
  class budget; the load-dependence multiplies error at low load per
  the class's published curve points. Three components, each visible
  in a different analysis (spectrum, day-to-day baseline, load sweep),
  which is what makes the model teachable.
- **D3, one sensor, two consumers.** The rack current sensor's offset
  and gain parameters are defined once in the instrument inventory;
  the phase 2 estimator and the published rack current telemetry both
  read them. Estimator drift and telemetry error now correlate, as
  they do in a real rack, where they are the same copper.
- **D4, exact points stay exact, and the meters are not among them.**
  Counters, states, enums and config carry no instrument. A meter's
  energy register is instrumented by construction: it integrates its
  own measured power, so the published energy drifts within the class
  budget while the kernel's true energy accounting stays exact. The
  list of instrumented points is the inventory, not a heuristic.
- **D5, no checkpoint change for walks if avoidable.** Walk state is
  publication-layer state; proposal is to derive it deterministically
  from (seed, instrument, tick) so it needs no persistence and resume
  reproduces it exactly. If derivation proves awkward, walk state
  enters the shell checkpoint story instead of the kernel's, decided
  in PR1 and recorded here.

## Signal map impact

No addresses move. The map's documentation columns gain the instrument
and class per point, which is additive metadata; consumers finally get
told how good a number claims to be, which real signal lists do state.

## Checkpoint impact

None in the kernel (D5, target outcome).

## Test plan

- **Property: kernel unaware.** State digest identical with
  instrumentation on and off.
- **Gate: class compliance.** A replayed year's instrumented series
  versus truth: error within class budget at the standard's stated
  load points, for every instrumented chain; enforced in CI via bench.
- **Unit: the S-curve.** Relative error at 5 percent load exceeds
  error at rated load, within the class's own ratio.
- **Integration: two meters disagree.** Revenue and SCADA POI series
  differ by a bounded, wandering, load-dependent amount; the bench
  emits the reconciliation table a data engineer would build, and the
  M2.5 CALIBRATION.md entry prints it.
- **Determinism:** byte-identical published output across runs of the
  same tuple.
