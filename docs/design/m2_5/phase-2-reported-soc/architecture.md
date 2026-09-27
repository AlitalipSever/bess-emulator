# M2.5 phase 2: reported SoC, architecture

What exists after this phase: the plant stops publishing the truth about
its own state of charge, because no real plant can. The kernel keeps the
true SoC as internal state; what SCADA, Modbus and MQTT carry is the
BMS's estimate, produced by a behavioral model of how estimation
actually degrades: it drifts, and then it snaps.

## The estimator

Lives in the BMS layer, which is where real ones live. Per rack, a
behavioral model rather than a full filter:

- **Coulomb counting with an imperfect sensor.** The estimator
  integrates the rack current as measured, not as it is: a per-rack
  seeded offset and gain error (phase 3 formalizes the sensor model;
  this phase installs the two parameters it will inherit) makes the
  estimate walk away from the truth at a rate proportional to throughput
  and time.
- **Weak voltage correction on the plateau.** Where the OCV curve is
  flat and hysteresis (phase 1) makes rested voltage ambiguous, voltage
  pulls the estimate only feebly. The drift dominates mid-plateau, which
  is the LFP signature.
- **Resnap at the knees.** When the rack leaves the plateau (SoC
  approaches a window edge, where OCV becomes steep and unambiguous),
  the estimator corrects hard: the reported SoC jumps to near-truth,
  sometimes by whole percentage points. Anyone who has watched a real
  LFP fleet knows this sawtooth; producing it is the phase's acceptance
  bar.

A full EKF was considered and rejected, per "model to the interface": an
EKF's internals are invisible through a register, and its behavior at
this observability (drift, then correction at the knees) is exactly what
the behavioral model produces at a fraction of the state and none of
the tuning opacity.

## Truth and estimate in the tree

Both live in the state tree: `soc` (truth, the kernel's own quantity,
which physics and invariants keep using) and `soc_reported` (the
estimator's output). The published SCADA surfaces carry the reported
value at the existing SoC addresses and topics; the truth leaves those
surfaces. The research surfaces (Parquet, bench) carry both, because
measuring the estimator's error is a calibration activity and the whole
point of keeping a truth.

Site and block SoC aggregates follow the same rule: aggregates of
reported values, because that is what a real EMS aggregates.

## What the kernel still believes

Dispatch, BMS limits and derating keep reading the truth in M2.5. A real
BMS acts on its own estimate, and routing the estimate into the limit
chain is the honest end state, but it couples estimator error into
plant behavior and therefore into every calibration number at once.
That coupling is deliberately deferred to a later iteration, so this
phase changes what the plant says before it changes what the plant
does. The gap is stated in CALIBRATION.md's known-gaps list.

## Invariants

- The truth never leaves the kernel's physics: energy conservation and
  SoC bounds keep binding on `soc`, untouched by anything this phase
  adds.
- `soc_reported` is continuous except at resnap events, and resnap
  events appear in the kernel event log (they are the estimator
  admitting error, and real BMS logs show them).
- Determinism: the estimator's error trajectory is a pure function of
  the tuple, like everything else.
