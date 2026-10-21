# M4 phase 4: the market EMS, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the index is the band.** The ISEA (RWTH Aachen) and enspired
  Battery Revenue Index is the public reference for German large-scale
  storage revenue: a one- or two-hour system, up to two full cycles a
  day, 90 % AC/AC efficiency, perfect daily foresight, 80 % of power
  offered to FCR at the German local marginal price, aFRR capacity at the
  German/Austrian average price, trading on EPEX ID1, no aFRR energy
  revenue; a 365-day rolling figure updated daily. Its published
  twelve-month figures to October 2024 for a two-hour system: about
  195 kEUR/MW cross-market, 125 aFRR-only, 145 trading-only; its
  monthly annualized values through April to December 2024 ran from
  about 220 (June) down to about 100 kEUR/MW/year at the year's end
  (as reported in the public press summaries, the index's own site
  being unreachable from here on 2026-10-21; the figures are recorded
  with that provenance and replaced by the site's when it is reached).
  The band: the GW-01 annual total per MW between 50 % and 100 % of the
  cross-market figure, the lower bound for the intraday revenue the
  emulator does not model and the perfect-foresight assumption it only
  partly shares; the mix (balancing share of the total) between the
  shares the two single-market figures imply. The band is written in
  CALIBRATION.md before the first number.
- **D2, the ordering rule is the M2 finding made a decision.** Charge
  slots precede the discharge slots they feed; the plan reads the
  day-start SoC. The three-charge/two-discharge rule is kept (the 2 h
  plant needs the extra hour for losses), so the clean-year record
  changes only in ordering; both plans are kept and the switch is a
  config value, so the M2 annual record can still be reproduced.
- **D3, the daily block decision is value-ranked, not optimized.** A
  ranking the way the index describes its own method ("simple enough to
  be calculable in Excel"): per block, the three values compared and
  the best taken, with FCR's share bounded by the working range. An
  optimizer would be a second project; the ranking is stated as the
  method in CALIBRATION.md.
- **D4, checkpoint bump.** Format 20: the planner's day decision and
  the plan's ordering state.
- **D5, points.** `site.ems.mode` and `control.ems_mode` gain value 2
  (`market`); `site.market.block_use` (80: 0 arbitrage, 1 fcr, 2 afrr,
  3 split) for the current block.

## Signal map impact

One site input, one enum value on two existing points (addition). Map
0.6.0 at the release.

## Checkpoint impact

One format bump (D4).

## Test plan

- **Unit: the ordering** on hand-built days (the M2 counter-example day:
  morning peak before the midday trough; the plant ending full).
- **Unit: the block decision** picks FCR when the capacity price beats
  the spread, aFRR when its price and expected energy beat both.
- **Gate: the annual revenue** inside the band, itemized by row and by
  product, with the index's figures and assumptions beside it.
- **Release:** v0.9.0 (version bump, records regenerated, ROADMAP M4
  done, release notes from the inventory, COMPATIBILITY map 0.6.0 row,
  CALIBRATION the M4 gates and the annual record at release).
