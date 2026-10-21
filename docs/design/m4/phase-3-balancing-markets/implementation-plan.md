# M4 phase 3: balancing markets, implementation plan

Three PRs, each landing green.

## PR1: products, FCR service, prices

- `market.rs` with products, blocks and awards; `market/fcr.rs` SoC
  manager; `market/prices.rs` annual figures; `fetch-balancing.sh`,
  `compile-balancing`, `--balancing`; scenario `site.market` kinds; the
  `fcr-block` case; checkpoint 19 (partial); DATA-LICENSES.md rows.

Accept: the working range holds and the alert state releases it; the
shape test still passes with the manager active.

## PR2: aFRR service and the request

- `Inputs::afrr_request_pu`, `SyntheticAfrr`, `fetch-afrr.sh`,
  `compile-afrr`, `--afrr`; the fast-term follower at the aFRR gradient;
  activated energy metering; the `afrr-activation-day` case.

Accept: the ramp gate passes; activation never exceeds the award.

## PR3: the revenue meter

- reBAP and balancing volumes compiled and bundled (`compile-rebap`,
  hash pinned); the four EUR rows, points, Prometheus counters; the
  bench's revenue table; checkpoint 19 complete; CALIBRATION.md section
  with every price figure and its download named.

Accept: the annual record publishes EUR for the first time, itemized;
the figures are reproducible from the bundled fallback without any
fetched file.

## Open questions

- Whether a replayed aFRR series should drive the plant's share by the
  NRV's merit-order position (needs the bid) or pro rata (award over
  demand): leans pro rata, labeled.
- Whether the FCR scheduled transaction should be booked at the
  day-ahead price of the hour or at the intraday price the plant does not
  have: leans day-ahead, stated.
