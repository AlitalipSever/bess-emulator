# v0.9.0: M4, the plant becomes a market participant

Draft of the release note, edited from the inventory in [README.md](README.md).
The maintainer creates the tag with it:
`gh release create v0.9.0 --title "v0.9.0: M4, the plant becomes a market participant" --notes-file docs/design/m4/release-notes.md`
(drop this preamble first).

---

Until v0.8.0 the grid was a sinusoid, the control surface two holding
registers, and the plant's income a quantity it metered and never
priced. v0.9.0 gives the grid a shape and the plant an answer to it,
gives a dispatch application the registers it writes and the figures it
is judged by, lets the plant hold German balancing products on the 2024
prices, books EUR, and ships a strategy that decides what to sell and
earns inside the band the public index gave.

## Highlights

- **The grid has a shape and the plant answers it.** The frequency at
  the POI is a synthetic Continental European series (20 mHz spread, the
  hour-boundary pattern, labeled against Rydin Gorjão et al. 2020) or a
  measured year replayed with `--frequency` from a file you compile
  from Energy-Charts (`fetch-frequency.sh`, `compile-frequency`; never
  bundled, the record names the hash). VDE-AR-N 4110's LFSM-O and
  LFSM-U cut and raise the target at 40 %/Hz from the tick after the
  excursion, a trip outside 47.5 to 51.5 Hz, and an FCR droop block on
  the SOGL rules (±10 mHz, ±200 mHz, 30 s) answers through a fast term
  the setpoint follower does not ramp. Reactive power in three modes
  (fixed cos φ at unity by default, fixed Q, Q(U)) on each converter's
  5 MVA circle, the whole rating available for vars at zero active
  power, the transformer consuming I²X: `site.poi.reactive_power_var`
  stops reading zero and `site.poi.power_factor` is new.
- **A dispatch application has a control surface and a report card.**
  The holding bank grows from three registers to twelve in SunSpec's
  immediate-controls shape: a percent limit with an enable and a revert
  time, obeyed in both directions at the follower's 10 %/min (inside
  the TSOs' 6 to 20 %/min band for third-party changes) and flagged as
  `external_limitation` while it binds; the reactive mode and its
  setpoints; a command register for the site reset, the switchgear and
  the limit's clear, whose refusals leave a `command_refused` event on
  the counter. REST gains `/api/v1/limit`, `/api/v1/reactive` and
  `/api/v1/substation/{switch}`. Per closed quarter-hour the plant
  reports its schedule deviation, availability each way, tracking RMS
  and redispatch potential (`site.kpi.*`), with `schedule_deviation`
  past a 5 % frame.
- **The plant holds balancing products and books EUR.** FCR and aFRR
  awards per four-hour block, from the strategy or pinned by a scenario;
  the FCR award arms the droop under the SOGL rules for a limited
  energy reservoir (the 15-minute working range, the alert state, the
  German degrees of freedom); the aFRR award follows the TSO's request
  (a synthetic one, or `--afrr` from a file you compile) at 0.5 % of the
  award a second. Prices from regelleistung.net's 2024 results, fetched
  and compiled by you (`fetch-balancing.sh`, `xlsx-to-csv.py`,
  `compile-balancing`, `--balancing`), the annual figures bundled and
  cited. The fiscal meter books four EUR rows per quarter-hour
  (`site.revenue.period_*_eur_x100`, `site.revenue.total_eur`,
  Prometheus counters): the first EUR the emulator publishes.
- **A strategy that decides.** `MarketEms` (the bench's `--market`
  year, `gw01_market_models`) chooses per block between arbitrage, FCR
  and aFRR on the day's prices and plans the rest with charges before
  discharges; the market year earned 127.4 kEUR/MW, inside the
  97.5 to 195 band derived from the ISEA/enspired Battery Revenue Index
  and written before the number. The shell's default stays the
  day-ahead plan.

## What an integrator can trip over

- Signal map 0.6.0: additions only, at site inputs 59 to 99 and the
  second per-block range's +8, and nine new holding registers (3 to 11);
  COMPATIBILITY.md has the row. Site word bits 4 to 7 and 10 are new.
- Checkpoint format 19: v0.8.0's format 16 files are refused by version.
- `site.poi.frequency_hz` is a different series; baselines recorded
  against v0.8.0 differ at every tick. `site.poi.reactive_power_var`
  reads about −12 Mvar at rating at unity (the transformer).
- `Inputs` gained `grid_voltage_pu`'s companion `afrr_request_pu`; code
  that builds `Inputs` by hand gains a field.
- Scenario files gain `site.grid` (`frequency_offset`), `site.ems`
  (`limit`, `limit_clear`) and `site.market` (`award`, `award_clear`),
  the `grid` and `market` taxonomies, and four new cases
  (`frequency-event`, `curtailment-order`, `fcr-block`,
  `afrr-activation-day`): twenty-one cases, every record regenerated.
- The annual record gains `control` and `revenue` sections; a second
  record, `calibration/m4-market.json`, and a second CI step hold the
  market year. The bench's `--write` and `--check` keep their meaning;
  `--market` runs the other plant.
- Four data sources are fetched, never bundled (DATA-LICENSES.md):
  Energy-Charts' frequency, regelleistung.net's results, Netztransparenz's
  aFRR setpoint; SMARD's reBAP is deferred over an id question and the
  imbalance row is priced at the day-ahead price, labeled.

## Calibration

Eight M4 sections in CALIBRATION.md: the frequency's statistics and the
grid-code responses (the SOGL shape test, the 4110 gradient), reactive
power (the circle, the 4110 range, the labeled reactance), the control
surface (SunSpec's shape, the TSOs' gradient band), the quarter-hour
report (the TSOs' two figures, the labeled frame), the products and
their 2024 prices (what the files taught), the aFRR service (the
gradient, the pro-rata share), the EUR meter (the floor, the stand-in),
and the market participant's year (the band before the number, 127.4
kEUR/MW inside it). The known gaps are written beside each; the largest
is that balancing energy counts as schedule deviation in the report and
is charged as imbalance, which the gate holds with.
