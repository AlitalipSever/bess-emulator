# M3 phase 3: converter thermal and the overload budget, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, a lumped node with a published time constant.** Sized so the
  converter reaches its steady-state temperature rise at rated power in
  the time a datasheet thermal curve shows; labeled estimate where only
  the steady state is public.
- **D2, the derating curve from the converter datasheet.** Utility
  converter datasheets publish permitted output against ambient (full
  power to a stated temperature, linear to zero beyond); taken point
  for point, as the M2 cell table was.
- **D3, the overload budget from the datasheet's overload table,** or a
  labeled estimate (110 % for 10 minutes, the common figure) if the
  pinned record has none; the README's open question.
- **D4, checkpoint bump.** Node temperature and budget per block.
- **D5, points.** `blockNN.pcs.temperature_c` (thermometer instrument)
  and `blockNN.pcs.overload_budget_pct` at 2000 + 10b + 6 and + 7; a
  block alarm bit `pcs_over_temperature` (warning) with a derate bit
  reuse (`racks_derated` stays the rack count; a new
  `pcs_derated` bit in the block word).

## Signal map impact

Two additions, one new block alarm bit (map 0.5.0 absorbs it, unreleased).

## Checkpoint impact

One format bump (D4).

## Test plan

- **Property: thermal balance** closes.
- **Unit: the curve** reproduces the datasheet points.
- **Unit: the budget** spends and refills at the published rates and
  caps.
- **Integration:** a hot-week library case where converters derate
  before cells do, with the header saying so.
- **Switch-off proof** and golden regeneration.
