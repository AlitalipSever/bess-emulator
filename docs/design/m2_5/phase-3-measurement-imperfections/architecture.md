# M2.5 phase 3: measurement imperfections, architecture

What exists after this phase: everything the plant publishes has passed
through an instrument, and the instruments have classes. The M0 design
gave every signal an LSB, a noise sigma and a range clamp; this phase
gives the error model the two dimensions it lacked, slow offset drift
and accuracy that depends on load, and ties both to the classes real
metering claims.

## The error model

Per published signal, on top of the existing quantization and noise:

- **Offset drift.** A seeded slow random walk per instrument, bounded
  by the instrument's class budget. Two meters measuring the same
  quantity now disagree by a slowly wandering amount, which is the
  "why don't these two sum" problem every plant data pipeline meets in
  its first week.
- **Load-dependent error.** Metering error grows at low load; that is
  the reason accuracy classes with the S suffix exist. The revenue
  meter (0.2S) and the auxiliary meter (0.5S) get the class curve, so
  a plant idling at night is measured worse, relatively, than one
  cycling at noon.
- **Device timestamps keep their M0 semantics** (drift as a scenario
  amplifies it); this phase adds the baseline wander that is always
  there, small, per instrument.

The model lives where the M0 resolution model lives, in the projection
layer's per-point metadata, not in the kernel: physics stays exact,
measurement is a property of publication. The tap from M2 phase 5
corrupts observations on demand; this layer is the honest instrument
noise that is never off. The two compose: scenario faults apply to
already-instrumented values, as in reality.

## Which instruments exist

An instrument inventory, small and explicit, mirroring the house-load
inventory's spirit: the POI revenue meter (0.2S), the auxiliary meter
(0.5S), CT/VT-fed SCADA analogs at the substation, rack-level BMS
measurement chains (the current sensor phase 2's estimator already
inherits its offset from, formalized here so estimator and telemetry
share one sensor, not two coincidentally similar ones). Each published
point names its instrument; points without a physical instrument
(counters, states, config) carry none and stay exact.

## Invariants

- Kernel state and physics invariants are untouched: the digest of
  kernel state is identical with instrumentation on or off.
- Every instrumented point's long-run error stays inside its class
  budget at every load level; the class table is enforceable, and
  enforced.
- Determinism: instrument walks are seeded per (seed, instrument), so
  the tuple still fully determines every published byte.
