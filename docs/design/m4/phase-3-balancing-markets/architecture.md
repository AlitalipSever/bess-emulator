# M4 phase 3: balancing markets, architecture

What exists after this phase: products. The plant holds awarded blocks
of FCR (symmetric, four-hour products, six a day) and aFRR (positive and
negative, four-hour products), serves FCR from the phase 1 droop block
under the limited-energy-reservoir rules, serves aFRR from a request the
TSO sends (replayed from a fetched series, or synthetic and calibrated),
and meters what it earned: capacity payments per block, aFRR energy at
the published energy price, day-ahead energy at the replayed price, and
the imbalance settlement of its schedule deviation at the replayed
reBAP. The first EUR figures the emulator publishes.

## The model

- **Products.** `bess-models/src/market.rs` (new module directory
  `market/`): `Product { Fcr, AfrrPos, AfrrNeg }`, `Block { product,
  day, index 0..6, awarded_w, capacity_price_eur_mw }`, an `Awards`
  table per day. `EmsState::awards_today`: the blocks the plant holds.
- **FCR service.** The droop block armed with `prequalified_w` equal to
  the awarded power; `FcrSoc` manager (`market/fcr.rs`): the 15-minute
  criterion's working range (SoC between 0.25 h·P/E and 1 − 0.25 h·P/E,
  per the SOGL rule the Swissgrid annex states), the dead-band degree of
  freedom (charging or discharging inside ±10 mHz toward the range's
  centre), over-fulfilment up to 120 % when it helps, and scheduled
  transactions (a day-ahead energy block bought or sold to recentre,
  booked at the hour's price) when the free degrees are not enough.
- **aFRR service.** `Inputs::afrr_request_pu` (−1..1, the TSO's request
  as a fraction of the awarded power): a replayed series from the
  Netztransparenz per-second SRL-Sollwert the user fetched (compiled by
  `compile-afrr`), or `SyntheticAfrr`, a value-noise request with the
  published activation statistics; the plant follows it at the aFRR
  gradient (at least 0.5 % of rated per second, full activation within
  5 min) through the fast term, the activated energy metered per
  direction.
- **Prices.** `bess-data`: the 2024 reBAP year (SMARD 4998, CC BY,
  bundled, hash pinned); `BalancingPrices` loaded from a fetched
  regelleistung.net year (`fetch-balancing.sh`, `compile-balancing`) or
  the bundled annual figures per block and direction (`market/prices.rs`
  constants, cited).
- **The revenue meter.** `SubstationState::revenue` gains EUR: per
  closed quarter-hour the day-ahead energy value (metered export minus
  import at the hour's price), the imbalance settlement (schedule
  deviation at the reBAP), the capacity payment accrued (the block's
  price over its hours), the aFRR energy payment; running totals and
  the last period.

## What it is not

- Not a bidding model: awards are given (config or the phase 4 EMS),
  never cleared against other bidders.
- Not intraday trading.

## Invariants

- The droop's energy over a day equals the FCR energy row of the
  waterfall; the SoC never leaves the working range outside an alert
  state.
- aFRR activation never exceeds the awarded power; the fast term never
  pushes a block beyond rating plus the M3 overload.
- EUR totals equal the sum of the period figures; a period with no award
  and no deviation books zero.
