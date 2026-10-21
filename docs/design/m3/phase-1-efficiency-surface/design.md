# M3 phase 1: efficiency surface, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the Sandia voltage terms, not a second fit.** The CEC record
  already carries `C1`, `C2`, `C3` and `Vdco`; using them keeps the fit
  family the database was built with and gives the gate a source at
  three voltages. A free two-dimensional fit would be a claim.
- **D2, the DC voltage is the racks' mean terminal voltage.** Not a
  separate bus model: with racks in parallel on one DC bus the
  converter sees their terminal voltage, and modeling the bus would
  invent a quantity the tree does not need. Computed at the tick's
  start, before the racks step, so the converter's request is made at
  the voltage it will convert at.
- **D3, current-bounded rack limits.** `max_current_a x voltage_v`
  replaces `max_current_a x nominal_v()` in the BMS limit; the taper
  and the temperature factor multiply as before. The 1C rating stays
  (the M2 known gap about the cell's 0.5P table stands).
- **D4, no checkpoint bump.** The DC voltage on `PcsState` is derived
  every tick from the racks; storing it would be a cache.
- **D5, one new point.** `blockNN.pcs.dc_voltage_v` at an unused
  address of the second per-block range (2000 + 10b + 4), class
  `fast`, SCADA analog instrument (class 0.5). Minor, map stays 0.4.0
  until phase 2 breaks it.

## Signal map impact

One addition (D5).

## Checkpoint impact

None (D4).

## Test plan

- **Gate: the surface against the CEC record** at its three DC voltages
  and six load points, within 0.3 points (`pcs.rs` test, extending the
  M0.5 one).
- **Unit: reduction at `Vdco`** equals the M0.5 curve to the ULP.
- **Property: monotonic capability in SoC** within the window.
- **Golden snapshot:** regenerated (physics: the surface moves losses by
  tenths of a percent at the window's ends), called out.
- **Annual run:** regenerated; the round trip moves marginally and the
  waterfall's PCS row is explained in the PR.
