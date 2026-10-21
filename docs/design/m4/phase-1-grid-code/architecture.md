# M4 phase 1: grid code at the POI, architecture

What exists after this phase: a grid input with a shape, and a plant
that answers it. The frequency at the POI is either a replayed measured
year (fetched by the user) or a synthetic series with the Continental
European statistics, no longer a 10 mHz sinusoid; the converter fleet
reduces or raises active power on the LFSM characteristics of the
connection codes, serves an FCR droop when the EMS arms one, and
delivers reactive power in a mode an operator chose, the apparent-power
circle of each converter and the transformer's reactive consumption
accounted for, so `site.poi.reactive_power_var` stops reading zero.

## The model

- **Frequency driver.** `bess-models/src/grid_signal.rs` (new module
  directory `grid_signal/`): `SyntheticFrequency`, a pure function of
  the timestamp built from value noise on 10 s knots (FNV-hashed, linear
  between knots, so the kernel stays rational) with a stated standard
  deviation, plus the deterministic hour-boundary deviation (a step at
  the hour that decays over minutes, the quarter-hour pattern smaller),
  parameters labeled estimates against the published statistics.
  `FrequencyReplay` in `bess-data/src/frequency.rs`: a compiled 1 s year
  (i16 millihertz) the user builds from Energy-Charts with
  `fetch-frequency.sh` and `compile-frequency`; loaded at run time by
  path, never bundled. Both drivers hand the kernel
  `Inputs::grid_frequency_hz`, unchanged in type.
- **Grid-code block, per plant.** `bess-core/src/kernel/gridcode.rs`:
  `GridCode { lfsm_o_hz: 50.2, lfsm_u_hz: 49.8, gradient_per_hz: 0.4,
  disconnect_over_hz: 51.5, disconnect_under_hz: 47.5 }` applied to the
  site target after the EMS and before the share is cut: above the
  over-frequency threshold the discharge target falls (and a charge
  target rises) by 40 % of rated power per hertz; below the
  under-frequency threshold the mirror; outside the disconnect band the
  kernel trips the site like a protection trip. `FcrDroop {
  prequalified_w, deadband_hz: 0.010, full_hz: 0.200, activation_s: 30,
  overfulfil: 1.0..=1.2 }` with a ramp-limited output state, summed onto
  the site target when armed. Both act on the site target, so the
  converters' own follower (M3) still shapes the step; the FCR ramp
  limit is the SOGL's (at least linear to full in 30 s), which is faster
  than the follower's 10 %/min, so the follower is bypassed for the
  droop share: the droop's share is handed to the blocks as a separate
  fast term the follower does not ramp.
- **Reactive power.** `PcsModel` gains `reactive(&self, pcs, q_target_var)
  -> f64`: the converter's apparent-power circle
  (`S_rated` = the 5 MW rating as 5 MVA, labeled) bounds Q at the
  current P; `PcsState::q_var`. `ReactiveMode { FixedCosPhi(φ), FixedQ(var),
  QofU { slope, deadband } }` in `EmsState::reactive_mode`, default
  `FixedCosPhi(1.0)` (the M3 plant). `GridInterface::step` takes the
  total Q and subtracts the transformer's reactive consumption (I²·X with
  a labeled 12 % impedance) to the POI; `SubstationState::poi_reactive_power_var`
  moves, `poi_power_factor` is new.
- **Where the mode is set.** In `EmsState` (the operator's choice), by a
  kernel command `set_reactive_mode` (phase 2 puts it on the shell).

## What it is not

- Not a market: the droop block exists; who arms it and for what price
  is phase 3.
- Not a voltage controller for the grid: Q(U) reads the POI voltage the
  tap changer and the grid input set; it does not move the grid.
- Not a replay of any copyrighted frequency series inside the
  repository.

## Invariants

- With the synthetic driver's amplitude set to zero and the modes at
  their defaults, the plant is the v0.8.0 plant byte for byte
  (switch-off proof).
- The LFSM response never asks a converter for more than its permitted
  AC power; the droop's share never pushes a block over its rating plus
  the overload the M3 budget allows.
- Reactive power never leaves the converter's circle: P² + Q² ≤ S² at
  every tick, per block.
- Energy balance unchanged: reactive power carries no energy; the
  transformer's reactive consumption is a var figure, not a loss row.
