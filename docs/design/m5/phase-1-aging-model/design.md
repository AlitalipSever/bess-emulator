# M5 phase 1: the aging model, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the capacity model is NREL's large-format prismatic LFP model.**
  *As built (PR1):* `aging/nrel.rs::NrelLfp250::blast_lite_2023`, the
  anode potential in `anode_potential_v`; the year-one figures held in
  `year_one_at_the_design_stress`.
  Of the public LFP aging models with parameters a reader can check,
  two are identified on cells, not on a chemistry in the abstract:
  Naumann et al.'s Sony/Murata 26650 model (TUM, 2018 and 2020) and
  Gasper's BLAST-Lite `Lfp_Gr_250AhPrismatic` (NREL, 2023), fitted on a
  commercial prismatic cell above 250 Ah with an energy-to-power ratio of
  six hours. GW-01's rack is 416 cells of a 314 Ah prismatic class at
  0.5C, so the NREL cell is the closer analogue by an order of magnitude
  in format, and its own notes name what it cannot do (the cycling data
  was taken at conservative rates, 0.65C charge at most, and the cycle
  term carries an inverse temperature dependence the fit learned to
  balance the calendar term inside 10 to 45 °C, so it extrapolates
  badly outside). The Sony cell is a high-power 3 Ah cylinder with
  exceptional cycle life; its capacity law would understate a 314 Ah
  cell's cycle fade. Both are BSD 3-Clause, so the constants can be
  carried with attribution. Trajectories: `q_loss_cal = k_cal *
  t_days^0.526` with `k_cal = 8.37e4 * exp(-5210 / T) * exp(-3560 * Ua /
  T)`, and `q_loss_cyc = k_cyc * efc^0.828` with `k_cyc = (4.38e-8 +
  1.55e-8 * dod + 1.68e-7 * c_rate) * (exp(2190 / T) + exp(-1.55e5 /
  T))`, `T` in kelvin, `Ua` the graphite anode potential at the mean
  state of charge (Safari and Delacourt's function on Schimpe's
  lithiation map, as BLAST-Lite computes it). At 25 °C, a mean state of
  charge of 0.5, 0.8 depth, 0.5C and 280 cycles a year the model gives
  1.1 % calendar and 2.3 % cycle loss in year one and a state of health
  of 0.887 after five years, which is the kind of number the gate's band
  (README) was derived to hold, derived from other sources. The
  alternative, a model of the project's own fitted on nothing, was not
  considered.
- **D2, the resistance model is Naumann's**, (*as built (PR1):*
  `aging/naumann.rs::NaumannResistance::simses`), because the NREL model
  publishes no resistance law. Calendar: `r_gain_cal += 3.4194e-10 *
  exp(-71827 / R * (1/T - 1/298.15)) * (-3.3903 * (soc - 0.5)^2 +
  1.5604) * dt_s`; cycle: `r_gain_cyc += (-0.0020 * c_rate + 0.0021) *
  (6.8477 * (dod - 0.5)^3 + 0.91882) * delta_efc / 100`. About 1.7 % a
  year at rest at 25 °C and 0.4 % per 300 cycles, labeled as a different
  cell's law in CALIBRATION.md. The field study's finding that capacity
  loss dominates and resistance is secondary (Figgener 2024) is the
  check the label leans on. The constants are the ones SimSES carries
  from the papers (BSD 3-Clause).
- **D3, the cadence is a day, the ledger is sums.** (*As built (PR1):*
  `StressLedger` and `StressDay` in `bess-core/src/state/aging.rs`,
  rates under 0.01C counted as rest as BLAST-Lite does; the pass in
  `kernel/aging.rs` runs after the blocks step.) The models are
  identified on check-ups days apart and their rate coefficients are
  functions of window averages; evaluating them per tick would cost a
  transcendental call per rack per second for no information. The ledger
  keeps sums and extrema only (eight numbers), closes at 86,400 s of
  ledger time from the rack's last close (the run's day boundary, not
  local midnight: a day's stress is one row whatever the start time),
  and the depth of discharge is the day's state-of-charge range, the
  same window proxy BLAST-Lite uses when it chunks by day rather than by
  rainflow. A rainflow count was rejected for phase 1: the day-ahead
  plan makes one or two cycles a day, which the range captures, and the
  FCR service's small cycles are below the models' resolution.
