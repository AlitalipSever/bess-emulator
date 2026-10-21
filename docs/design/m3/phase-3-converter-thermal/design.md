# M3 phase 3: converter thermal and the overload budget, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, a lumped node with a published time constant.** Sized so the
  converter reaches its steady-state temperature rise at rated power in
  the time a datasheet thermal curve shows; labeled estimate where only
  the steady state is public. *As built:* `ConverterThermal` in
  `bess-models/src/pcs/thermal.rs`, owned by `SurfacePcs`; the node
  settles at the air plus a rise proportional to the loss (20 K at rated
  loss, 15 min, both labeled estimates), cooled toward the ambient since
  the converter sits outdoors between the containers. The trait gained
  `step_thermal`, called before the capability; `ac_capability_w` and
  `dc_request_w` take the `PcsState` and read the permitted power from
  it.
- **D2, the derating curve from the converter datasheet.** Utility
  converter datasheets publish permitted output against ambient (full
  power to a stated temperature, linear to zero beyond); taken point
  for point, as the M2 cell table was. *As built:* the Sungrow SG2500U
  datasheet (the unit behind the CEC record) is public on the knee only:
  full power to 50 °C ambient, derating above, operating to 60 °C. The
  knee is shifted onto the node by the rated rise (70 °C); the slope to
  zero (20 K past the knee) is a labeled estimate. The replayed year
  never reaches it.
- **D3, the overload budget from the datasheet's overload table,** or a
  labeled estimate (110 % for 10 minutes, the common figure) if the
  pinned record has none; the README's open question. *As built:* the
  110 % is the datasheet's (2750 kVA at 45 °C against 2500 kVA at
  50 °C); the ten minutes and the thirty-minute refill are labeled
  estimates. The budget is a reservoir in seconds at the permitted
  overload, spending in proportion to the excess, with an arming latch:
  a spent budget disarms the overload and only a full one re-arms it,
  so the limit does not flicker at the edge of an empty budget. The
  plant controller leans on it without knowing: its share is the target
  over the dispatchable blocks, so a lost block's share lands on the
  others at 105 % and a staggered return runs the early blocks at
  110 %. Advertised capability stays bounded by the rating.
- **D4, checkpoint bump.** Node temperature and budget per block. *As
  built:* format 14 with `temperature_c`, `overload_budget`,
  `overload_armed` and `ac_limit_w`.
- **D5, points.** `blockNN.pcs.temperature_c` (thermometer instrument)
  and `blockNN.pcs.overload_budget_pct` at 2000 + 10b + 6 and + 7; a
  block alarm bit `pcs_over_temperature` (warning) with a derate bit
  reuse (`racks_derated` stays the rack count; a new
  `pcs_derated` bit in the block word). *As built:* as proposed, bits 4
  and 5 of the block word; the temperature passes a Pt100 class B
  thermometer, the budget is exact. The warning band sits 5 K under the
  knee (65 °C raise, 62 clear); the derate bit mirrors the curve, not
  the budget. A block at its rating asked for more reports no setpoint
  miss (it delivers what it follows); the site reports the limit.

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
  before cells do, with the header saying so. *As built:* the replayed
  year's warmest evening leaves the node 15 K under the knee, so no
  public-weather case can show a derate; the library case is
  `pcs-overload-budget` instead (two converters trip, the eighteen left
  run at 110 % for ten minutes to the second, then the site reports its
  limit), and the derate is covered by `converter_thermal.rs` with a
  node set hot by hand. `pcs-trip-and-reset` changed its story: the
  nineteen carry the lost block for twenty minutes before the limit.
- **Switch-off proof** and golden regeneration.
