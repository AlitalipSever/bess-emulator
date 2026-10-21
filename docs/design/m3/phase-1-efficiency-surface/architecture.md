# M3 phase 1: efficiency surface, architecture

What exists after this phase: a converter whose conversion loss depends
on the DC voltage the racks present, and whose power limit is a current
limit multiplied by that voltage. The rack voltage already carries SoC,
current and hysteresis (M2.5), so the surface's second axis is a real
quantity in the tree, not a parameter.

## The model

`CurvePcs` keeps its three partial-load coefficients and gains the
Sandia model's voltage terms: the loss polynomial's coefficients become
linear functions of `(V_dc - V_dco)`, exactly as the CEC record
tabulates them (`C1`, `C2`, `C3` against `Pdco`, `Pso`, `C0`). The
block's DC voltage is the mean terminal voltage of its in-service racks
at the tick's start, which is what a converter's DC bus sees with racks
in parallel.

The DC-side limit stops being a power. `PowerLimits` keeps its shape
(the trait does not widen), but the BMS computes each rack's limit as
`I_max x V_terminal` rather than `I_max x V_nominal`, so the block's
capability falls toward the bottom of the SoC window and rises toward
the top, as a real converter's does.

## What it is not

- Not reactive capability, not the state machine, not setpoint
  dynamics: phases 2 and 3.
- Not a new trait: `PcsModel::finalize` keeps its signature; the DC
  voltage reaches the model through `PcsState`, which gains the field.

## Invariants

- Energy conservation closes exactly: the loss is still booked per tick
  from the same `p_dc` and `p_ac`.
- At `V_dc = V_dco` the surface reduces to the M0.5 curve, byte for byte
  (the switch-off proof, same pattern as every phase since M1).
- Capability is monotonic in SoC within the window.
