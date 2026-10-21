# M4 phase 3: balancing markets, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the products are the German ones of 2024.** FCR as a symmetric
  four-hour product (six blocks a day, 00 to 04 through 20 to 24) with a
  settlement capacity price in EUR/MW per block; aFRR as positive and
  negative four-hour products with a capacity price in EUR/MW per block
  and an energy price in EUR/MWh per quarter-hour (the marginal price of
  the activated merit order). These are the columns regelleistung.net's
  result files carry (`GERMANY_SETTLEMENTCAPACITY_PRICE_[EUR/MW]`,
  `GERMANY_MARGINAL_ENERGY_PRICE_[EUR/MWh]`). mFRR is left out: the
  public fleet index reports it as the smallest stream and the plant's
  two-hour duration points at the other two. *As built (PR1):* `Product`
  and `Awards` in `kernel/market.rs`; the aFRR capacity price in the
  files is per MW and hour (the German pay-as-bid average), carried per
  block as four times it.
- **D2, prices are fetched, figures are cited.** The results are the
  TSOs' platform's; no licence statement was found, so the series is not
  bundled. `fetch-balancing.sh` downloads the 366 daily files for 2024
  from the data centre's download API (no login), `compile-balancing`
  turns them into a year artifact loaded by `--balancing <file>`. The
  bundled fallback is a table of annual figures (mean settlement price
  per FCR block, mean capacity price per aFRR block and direction, mean
  marginal energy price per direction), each a number cited in
  CALIBRATION.md with the download it came from, the way datasheet values are cited. A replayed run's record names the
  file's hash. *As built (PR1):* `fetch-balancing.sh`, `xlsx-to-csv.py`
  (the platform serves xlsx only; a standard-library converter keeps the
  data crate dependency-free), `compile-balancing` (second-tender rows
  without a price skipped, the cross-border price as the fallback for a
  German `-`), `BalancingYear` in `bess-data`, `--balancing` on the
  shell and the bench, `market::replay_balancing` once per process; the
  annual figures in `bess-models/src/market/prices.rs`.
- **D3, the imbalance price is bundled.** SMARD's reBAP (chart-data
  filter 4998, DE-LU, quarter-hourly, 2024) is CC BY 4.0 like the
  day-ahead year, compiled by the same compiler family
  (`compile-rebap`), hash pinned, recorded in DATA-LICENSES.md. The
  balancing volumes (4996, 4997) are compiled beside it and feed the
  synthetic aFRR request's sign and magnitude statistics.
- **D4, FCR under the SOGL rules for limited energy reservoirs**, as the
  Swissgrid annex states them (public, Continental Europe): continuous
  activation in the normal state, at least 15 minutes of full activation
  from the start of an alert state (|Δf| ≥ 50 mHz for 15 min, ≥ 100 mHz
  for 5 min, or ≥ 200 mHz), a working range of SoC derived from the
  usable energy and the prequalified power, charge management by
  schedule transactions or by other units of the balance group. The
  German degrees of freedom (dead-band use, over-fulfilment up to 120 %,
  scheduled transactions) are modelled; the TSO document that states
  them is not reproduced and the 120 % figure is labeled as the common
  public statement of it. *As built (PR1):* `ReservoirRules` and
  `Reservoir` in `kernel/market.rs`: the working range from the usable
  window and the prequalified power, the three alert criteria, a labeled
  release (a calm minute after the 15-minute minimum), the dead-band
  recentering proportional to the reservoir's lean, the over-fulfilment
  when the characteristic helps, and the transaction at the prequalified
  power on the plan while the reservoir sits outside the range. The
  awards arm the droop (`fcr_prequalified_w` is the FCR award) and take
  headroom from the plan in `Simulation::take_awards`.
