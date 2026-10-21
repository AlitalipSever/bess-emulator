# M3 phase 1: efficiency surface, implementation plan

One PR.

## PR1: the second axis

- `CurvePcs` gains `C1..C3` and `Vdco` from the CEC record; `PcsState`
  gains `dc_voltage_v`; the kernel computes it before the block's
  request; `finalize` converts at that voltage.
- `BasicBms::rack_limits` bounds by current at terminal voltage (D3).
- Point `blockNN.pcs.dc_voltage_v` (D5), instrumented, CSV regenerated.
- Tests per the design; golden and annual regenerated.

Accept: a block at the bottom of its window reports less capability
than one at the top, the efficiency at `Vdco` is the M0.5 number, and
the CEC gate passes at all three tabulated voltages.

*As built (feature/m3-phase1-pr1, 2026-10-21):* `SurfacePcs` in the GW-01
bundle; `dc_voltage_v` on `PcsState` (checkpoint 11); current-bounded
rack limits; `blockNN.pcs.dc_voltage_v` at 2000 + 10b + 4 (SCADA analog
instrument); map 0.5.0. The M2.5 switch-off digest proof was retired
(the bundle changed). Cycle RTE gate 0.9129 to 0.9131. Golden digest
regenerated; library records and the annual record regenerated.

## Open questions

- Whether the CEC record's three voltages (Vmin, Vnom, Vmax) span the
  GW-01 rack's window (416 x 2.90 to 3.45 V is 1,206 to 1,435 V; the
  SC2500UD-US record is tabulated around 900 V nominal). If not, the
  surface is used as the model's shape with its reference voltage set
  to the rack's nominal, stated as such; the gate then holds the shape,
  not the absolute voltage. *Resolved:* Vdco is 1,350 V and the MPPT
  window 1,300 to 1,500 V; the record's own voltage axis is used.
