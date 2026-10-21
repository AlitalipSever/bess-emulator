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
  two-hour duration points at the other two.
- **D2, prices are fetched, figures are cited.** The results are the
  TSOs' platform's; no licence statement was found, so the series is not
  bundled. `fetch-balancing.sh` downloads the 366 daily files for 2024
  from the data centre's download API (no login), `compile-balancing`
  turns them into a year artifact loaded by `--balancing <file>`. The
  bundled fallback is a table of annual figures (mean settlement price
  per FCR block, mean capacity price per aFRR block and direction, mean
  marginal energy price per direction), each a number cited in
  CALIBRATION.md with the download it came from, the way datasheet
  values are cited. A replayed run's record names the file's hash.
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
  public statement of it.
- **D5, the aFRR request is replayed or synthetic.** Netztransparenz
  publishes the NRV's per-second SRL-Sollwert and the 15-minute activated
  energy; both are the TSOs' copyright and the web API needs
  registration, so `fetch-afrr.sh` is for the user and nothing is
  bundled. The synthetic request is value noise with a 15-minute hold
  structure, its sign and magnitude statistics fitted to the bundled
  SMARD balancing volumes (CC BY), and its share of the plant's award
  labeled. The gradient is the prequalification's 0.5 %/s of rated
  power at least, the full activation time ENTSO-E's 5 min.
- **D6, the revenue meter in EUR.** Four rows per quarter-hour (day-ahead
  energy value, imbalance settlement, capacity accrual, aFRR energy),
  running totals, published as `site.revenue.period_eur_x100` (signed
  i32, cents) per row and in total, and as Prometheus counters; the
  bench's annual record gains a revenue table. The day-ahead value is
  metered energy at the replayed hourly price: the plant is taken as a
  price taker on its own schedule, no bid curve.
- **D7, checkpoint bump.** Format 19: awards, the droop's SoC manager,
  the aFRR request follower, the EUR accumulators.
- **D8, points.** Site inputs 70 to 79: `site.market.fcr_awarded_kw`,
  `site.market.afrr_pos_awarded_kw`, `site.market.afrr_neg_awarded_kw`,
  `site.market.afrr_request_pct`, `site.market.afrr_activated_kw`,
  `site.revenue.period_eur_x100` (four rows and total); a `fcr_alert_state`
  site bit (7, warning). Scenario files gain `site.market` with `award`
  kinds.

## Signal map impact

Ten site inputs, one site bit, scenario kinds. Map 0.6.0.

## Checkpoint impact

One format bump (D7).

## Test plan

- **Gate: the FCR working range** holds over a synthetic week; the alert
  state releases it and the plant delivers 15 minutes at full power from
  the range's edge.
- **Gate: the aFRR ramp** reaches full activation within 5 min at no
  less than 0.5 %/s on a step request.
- **Unit: the meter** books the four rows on a hand-built period; totals
  equal the sum.
- **Integration:** new library cases `fcr-block` (a four-hour FCR block
  on a synthetic frequency day; the droop's energy, the dead-band
  recentring) and `afrr-activation-day` (positive and negative awards, a
  synthetic request day); the annual record with awards from config,
  revenue rows published, the M1 gate still holding.
