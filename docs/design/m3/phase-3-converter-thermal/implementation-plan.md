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

## Open questions

- Whether the overload interacts with the BMS limit chain (a rack at 1C
  cannot supply 110 % of the converter's rating): the budget spends
  only when the AC side exceeds rating, which the DC side may never
  allow at GW-01's 1C racks; the case may need the external setpoint
  above rating. Stated in the case's header either way.
