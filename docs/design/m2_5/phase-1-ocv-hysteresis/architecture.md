# M2.5 phase 1: OCV hysteresis, architecture

What exists after this phase: an LFP cell whose rest voltage depends on
where it has been, not only where it is. The charge and discharge OCV
curves separate, the flat plateau between them becomes the ambiguity it
is in real cells, and voltage stops being a cheap oracle for SoC, which
is the whole setup for phase 2.

## The model

Plett's one-state hysteresis model, the standard equivalent-circuit
extension: one additional state `h` per rack, driven toward a bounded
envelope by throughput,

```
dh/dt = -|i| * gamma / Q * (h - h_max(soc) * sign(i))
```

with the effective open-circuit voltage `OCV(soc) + h`. The envelope
`h_max(soc)` is largest mid-plateau and small at the knees, matching the
published shape of LFP charge/discharge curve pairs. At rest `h` holds,
which is the physically right behavior for LFP (the separation persists
for hours) and what makes rested voltage ambiguous.

`CellModel` is the one module that deepens. The trait does not widen:
`CellModel::step_rack` already owns rack integration and `RackState`
gains the `h` field; `stored_energy_wh` keeps its meaning as the OCV-curve integral,
with the hysteresis contribution accounted so the energy-conservation
invariant stays exact rather than gaining a tolerance.

## What it is not

- Not a second RC branch, not temperature-dependent parameters: those
  stay on the `CellModel` upgrade path where the layer table has always
  listed them.
- Not per-cell: `h` is rack-granular like every other cell quantity, per
  the interface rule.
- NMC-style minor-loop modeling is out: GW-01 is LFP, one chemistry, one
  envelope.

## Invariants

- `|h| <= h_max(soc)` always; `h` moves only under current; sign chases
  the current's sign.
- Energy conservation closes exactly over arbitrary charge/discharge
  cycles including hysteresis work.
- With `gamma = 0` the plant is byte-identical to pre-phase behavior
  (the switch-off proof, same pattern as FlatPcs beside CurvePcs).
