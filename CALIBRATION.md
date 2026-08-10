# Calibration record

Realism in this project is never claimed, always measured: every milestone
states a target against public data, and this file records what was measured,
with what sources, at which release. The gates listed here run in CI; a
regression fails the build.

This file is currently maintained by hand. The `bess-bench` harness described
in ARCHITECTURE.md will regenerate it automatically once it exists.

## M0 (v0.1.0): energy balance and initial round-trip band

- **Gate:** energy conservation, SoC bounds, and meter monotonicity as CI
  property tests; point-of-interconnection round-trip efficiency of a
  full-depth 0.5C cycle inside [0.87, 0.90] with the flat 97.5% PCS
  placeholder.
- **Measured:** RTE 0.897 with the flat PCS (import 173.9 MWh, export
  156.0 MWh; re-measured on the v0.2.0 kernel with `FlatPcs` swapped back
  in, since this file did not exist at v0.1.0).
- **Sources:** none external yet; the band was an engineering estimate for a
  system with conversion and transformer losses but no thermal/auxiliary
  modeling.

## M0.5 (v0.2.0): PCS partial-load efficiency curve

- **Gate 1, curve fit:** `CurvePcs::cec_utility_reference` coefficients
  (k0 = 2.464096e-3, k1 = -1.402627e-3, k2 = 2.144562e-2, per-unit of AC
  rating) must reproduce the reference efficiency curve within 0.3
  percentage points at the 5/10/20/30/50/75/100% load points. Enforced by
  `curve_matches_the_cec_reference_within_the_gate` in
  `crates/bess-models/src/pcs.rs`.
  - **Reference:** Sungrow SC2500UD-US {900V}, Sandia performance-model
    coefficients from the CEC inverter database as distributed with NREL SAM
    (see DATA-LICENSES.md), evaluated at nominal DC voltage.
  - **Measured:** worst error 0.037 pp (at the 5% point). CEC-weighted
    efficiency of the fit: 98.33%; Euro-weighted: 98.27%.
- **Gate 2, round-trip re-measurement:** the 0.5C full-depth cycle gate
  moved from [0.87, 0.90] to [0.90, 0.93].
  - **Measured:** RTE 0.917 (import 172.0 MWh, export 157.8 MWh).
  - **Why the band moved:** the flat 97.5% placeholder underestimated
    conversion efficiency at the 50% load this cycle runs at (the real curve
    sits near 98.6% one-way there), so replacing it raised the measured
    value from 0.897 to 0.917. The new value still sits inside the 88-94% nameplate
    band for modern LFP systems. The 80-85% field band remains the M1 gate:
    thermal and auxiliary losses are the missing components, and they are
    M1 scope, not tunable knobs to force this gate to stay put.
- **Known simplifications (tracked for M3):** no V_dc dependence (the
  Sandia C1-C3 terms are dropped at nominal voltage), charge direction
  reuses the discharge curve, no operating state machine or setpoint
  dynamics. The three-term polynomial is a fit family, not a strict physical
  loss decomposition; k1 fitting slightly negative is expected and
  documented in `crates/bess-models/src/pcs.rs`.

## Planned gates (from ROADMAP.md)

- **M1:** annual RTE in the 80-85% field band (CAISO/EPRI fleet reports);
  realistic auxiliary share of throughput.
- **M2:** failure type/frequency distribution follows the EPRI incident
  taxonomy.
- **M3:** efficiency surfaces f(P, V_dc) match the Sandia/CEC database; the
  M1 RTE gate still holds.
- **M4:** simulated annual revenue inside public German fleet index bands.
- **M5:** capacity fade inside published LFP field bands.