- **D4, the states are power laws integrated at the current slope.**
  `q = k * x^p` advanced by `dq = k * p * (q / k)^((p - 1) / p) * dx`
  (BLAST-Lite's `_update_power_state`), so a day at a higher temperature
  adds the loss that day would add at the trajectory's present point,
  not the loss of a fresh cell. A simple sum of daily rates was rejected
  because it restarts the square-root-like curve every day and
  overstates the first years by a factor near two. *As built (PR1):*
  `aging/power_law.rs::advance`; at a unit step the discretization is
  under 2 % of the closed form at p = 0.526 and converges with the step.
- **D5, the spread is a scalar per rack.** `rate_scale` drawn uniformly
  in [0.9, 1.1] at commissioning (labeled; the field study's spread
  between identical systems is about 2 percentage points a year on a
  2 mean, but it includes operation, which the ledger carries on its
  own). It multiplies every state's increment. The alternative, no
  spread, would make the SoH minimum equal the mean and the warning bit
  raise on every rack at once, which no plant does. *As built (PR1):*
  `RackAge::rate_scale` from a fourth PRNG stream (`AGING_STREAM` in
  `state/init.rs`), so no earlier draw moved.
- **D6, the lost stored energy is booked.** A capacity loss at constant
  state of charge lowers the stored-energy integral; the bench's residual
  gate would read it as unexplained energy. The daily close computes the
  stored energy before and after the state change and books the
  difference to an `aging_wh` row of the energy accounting, which the
  loss waterfall publishes beside the battery's heat. It is not a loss
  in the heat sense and the record says so. *As built (PR1):*
  `EnergyAccounting::aging_wh`, `LossKpis::aging_mwh` ("Aging: capacity
  written off"), on the loss side of the bench's residual.
- **D7, `soh` stays the single truth.** The BMS reports it as it is;
  an estimator with error is named as a later deepening (the reported
  state of charge got one in M2.5, and the same pattern applies).
- **D8, the warranty threshold is 80 %**, the rack bit's level in
  phase 3 and the "cycles to 80 %" the laboratory band is stated in;
  70 % (the datasheet's end of life) is published as a status, not a bit.

## Signal map delta

None in this phase beyond the value of `site.soh_pct` (7), which has
read 100 since M0 and now reads the mean state of health of in-service
racks in whole percent. *As built (PR1):* `SiteState::average_soh`. Map 0.7.0 is cut in phase 3 with the additions.

## Checkpoint impact

Format 20: `RackState::age` (the four states, `rate_scale`, the ledger,
`last_day`), `EnergyAccounting::aging_wh`. The library's records and the
golden digest regenerate (the aging year moves every long case by its
first days of fade; a one-hour case moves by the ledger fields only).
*As built (PR1):* as designed; the golden digest moved to
0x5750_152e_0a3e_abf4 and all twenty-one library records regenerated.

## Test plan

- `aging/power_law.rs`: the state update follows the closed form at
  constant rate to 1e-9 over a thousand days; a rate change integrates at
  the current slope (the two-rate trajectory lands between the two
  closed forms).
- `aging/nrel.rs`: the year-one figures of D1 at the stated stress to
  three digits; calendar loss rises with temperature and with state of
  charge; cycle loss rises with depth and with rate.
- `aging/naumann.rs`: the D2 figures.
- `bess-models/tests/aging.rs`: a rack held at rest for a simulated year
  ages by the calendar only; a rack cycled daily ages faster; an aged
  rack stores less at the same state of charge and heats more at the
  same current; two racks with different `rate_scale` diverge; the
  no-aging proof (`NoAging` leaves the v0.9.0 tree, ledger aside); the
  determinism test covers an aging week.
- `bess-core`: the checkpoint round-trip carries the age; `soh` is
  written from the states, never read from the file.
