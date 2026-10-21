# M3 phase 3: converter thermal and the overload budget, implementation plan

One PR.

## PR1: the warm converter

- Thermal node and derating factor in `CurvePcs` (behind `PcsModel`),
  the budget in `PcsState`, the two points, the block alarm bits,
  checkpoint bump.
- A new library case, `pcs-overload-and-derate`, and the hot-week
  cases' headers checked against the new behavior.

Accept: at 110 % for ten minutes the budget reaches zero and the
converter falls back to rating; on the replayed year's warmest day the
converter's temperature is published and no converter derates (the
annual record holds), unless the datasheet curve says otherwise, in
which case the record moves and the PR explains it.

*As built (PR #TBD):* as the design's as-built notes say; the case is
named `pcs-overload-budget`, since the budget, not the derate, is what
the replayed year can show. The open question below resolved as
expected: GW-01's racks (1C) allow 110 % of the converter's rating on
the DC side, so no external setpoint above rating was needed; the lost
blocks' share lands on the rest.

## Open questions

- Whether the overload interacts with the BMS limit chain (a rack at 1C
  cannot supply 110 % of the converter's rating): the budget spends
  only when the AC side exceeds rating, which the DC side may never
  allow at GW-01's 1C racks; the case may need the external setpoint
  above rating. Stated in the case's header either way.
