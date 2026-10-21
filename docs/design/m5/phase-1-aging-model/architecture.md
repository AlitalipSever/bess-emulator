# M5 phase 1: the aging model, architecture

What exists after this phase: racks that age. Every rack carries two
capacity-loss states and two resistance-growth states, advanced once per
simulated day from a stress ledger the tick fills, and the cell model
reads them, so a rack that has been through a year delivers less and
heats more than it did at commissioning. Nothing on the surfaces changes
yet beyond `site.soh_pct` reading what the racks say; phase 3 adds the
points.

## The model

- **A new layer trait.** `bess-core/src/traits.rs` gains `AgingModel`:
  `fn age_rack(&self, age: &mut RackAge, cfg: &RackConfig, day:
  &StressDay)`. It is called once per rack per closed ledger (a day), not
  per tick, which is the cadence the published models were identified
  at (daily to weekly check-ups) and the cadence that keeps the kernel's
  tick cost flat. `Models` gains `aging: Box<dyn AgingModel>`.
- **The stress ledger, per rack.** `bess-core/src/state/aging.rs`:
  `StressLedger { seconds, soc_sum, temp_k_sum, throughput_ah,
  abs_current_sum_a, active_s, soc_min, soc_max }`, filled every tick
  from the rack's state after the cell model steps it. When `seconds`
  reaches a day it closes into a `StressDay { days, efc, soc_mean,
  temp_k_mean, dod, c_rate }`: the day's equivalent full cycles
  (throughput over twice the nominal capacity), its mean state of
  charge and cell temperature, its depth of discharge (the day's
  state-of-charge range, the proxy the NREL model uses over a window),
  and its mean rate while current flowed. The closed day is kept on the
  rack (`last_day`) so an observer outside the kernel can collect a
  stress year (phase 2).
- **The aging states, per rack.** `RackAge { days, efc, q_loss_cal,
  q_loss_cyc, r_gain_cal, r_gain_cyc, rate_scale, ledger, last_day }`.
  `RackState::soh` becomes derived: `1 - q_loss_cal - q_loss_cyc`,
  written by the kernel after each aging step so every reader that
  already uses `soh` (the cell model's capacity, the BMS's spread law,
  the stored-energy integral) sees it. Resistance growth is read by the
  cell model as a multiplier beside the manufacturing spread:
  `r0 * resistance_scale * (1 + r_gain_cal + r_gain_cyc)`. `rate_scale`
  is the rack's own aging speed, drawn once at commissioning from the
  seeded PRNG on a fourth stream so no earlier draw moves.
- **The kernel's daily pass.** `bess-core/src/kernel/aging.rs`: after the
  blocks step, `observe` adds the tick to every rack's ledger; a rack
  whose ledger closed hands its day to the model, which advances the
  four states; the kernel writes `soh` back and clears the ledger. A
  rack out of service still ages by the calendar (its ledger sees zero
  current and the air it sits in).
- **The LFP model.** `bess-models/src/aging.rs` (module directory
  `aging/`): `LfpAging { capacity: NrelLfp250, resistance:
  NaumannResistance }` implements `AgingModel`; `NoAging` leaves the
  states where they are, the switch-off proof's model. `aging/nrel.rs`:
  the BLAST-Lite `Lfp_Gr_250AhPrismatic` capacity model, two power-law
  trajectories (`q_loss_cal = k_cal * days^0.526`, `q_loss_cyc = k_cyc *
  efc^0.828`) with rate coefficients in temperature, anode potential
  (from the state of charge through the graphite open-circuit potential
  the model uses), depth and rate. `aging/naumann.rs`: the resistance
  laws, calendar growth linear in time with an Arrhenius term and a
  state-of-charge term, cycle growth linear in throughput with depth and
  rate terms. `aging/power_law.rs`: the state update for a power-law
  trajectory (the change over a step at the trajectory's current slope,
  so a changing rate is integrated rather than restarted).
- **Deterministic arithmetic.** The model needs `exp` (Arrhenius, the
  anode potential), `tanh` (the anode potential) and fractional powers.
  `bess-models/src/math.rs` makes `exp_pos` public beside `pow_pos`, and
  gains `tanh_pos` built on it; the kernel contract (no transcendental
  calls in the physics) holds, and the determinism test covers the
  aging year.

## Data flow

```
tick:   cell model steps the rack (reads soh, r_gain)
        ledger.observe(rack, dt)                       every rack, every tick
day:    ledger.close() -> StressDay                     when seconds >= 86,400
        models.aging.age_rack(age, cfg, &day)           four states advance
        rack.soh = 1 - q_loss_cal - q_loss_cyc
        age.last_day = Some(day)                        phase 2 collects it
```

## Invariants

- With `NoAging`, the tree after any run is byte-identical to the tree
  the v0.9.0 kernel left, ledger fields aside: aging is additive.
- `soh` never rises; `r_gain` never falls; both are per rack and the
  spread between racks comes from `rate_scale` and from their own
  ledgers, never from a shared counter.
- The energy balance closes with an aged rack: a capacity loss is not an
  energy flow (the stored-energy integral shrinks with `soh` at the same
  state of charge, and the bench's residual gate stays at zero because
  the day's close books the lost stored energy to a new `aging` row of
  the loss waterfall, the way the balance treats nothing else).
- Determinism: the aging year's digest is identical on macOS and Linux.
