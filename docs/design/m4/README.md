# M4: EMS and market signals

Status: designed 2026-10-21 after M3 shipped as v0.8.0; no phase has
landed. ROADMAP.md defines the goal and the calibration gate; this folder
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