- **D5, the aFRR request is replayed or synthetic.** Netztransparenz
  publishes the NRV's per-second SRL-Sollwert and the 15-minute activated
  energy; both are the TSOs' copyright and the web API needs
  registration, so `fetch-afrr.sh` is for the user and nothing is
  bundled. The synthetic request is value noise with a 15-minute hold
  structure, its sign and magnitude statistics fitted to the bundled
  SMARD balancing volumes (CC BY), and its share of the plant's award
  labeled. The gradient is the prequalification's 0.5 %/s of rated power
  at least, the full activation time ENTSO-E's 5 min. *As built (PR2):*
  `Inputs::afrr_request_pu` from the drivers (`SyntheticAfrr`: the
  frequency's one-minute average scaled to the full award at 100 mHz
  plus ±40 % value noise on 15-minute knots, labeled; or
  `AfrrRequestYear` the user compiled with `compile-afrr` from a
  `timestamp;mw` CSV and a stated demand, `fetch-afrr.sh` describing the
  registered download); `AfrrRules` (0.5 %/s of the award) and
  `Simulation::serve_afrr` (the request in its award, ramped, metered
  each way, through the fast term). The SMARD balancing volumes were
  not used: the ids SMARD serves them under could not be identified
  with confidence from public documentation, so the synthetic request
  is calibrated to nothing but the frequency and labeled.
- **D6, the revenue meter in EUR.** Four rows per quarter-hour (day-ahead
  energy value, imbalance settlement, capacity accrual, aFRR energy),
  running totals, published as `site.revenue.period_eur_x100` (signed
  i32, cents) per row and in total, and as Prometheus counters; the
  bench's annual record gains a revenue table. The day-ahead value is
  metered energy at the replayed hourly price: the plant is taken as a
  price taker on its own schedule, no bid curve.
- **D7, checkpoint bump.** Format 19: awards, the droop's SoC manager,
  the aFRR request follower, the EUR accumulators. *As built (PR1):*
  format 19 with `EmsState::awards`, `awards_pinned` and `reservoir`;
  PR2 and PR3 widen it.
- **D8, points.** Site inputs 70 to 79: `site.market.fcr_awarded_kw`,
  `site.market.afrr_pos_awarded_kw`, `site.market.afrr_neg_awarded_kw`,
  `site.market.afrr_request_pct`, `site.market.afrr_activated_kw`,
  `site.revenue.period_eur_x100` (four rows and total); a `fcr_alert_state`
  site bit (7, warning). Scenario files gain `site.market` with `award`
  kinds. *As built (PR1):* `site.market.fcr_awarded_kw` (72),
  `afrr_pos_awarded_kw` (74), `afrr_neg_awarded_kw` (76), the reservoir's
  `fcr_range_min_pct` (78) and `fcr_range_max_pct` (79); `fcr_alert_state`
  is bit 7; `award` takes `product` and `mw`, `award_clear` nothing; the
  request and the EUR points are PR2's and PR3's. *As built (PR2):*
  `site.market.afrr_request_pct` (80), `afrr_activated_kw` (81),
  `afrr_energy_pos_kwh` (83), `afrr_energy_neg_kwh` (85).

## Signal map impact

Ten site inputs, one site bit, scenario kinds. Map 0.6.0.

## Checkpoint impact

One format bump (D7).

## Test plan

- **Gate: the FCR working range** holds over a synthetic week; the alert
  state releases it and the plant delivers 15 minutes at full power from
  the range's edge.
- **Gate: the aFRR ramp** reaches full activation within 5 min at no
  less than 0.5 %/s on a step request. *As built (PR2):*
  `the_afrr_request_is_followed_at_the_gradient_inside_the_award`
  (full in 200 s at the gradient, clamped at the award, nothing in a
  direction without an award, through a checkpoint); the synthetic
  request's statistics in `grid_signal/afrr.rs`.
- **Unit: the meter** books the four rows on a hand-built period; totals
  equal the sum.
- **Integration:** new library cases `fcr-block` (a four-hour FCR block
  on a synthetic frequency day; the droop's energy, the dead-band
  recentring) and `afrr-activation-day` (positive and negative awards, a
  synthetic request day); the annual record with awards from config,
  revenue rows published, the M1 gate still holding.
