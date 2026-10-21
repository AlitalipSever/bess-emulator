# M4: EMS and market signals

Status: designed 2026-10-21 after M3 shipped as v0.8.0; phase 1 landed
the same day (PRs #48 and #49), phase 2 after it (PRs #50 and #51), phase 3 after that (PRs #52, #53 and #54), phase 4 PR1 after that (PR #55). ROADMAP.md defines the goal and the calibration gate; this folder
fixes the architecture, the decisions and the work breakdown, one phase
per subfolder, each phase's documents to be marked with what was actually
built. Layout per [docs/design/README.md](../README.md).

## Goal

The plant behaves like a market participant, and external control
becomes fully testable. After M4 a consumer polling the plant sees a POI
whose frequency and voltage are a replayed or statistically faithful
grid, a converter fleet that answers that frequency the way the
connection codes demand (LFSM over and under frequency, FCR droop with a
dead band and a thirty-second activation), reactive power that follows a
mode an operator chose (fixed cos φ, fixed Q, Q(U)) including at zero
active power, a control surface over Modbus and REST that carries
everything a dispatch application writes (setpoint, power limit, reactive
mode, mode of operation, switchgear) and obeys it with the gradient the
grid asks of third-party changes, telemetry that reports how well the
plant tracked what it was told and how much of it was available, and an
EMS that sells the plant into the day-ahead market and the German
balancing markets (FCR, aFRR) with real activation shapes and real
prices, metering revenue in EUR for the first time.

## What M4 changes

Following the M2 and M3 pattern: the module that deepens is the EMS,
behind `EmsStrategy`, with two neighbours widened because the market
behaviour has nowhere to land without them: the grid interface
(`GridInterface`, reactive power and the grid-code responses at the POI)
and the control surface in the shell (holding registers, REST commands).

1. **The grid becomes an input with a shape.** Frequency and voltage
   stop being sinusoids: a statistically calibrated synthetic frequency
   with the Continental European hour-boundary pattern, bundled, and a
   replayed measured year the user fetches (Energy-Charts, under their
   own acceptance of its terms), never bundled.
2. **Grid-code behaviours at the POI.** LFSM-O and LFSM-U per
   VDE-AR-N 4110/4120, an FCR droop block per the SOGL prequalification
   rules, and reactive power control in three modes with the converter's
   apparent-power circle and the transformer's reactive consumption.
3. **External control, obeyed and flagged.** A written power limit
   (curtailment, redispatch) is followed at the third-party gradient and
   reported as an external limitation; every holding register exercised
   end to end over Modbus; switchgear commands on the shell (the M3 gap).
4. **Tracking quality and availability reporting.** Per quarter-hour,
   the plant reports how far its metered mean sat from its schedule and
   how much of its contracted power was available, the two figures the
   TSOs' 2026 operational requirements for EHV-connected batteries ask
   for.
5. **Balancing-market activation replay.** FCR and aFRR as products with
   awarded blocks; FCR served from the droop block with the limited-
   energy-reservoir SoC management; aFRR served from a replayed TSO
   request (fetched) or a calibrated synthetic one (bundled); prices from
   regelleistung.net's published results (fetched) with annual figures
   cited; the imbalance price (SMARD reBAP 2024, CC BY, bundled) settling
   the tracking error.
6. **A market-participant EMS.** A SoC-aware day-ahead plan (charge before
   discharge, the M2 finding), balancing blocks reserved as headroom, a
   daily choice between the products on their prices, and a revenue
   meter in EUR whose annual mix and magnitude are held against a public
   German fleet index.

## Phases

| Phase | Theme | Depends on |
|---|---|---|
| [1: Grid code at the POI](phase-1-grid-code/) | frequency input with a shape, LFSM-O/U, FCR droop block, reactive power modes | nothing |
| [2: External control](phase-2-external-control/) | power limit and reactive registers, external limitation flag, tracking and availability KPIs, shell switchgear commands, Modbus end to end | 1 (the reactive registers address phase 1's modes) |
| [3: Balancing markets](phase-3-balancing-markets/) | FCR and aFRR products, activation replay, LER SoC management, price data, EUR revenue meter, reBAP settlement | 1 (the droop block), 2 (the KPIs the products are judged by) |
| [4: Market EMS](phase-4-market-ems/) | SoC-aware plan, product stacking, revenue gate, release v0.9.0 | 3 |

Phases land in order. A phase is done when its PRs are merged, its tests
are green in CI, and its documents record what was actually built.

## Calibration gate

From ROADMAP.md: simulated annual revenue mix and magnitude land inside
public German fleet index bands. Per phase, against public data,
recorded in CALIBRATION.md:

1. The synthetic frequency's distribution and hour-boundary pattern
   match the published Continental European statistics (Rydin Gorjão et
   al., Nature Communications 2020, CC BY 4.0): near-Gaussian with a
   stated standard deviation, the fifteen-minute dispatch pattern
   visible, the autocorrelation decaying as the paper describes; the
   LFSM and FCR responses reproduce the standards' test signals
   (VDE-AR-N 4110's 40 %/Hz from 50.2 Hz; the SOGL ramp: at least 50 % of
   the prequalified power at 15 s and 100 % at 30 s for a ±200 mHz step).
2. The external gradient sits inside the TSOs' published 6 to 20 %/min
   band for third-party power changes; a curtailment order is met within
   the time that gradient implies.
3. The FCR provision passes the Swissgrid/SOGL prequalification shape
   test and the 15-minute energy criterion's working range; the aFRR
   provision ramps at no less than 0.5 % of rated power per second and
   reaches full activation within 5 min (ENTSO-E FAT); prices used for
   the annual record are the published 2024 results, cited as annual
   figures.
4. The annual revenue: total inside a band derived from the ISEA/enspired
   Battery Revenue Index for a two-hour system (cross-market about
   195 kEUR/MW/year over the twelve months to October 2024, with the
   index's own assumptions named: two cycles a day, 90 % AC/AC, perfect
   daily foresight, 80 % of power offered to FCR), and the mix (balancing
   against trading) between the index's two single-market figures
   (aFRR-only 125, trading-only 145 kEUR/MW/year). The band and its
   derivation are written in CALIBRATION.md before the number is.
5. The M1 round-trip gate still holds with the frequency responses and
   the reactive losses in place.

## Versioning

- **Crate:** v0.9.0 at milestone end.
- **Signal map:** additions in every phase (reactive power per block and
  at the POI, grid-code bits, power-limit and reactive holding registers,
  KPI points, product points, EUR revenue points), at unused addresses;
  `control.ems_mode` gains values (minor). No address moves. Map 0.6.0.
  COMPATIBILITY.md promised since M0 that the holding bank grows in M4.
- **Checkpoint:** bumps in phases 1 (reactive and grid-code state, the
  droop block's state), 2 (limits, KPI accumulators), 3 (product state,
  activation and revenue accumulators) and 4 (the planner's state).

## Data and licences

Recorded in DATA-LICENSES.md before each lands:

| Series | Source | Terms | M4 handling |
|---|---|---|---|
| Grid frequency, 1 s, 2022 onward | Energy-Charts / Fraunhofer ISE API `v2/frequency` (measured in Freiburg, RG Continental Europe, 3 days per request) | free for non-commercial and journalistic use with attribution; not MIT/Apache compatible | fetch script and `--frequency` replay, never bundled |
| Grid frequency, 1 s, 2012 to 2022 | Netztransparenz archive; TransnetBW via Zenodo (Kruse, Schäfer, Witthaut) | Netztransparenz: copyright, reproduction needs the TSOs' written consent; Zenodo: TransnetBW data republished with permission, not under an open licence | not bundled; named as the statistics' provenance only |
| FCR and aFRR tender results, 2024 | regelleistung.net data centre (`cpp-publisher` download API, daily files, no login) | no licence statement found; the TSOs' platform | fetch script; annual figures per block and direction cited as numbers in CALIBRATION.md and code constants |
| aFRR per-second request (SRL-Sollwert) and 15-min activated energy | Netztransparenz | copyright as above; the web API needs registration | fetch script for the user; bundled synthetic activation calibrated to cited aggregates |
| Imbalance price (reBAP), quarter-hourly, 2024 | Bundesnetzagentur / SMARD, chart-data filter 4998 (balancing volumes 4996/4997 beside it) | CC BY 4.0 | bundled, hash pinned, like the day-ahead year |

SMARD removed its balancing-reserve series (PRL, SRL, MRL prices and
volumes) on 2025-05-22, which is why the reserve prices come from
regelleistung.net and not from the CC BY source the day-ahead year came
from.

## Release-note inventory for v0.9.0

Filled as PRs land.

- **The grid has a shape** (phase 1 PR1). `site.poi.frequency_hz`
  carries a synthetic Continental European series (20 mHz spread, the
  hour-boundary pattern, labeled) instead of a 10 mHz sinusoid, or a
  measured year replayed with `--frequency` from a file the user
  compiles from Energy-Charts (`fetch-frequency.sh`, `compile-frequency`;
  never bundled, the record names the file's hash).
- **The plant answers the frequency** (phase 1 PR1). LFSM-O above 50.2 Hz
  and LFSM-U below 49.8 Hz at 40 %/Hz of rated power, from the tick
  after the excursion, through a fast term the setpoint follower does
  not ramp; a trip outside 47.5 to 51.5 Hz (`grid_frequency_trip`, site
  bit 10, latched); `lfsm_active` (site bit 4). An FCR droop block
  (±10 mHz dead band, full at ±200 mHz, 30 s activation) that phase 3's
  products will arm; `site.grid_code.state` (60) and `site.fcr.output_kw`
  (61). Scenario files gain `site.grid` with `frequency_offset`; a new
  `grid` taxonomy and case `frequency-event`. Map 0.6.0 (additions).
- **Checkpoint format 17** (phase 1 PR1): `EmsState::fcr_prequalified_w`,
  `fcr_output_w`, `lfsm_cut_w`; `PcsState::fast_term_w`;
  `SubstationState::frequency_offset_mhz`.
- **The plant has reactive power** (phase 1 PR2). Three modes the
  operator chooses (fixed cos φ, the default at unity; fixed Q; Q(U)
  with a ±2 % dead band reaching cos φ 0.9's range at ±5 %), a 10 s
  settling, each converter on its 5 MVA circle so the whole rating is
  there for vars at zero active power, the transformer consuming I²X on
  a 12 % reactance (labeled). `site.poi.reactive_power_var` reads the
  result (about −12 Mvar at rating at unity), `site.poi.power_factor` (59)
  and `blockNN.pcs.q_kvar` (2000 + 10b + 8, analog class 0.5) are new.
  Checkpoint 17 widened: the mode, the settled target, each converter's
  vars, the power factor.
- **A written limit is obeyed and flagged** (phase 2 PR1). The holding
  bank grows from three registers to twelve in SunSpec's
  immediate-controls shape: a percent limit with an enable and a revert
  time, the reactive mode with its setpoints, and a command register
  (site reset, the switchgear, clear limit). A limit caps the site
  target in both directions from the next tick, the plant walks to it
  at the follower's 10 %/min (inside the TSOs' 6 to 20 %/min band for
  third-party changes), `external_limitation` (site bit 5) says the
  shortfall is the order's while `power_limited` stays clear. A refused
  command leaves a `command_refused` event on the counter. REST gains
  `/api/v1/limit`, `/api/v1/reactive` and
  `/api/v1/substation/{switch}`; scenario files gain `site.ems` with
  `limit` and `limit_clear`; a new case `curtailment-order`.
- **Checkpoint format 18** (phase 2 PR1): `EmsState::external_limit`,
  `limit_binding`, `reactive_cos_phi`, `reactive_under_excited`,
  `reactive_q_var`.
- **The plant reports its tracking and availability** (phase 2 PR2).
  Per closed quarter-hour, on the fiscal meter's boundary: the schedule
  deviation (metered mean less scheduled mean, kW and % of rating), the
  availability to discharge and to charge (the capability over the
  rating, time-averaged), the tracking error's RMS, and the redispatch
  potential (the energy the racks hold above the window's floor and
  below its ceiling, every tick); `site.kpi.*` at inputs 63 to 71,
  `schedule_deviation` (site bit 6) past a 5 % frame (labeled). The
  annual record gains a `control` section. Checkpoint 18 widened.
- **The plant holds balancing products** (phase 3 PR1). FCR (symmetric)
  and aFRR positive and negative as awards per four-hour block, from the
  EMS strategy (`DayAheadEms::with_awards`) or pinned by a scenario
  (`site.market`, `award`); the FCR award arms the droop and the
  reservoir rules of a limited energy store hold the SOGL working range
  (15 minutes each way), the alert state and the German degrees of
  freedom (dead-band recentering, 120 % over-fulfilment, a transaction on
  the plan); the awards take headroom from the plan. Prices: the 2024
  German tender results, fetched and compiled by the user
  (`fetch-balancing.sh`, `xlsx-to-csv.py`, `compile-balancing`,
  `--balancing`), the annual figures bundled and cited. Points
  `site.market.*` at 72 to 79, `fcr_alert_state` (site bit 7), a
  `market` taxonomy and the case `fcr-block`. Checkpoint 19.
- **The aFRR awards are served** (phase 3 PR2). The TSO's request is a
  kernel input (`Inputs::afrr_request_pu`, a fraction of the award in
  its direction), synthetic by default (the frequency's minute average
  plus a slow imbalance, labeled) or replayed from a file the user
  compiled (`compile-afrr`, `--afrr`; the per-second setpoint is the
  TSOs' copyright); the plant follows it at 0.5 % of the award a second
  (full in 200 s, inside ENTSO-E's five minutes), clamped at the award,
  and meters the energy each way. Points `site.market.afrr_request_pct`
  (80), `afrr_activated_kw` (81), `afrr_energy_pos_kwh` (83),
  `afrr_energy_neg_kwh` (85); the case `afrr-activation-day`. Checkpoint
  19 widened.
- **The fiscal meter books EUR** (phase 3 PR3). Four rows per
  quarter-hour at the prices the strategy names: the day-ahead value of
  the exchange at the hour's replayed price, the capacity the awards
  accrue, the aFRR energy at the merit order's floor (labeled), and the
  imbalance settlement of the period's schedule deviation at the
  day-ahead price (a labeled stand-in for the reBAP, deferred over the
  SMARD id question); `site.revenue.period_*_eur_x100` (87 to 95) and
  `site.revenue.total_eur` (97), Prometheus counters, a `revenue`
  section in the annual record. The first EUR the emulator publishes.
  Checkpoint 19 widened.
- **A strategy that decides** (phase 4 PR1). `MarketEms`: per local day
  and four-hour block, the best-paid use of the plant's power
  (arbitrage on the day's spread, FCR at the index's 80 % share, aFRR at
  50 % each way), the awards following the decision, and a day-ahead
  plan whose charges precede its discharges (the M2 finding made a
  rule). The bench runs the market year beside the clean one
  (`--market`, `calibration/m4-market.json`) with the revenue band as its
  gate, 97.5 to 195 kEUR/MW from the ISEA/enspired index, written before
  the number; `site.market.block_use` (99). The shell's default stays
  the day-ahead plan.

## Open questions

- Whether the synthetic frequency's standard deviation can be pinned to
  a published figure for 2024 or stays a labeled estimate from the
  2020 paper's era (the paper gives the shape and the pattern; its
  numbers are for 2015 to 2019).
- Whether the FCR product is modelled at the German four-hour block or
  at the SOGL product level only; leans the four-hour block, since the
  price data is per block.
- Whether the aFRR awarded share should be a fixed fraction of the plant
  (a config value) or chosen daily by the EMS on the capacity price;
  leans config in phase 3, EMS in phase 4.
- Whether the revenue gate's band should be widened for the intraday
  markets the emulator does not model (the index's trading figure
  includes the intraday auction and continuous trading); leans a stated
  lower bound at 50 % of the cross-market figure with the reason named.
