# M4 phase 1: grid code at the POI, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the frequency is a shape, not a dataset.** No open-licence
  one-second series exists for the reference year: Energy-Charts'
  `v2/frequency` (Fraunhofer ISE, Freiburg, from 2022-05) is free for
  non-commercial and journalistic use only; Netztransparenz's
  per-second archive and regelleistung.net are the TSOs' copyright, and
  the Zenodo republication of TransnetBW's series is by permission, not
  an open licence. DATA-LICENSES.md's first rule therefore forbids
  bundling any of them. The bundled driver is synthetic and calibrated:
  Rydin Gorjão et al., "Open database analysis of scaling and
  spatio-temporal properties of power grid frequencies", Nature
  Communications 11, 6362 (2020), CC BY 4.0, describes Continental
  Europe as near-Gaussian with an approximately exponential
  autocorrelation decay and a clear fifteen-minute dispatch pattern;
  the standard deviation and the hour-boundary amplitude are labeled
  estimates (around 20 mHz and a few tens of mHz) until a public 2024
  figure is pinned. The replay path exists for anyone who fetches the
  measured year under their own acceptance: `fetch-frequency.sh` (122
  three-day requests), `compile-frequency` to i16 millihertz, loaded by
  `--frequency <file>` on the shell and the bench; the record file of a
  replayed run names the file's hash, so a replayed baseline is
  reproducible by whoever holds the same file. The alternative, a 2019
  year from the Zenodo set, was rejected on its terms and on the
  mismatch with the 2024 weather and prices. *As built (PR1):*
  `SyntheticFrequency` in `bess-models/src/grid_signal/synthetic.rs`
  (two octaves of value noise on 10 s and 120 s knots, the boundary
  sawtooth with the schedule's sign, 20/30/10 mHz labeled);
  `bess_data::FrequencyYear` with its codec, `compile-frequency` (60 s
  fill bound) and `fetch-frequency.sh`; `replay_frequency` attaches a
  year to the process once and `gw01_weather` carries it; the headless
  record gains a `frequency` hash field when one is attached.
- **D2, the LFSM characteristics are VDE-AR-N 4110's**, which names
  battery storage feeding at medium voltage as a generating unit: above
  50.2 Hz active power falls at 40 % of the instantaneous feed-in per
  hertz up to the 51.5 Hz disconnection; below 49.8 Hz the storage
  mirrors it (raising discharge or cutting charge), the gradient the
  same 40 %/Hz referred to rated power, labeled as the project's
  reading of the under-frequency rule since the public summaries give
  the over-frequency case only. The 110 kV rule (VDE-AR-N 4120) is not
  freely available; the 4110 figures are used and named as such, as M2
  did for the reconnection gradient. Disconnection at 51.5 and 47.5 Hz
  is a kernel trip (reusing `trip_site`). *As built (PR1):* `GridCode` in
  `kernel/gridcode.rs`, the gradient referred to rated power in both
  directions (labeled); the trip is `grid_frequency_trip` (site bit 10),
  judged on the frequency the substation read, a scenario's offset
  included, and cleared by the site reset.
- **D3, the FCR droop block follows the SOGL activation rules** as the
  Swissgrid prequalification annex (public, 2021) states them for
  Continental Europe: activation begins within 2 s, at least 50 % of the
  prequalified power at 15 s and 100 % at 30 s for a ±200 mHz deviation,
  at least linear, proportional below ±200 mHz, a combined insensitivity
  and dead band of at most 10 mHz; the German TSOs' ±200 mHz / ±10 mHz /
  30 s figures are the same rules. The block's output is rate-limited to
  reach full power in 30 s and is handed to the blocks as a fast term
  that bypasses the M3 follower's 10 %/min ramp (the follower is for
  third-party setpoints, the droop is the plant's own control). The
  over-fulfilment degree of freedom (up to 120 %) and the dead band's
  use for charging are parameters the phase 3 SoC manager will drive;
  phase 1 lands them at 100 % and "no use". *As built (PR1):* `FcrDroop`
  with `advance` on `EmsState` (`fcr_prequalified_w`, `fcr_output_w`);
  armed by `Simulation::set_fcr_prequalified_w` until phase 3's
  products; the fast term goes through `SetpointDynamics::follow` as a
  fifth argument and rides in `PcsState::fast_term_w`.
- **D4, reactive power on the converter's circle.** Each 5 MW converter
  is taken as 5 MVA (the SG2500U datasheet rates apparent power equal to
  active at unity, labeled), so Q at a given P is bounded by
  √(S² − P²), which at zero active power is the full rating: reactive
  support at zero active power, which VDE-AR-N 4110 asks of storage,
  comes for free from the circle. Three modes: fixed cos φ (the
  default at 1.0), fixed Q, Q(U) with a dead band of ±2 % around nominal
  and a slope reaching the 4110 range limit (cos φ 0.9 at rated P, so
  Q = 0.484 P_rated, labeled) at ±5 % voltage, first-order settling
  with a 10 s time constant (labeled; the standard asks for a settling
  time of seconds to tens of seconds). The transformer consumes
  I²·X with a 12 % short-circuit impedance on the 100 MVA base (labeled,
  the common figure for an HV/MV unit of that size) and the POI reads
  the difference; the power factor is published.
- **D5, checkpoint bump.** Format 17: `PcsState::q_var`,
  `EmsState::reactive_mode`, the droop block's output state and the Q(U)
  filter state, `SubstationState::poi_power_factor`. *As built (PR1):*
  format 17 carries the droop's state, the LFSM cut, the fast term and
  the substation's `frequency_offset_mhz`; PR2 adds the reactive fields
  to the same format, since 17 is unreleased.
- **D6, points and bits.** Per block `blockNN.pcs.q_kvar` (2000 + 10b +
  8, analog class 0.5); site `site.poi.power_factor` (59),
  `site.grid_code.state` (60: 0 normal, 1 lfsm_o, 2 lfsm_u, 3 fcr),
  `site.fcr.output_kw` (61); site word bits `lfsm_active` (4, warning
  byte, raised while either characteristic cuts the target) and
  `grid_frequency_trip` (10, trip byte, latched). `site.poi.frequency_hz`
  and `site.poi.reactive_power_var` keep their addresses and gain life.
  *As built (PR1):* `site.grid_code.state` (60) and `site.fcr.output_kw`
  (61, i32, so it takes 61 and 62), both exact; bits 4 and 10 as planned;
  `site.poi.frequency_hz` carries the new series. Map 0.6.0. The scenario
  engine gained `site.grid` with `frequency_offset` (`mhz`, −3000 to
  3000, 0 ends it) and a `grid` taxonomy for stories that are the grid's,
  not the plant's.

## Signal map impact

Four additions at unused addresses, two site bits (map 0.6.0, unreleased
until the milestone). `site.poi.reactive_power_var` changes value, same
meaning.

## Checkpoint impact

One format bump (D5).

## Test plan

- **Unit: the synthetic frequency** has the stated standard deviation
  within a tolerance over a day, a mean of 50.000 Hz within 1 mHz, and
  the hour-boundary step visible as a larger mean absolute deviation in
  the first minute of the hour than in the last.
- **Unit: LFSM** at 50.5 Hz the discharge target reads 88 % of a 100 MW
  target (40 %/Hz × 0.3 Hz = 12 % of rated); at 51.5 Hz the site trips;
  at 49.8 Hz the mirror.
- **Gate: the FCR shape test.** A −200 mHz step: at least 50 % of the
  prequalified power at 15 s and 100 % at 30 s, linear; a −100 mHz step
  half of that; inside ±10 mHz nothing.
- **Property: P² + Q² ≤ S²** per block over a day in every mode; the POI
  power factor at unity in the default mode within the transformer's
  consumption.
- **Integration:** a new library case `frequency-event` (a synthetic
  −300 mHz dip for 60 s, the plant answers through the droop and LFSM-U,
  then returns); the annual record with the synthetic driver (the M1
  gate still holds). *As built (PR1):* `frequency-event` (taxonomy
  `grid`): the dip answered through LFSM-U alone, since nothing arms the
  droop before phase 3; a 400 mHz rise cut through LFSM-O; a fall to
  47.4 Hz that trips the site, the reset and the return after. Its lesson
  test is in `library_lessons.rs`; the plant tests in
  `bess-models/tests/grid_code.rs`. The annual record's figures did not
  move; every library record did (the frequency in the tree).
