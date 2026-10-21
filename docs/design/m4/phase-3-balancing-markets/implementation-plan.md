# M4 phase 3: balancing markets, implementation plan

Three PRs, each landing green.

## PR1: products, FCR service, prices

- `market.rs` with products, blocks and awards; `market/fcr.rs` SoC
  manager; `market/prices.rs` annual figures; `fetch-balancing.sh`,
  `compile-balancing`, `--balancing`; scenario `site.market` kinds; the
  `fcr-block` case; checkpoint 19 (partial); DATA-LICENSES.md rows.

Accept: the working range holds and the alert state releases it; the
shape test still passes with the manager active.

*As built (PR #52):* as the design's as-built notes say. The
`fcr-block` case runs 20 MW on an idle summer night (taxonomy `market`,
new); the plant tests are in `bess-models/tests/balancing.rs`. The
annual record is unchanged: it is made with the plant that sells
nothing until phase 4's planner decides.

## PR2: aFRR service and the request

- `Inputs::afrr_request_pu`, `SyntheticAfrr`, `fetch-afrr.sh`,
  `compile-afrr`, `--afrr`; the fast-term follower at the aFRR gradient;
  activated energy metering; the `afrr-activation-day` case.

Accept: the ramp gate passes; activation never exceeds the award.

*As built (PR #53):* as the design's as-built notes say. The case
`afrr-activation-day` (taxonomy `market`) runs 20 MW each way through a
summer night; its two awards are a second apart, since the engine
refuses two events on one target at one offset.

## PR3: the revenue meter

- reBAP and balancing volumes compiled and bundled (`compile-rebap`,
  hash pinned); the four EUR rows, points, Prometheus counters; the
  bench's revenue table; checkpoint 19 complete; CALIBRATION.md section
  with every price figure and its download named.

Accept: the annual record publishes EUR for the first time, itemized;
the figures are reproducible from the bundled fallback without any
fetched file.

*As built (PR #54):* as the design's as-built notes say, with the reBAP
deferred (the id question, in DATA-LICENSES.md) and the imbalance at the
day-ahead price, labeled. The annual record's plant sells nothing, so
its EUR rows are the arbitrage's and the imbalance's; the gate is phase
4's.

## Open questions

- Whether a replayed aFRR series should drive the plant's share by the
  NRV's merit-order position (needs the bid) or pro rata (award over
  demand): leans pro rata, labeled.
- Whether the FCR scheduled transaction should be booked at the
  day-ahead price of the hour or at the intraday price the plant does not
  have: leans day-ahead, stated.
