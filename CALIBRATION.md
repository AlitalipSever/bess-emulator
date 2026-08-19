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

## M1 (in progress): thermal parameter provenance

The M1 gate itself, annual round-trip efficiency and auxiliary share, is
measured by `bess-bench` at the end of the milestone and replaces this
section. Until then this is the inventory of what the thermal model actually
runs on, written down now because the parameters landed before the
measurements did. An estimate that says it is an estimate is honest; one that
reads like a measurement is not.

| Parameter | Value | Basis | Status |
|---|---|---|---|
| Container air capacitance | 2.8e6 J/K | ~6 t of enclosure steel and rack frames at the ~470 J/(kg K) of structural steel, plus a negligible 40 kJ/K of air | estimate |
| Rack cell capacitance | 2.33e6 J/K | a 418 kWh rack at ~180 Wh/kg cell level is ~2330 kg of cells, at the ~1000 J/(kg K) reported for LFP | estimate; the rack energy is fixed by the site descriptor, the energy density and the specific heat are not |
| Rack-to-air conductance | 900 W/K | sized for a ~9 K cell-to-air spread at the ~8 kW a rack dissipates at full site power | dependent estimate: that 8 kW follows from the M0 equivalent-circuit resistances, which `cell.rs` calls tuned rather than sourced. Refining them in M1 requires revisiting this number, or the 9 K spread silently becomes something else |
| Envelope conductance (UA) | 500 W/K | M0 placeholder | estimate |
| Sol-air coefficient | 0.026 m2 K/W | ASHRAE Handbook of Fundamentals, light-colored surface (0.052 for dark). The effective solar aperture is derived from it, `UA * alpha / h_o` = 13 m2, never stored separately | referenced |
| HVAC cooling capacity | 40 kW thermal | M0 placeholder | measured to be undersized. Once the cells became their own thermal node, a replayed July peak-dispatch day leaves a container at about 36 C air and 42 C cells against a 27 C setpoint. Capacity, not only staging, is part of the HVAC step |

Two observations that belong here rather than in a commit message:

- **Cell temperature is no longer a derived value.** It integrates, so the
  Modbus cell-temperature registers now lag and spread instead of tracking
  container air plus a constant. Same registers, different dynamics.
- **The thermostat cycles roughly ten times faster** than it did when the air
  node still carried the cells' mass: a cycle went from hours to minutes.
  That is the realistic direction, but there is still no minimum run time, so
  compressor cycles per day is an unvalidated model output until the staged
  HVAC step measures it.

## Planned gates (from ROADMAP.md)

- **M1:** annual RTE in the 80-85% field band (CAISO/EPRI fleet reports);
  realistic auxiliary share of throughput.
- **M2:** failure type/frequency distribution follows the EPRI incident
  taxonomy.
- **M3:** efficiency surfaces f(P, V_dc) match the Sandia/CEC database; the
  M1 RTE gate still holds.
- **M4:** simulated annual revenue inside public German fleet index bands.
- **M5:** capacity fade inside published LFP field bands.
