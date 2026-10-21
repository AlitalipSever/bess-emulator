# M4 phase 4: the market EMS, architecture

What exists after this phase: an EMS that decides. `MarketEms`
implements `EmsStrategy` beside `DayAheadEms`: each local day it reads
the day's prices (day-ahead hourly, FCR per block, aFRR capacity per
block and direction, the aFRR energy price's recent mean), chooses per
block between keeping the power for arbitrage, offering it to FCR or
offering it to aFRR, writes the awards into the market state phase 3
built, and plans the remaining power as a state-of-charge-aware
day-ahead schedule whose charge hours precede its discharge hours. The
annual revenue the phase 3 meter books is then held against the public
fleet index.

## The model

- **The daily decision.** `ems/market.rs`: for each four-hour block, the
  value of each use: arbitrage (the block's best spread against the rest
  of the day, from the ranked plan), FCR (capacity price × offered MW),
  aFRR (capacity price × offered MW per direction plus the expected
  energy payment from the recent activation statistics). The highest
  wins the block; the plant may split (FCR takes a fraction of power,
  the rest arbitrages), bounded by the FCR working range's energy cost.
  Perfect foresight within the day, like the public index; no foresight
  across days.
- **The SoC-aware plan.** `DayPlan` gains an ordering: charge slots
  must precede the discharge slots they feed, and the plan reads the
  state of charge at the day's start, so a plant that ended full does
  not charge first; the M2 finding (65 days with discharge before
  charge, 28 exporting nothing) becomes a decision rather than an
  observation.
- **Headroom.** Awarded FCR and aFRR power is subtracted from the plan's
  power for the block's hours; the droop and the request get it.

## What it is not

- Not a price forecaster: the prices are the replayed year's.
- Not a bid: awards are the plant's own choice at the published price,
  never cleared against others. Stated in the gate.

## Invariants

- The planned power plus the awarded power never exceeds the rating in
  any block.
- The plan never asks for a discharge it cannot feed from the SoC it
  will have.
- With every market switched off the `MarketEms` plant is the
  `DayAheadEms` plant byte for byte, except for the ordering rule, which
  is itself switchable (switch-off proof).
