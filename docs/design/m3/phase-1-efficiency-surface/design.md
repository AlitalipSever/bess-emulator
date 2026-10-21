# M3 phase 1: efficiency surface, design

Each decision is proposed here and confirmed or revised in its PR.
As built in PR1 (2026-10-21): D1, D2, D3, D5 confirmed; D4 revised.

## Decisions

- **D1, the Sandia voltage terms, not a second fit.** The CEC record
  already carries `C1`, `C2`, `C3` and `Vdco`; using them keeps the fit
  family the database was built with and gives the gate a source at
  three voltages. A free two-dimensional fit would be a claim. *As built:* `pcs/sandia.rs`
  carries the record per unit of its rating (`CEC_SC2500UD_US`) and
  `SurfacePcs` converts on it; the M0.5 `CurvePcs` stays as the fitted
  single-voltage model. The record's Vdco is 1,350 V with an MPPT window
  of 1,300 to 1,500 V, inside GW-01's rack window (the plan's open
  question closes: no reference-voltage shift needed).
- **D2, the DC voltage is the racks' mean terminal voltage.** Not a
  separate bus model: with racks in parallel on one DC bus the
  converter sees their terminal voltage, and modeling the bus would
  invent a quantity the tree does not need. Computed at the tick's
  start, before the racks step, so the converter's request is made at the voltage it will convert at.
  *As built:* `dc_bus_voltage_v` in the kernel's block step, nominal
  when no rack is in service; `PcsModel::ac_capability_w` and
  `dc_request_w` gained the voltage as a parameter (the trait widened by
  one argument, since the request is made before `finalize` sees the
  state).
- **D3, current-bounded rack limits.** `max_current_a x voltage_v`
  replaces `max_current_a x nominal_v()` in the BMS limit; the taper
  and the temperature factor multiply as before. The 1C rating stays
  (the M2 known gap about the cell's 0.5P table stands). *As built* as
  proposed.
- **D4, no checkpoint bump.** The DC voltage on `PcsState` is derived
  every tick from the racks; storing it would be a cache. *Revised:* it is kept on `PcsState`, so a
  checkpoint says what the converter converted at and the surfaces read
  it without a second computation; the field is derived every tick
  before it is read, so nothing stale survives a resume. Serializing it
  changes the tree, so the format moved to 11 after all.
- **D5, one new point.** `blockNN.pcs.dc_voltage_v` at an unused
  address of the second per-block range (2000 + 10b + 4), class
  `fast`, SCADA analog instrument (class 0.5). Minor, map stays 0.4.0
  until phase 2 breaks it. *As built:* map 0.5.0 opened with this
  addition, since 0.4.0 shipped with v0.7.0; phase 2 adds its major
  step to the same row.

## Signal map impact

One addition (D5).

## Checkpoint impact

None (D4).

## Test plan

- **Gate: the surface against the CEC record** at its three DC voltages
  and six load points, within 0.3 points (`pcs.rs` test, extending the
  M0.5 one).
- **Unit: reduction at `Vdco`** equals the M0.5 curve to the ULP. *Revised:* the surface and the M0.5
  polynomial are different fit families, so ULP equality was never
  available; the gate holds the surface to the M0.5 reference points at
  Vdco within the M0.5 tolerance, and the surface's own inverse round
  trips to 1e-9 at every voltage.
- **Property: monotonic capability in SoC** within the window.
- **Golden snapshot:** regenerated (physics: the surface moves losses by
  tenths of a percent at the window's ends), called out.
- **Annual run:** regenerated; the round trip moves marginally and the
  waterfall's PCS row is explained in the PR.
