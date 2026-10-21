# M3 phase 2: PCS state machine and setpoint dynamics, design

Each decision is proposed here and confirmed or revised in its PR.
As built in PR1 (2026-10-21): D1, D3, D4, D5 confirmed; D2 confirmed
with the durations labeled estimates; the plan's open question resolved
toward the first option.

## Decisions

- **D1, seven states, SunSpec-named.** SunSpec model 702/704 publishes
  an inverter operating state enumeration (Off, Sleeping, Starting,
  MPPT/Running, Throttled, Shutting down, Fault, Standby); GW-01's
  seven map onto it and the published register carries SunSpec's
  numbering where a value exists, so an integrator who knows SunSpec
  reads the enum without the CSV. Breaking: `blockNN.pcs.state` moves
  from the M0 three-value enum, map 0.5.0, called out in
  COMPATIBILITY.md as promised since M0. *As built:* eight values with
  `Off` added (contactors open: after a reset, under maintenance), the
  three startup states share SunSpec's `starting` (3) on the wire and
  carry their names on MQTT and REST; `Ramp` and `Run` are `running`
  (4), `Standby` 8, `Fault` 7, `Off` 1.
- **D2, durations from public sources, ranges labeled.** Precharge and
  synchronization from a utility-scale converter manual where one is
  public; the power gradient from VDE-AR-N 4110 (10 % of agreed power
  per minute, public) as the ramp's default; the dead time from the M0 tick (1 s) plus one controller cycle, an estimate. Each labeled.
  *As built:* 10 s, 1 s, 5 s, labeled estimates (CALIBRATION.md). PR2:
  the gradient is 10 % of the converter's rating per minute (VDE-AR-N
  4110, public); the dead time is the kernel's own tick, a setpoint
  written between ticks acting on the next one and the filter starting
  from what the previous tick left, so no delay line is kept; the lag is
  a first-order 0.5 s, a labeled estimate, stepped by implicit Euler so
  the kernel stays rational.
- **D3, the return sequencer hands over to the machine.** M2's
  `blocks_released` count still spaces the blocks; a released block
  now enters `Precharge` rather than `Run`, so the site's return takes
  the spacing plus the startup time, and `site.available_discharge_kw`
  counts a block only from `Ramp`. *As built:* `restart::cleared` says
  whether the controller has cleared a block, `dispatchable` adds the
  converter's connected state; the sequence lives in the kernel beside
  the return policy (`StartupSequence`), not in `PcsModel`, since it is
  the observable order of a start and no trait owns it (the architecture
  note's `advance` method was not added). Maintenance opens the
  contactors (`Off`) and a block handed back starts its sequence.
- **D4, checkpoint bump.** State, elapsed time, the setpoint filter's
  two values (ramped target, lagged output) per block. *As built:*
  format 12 with `state_elapsed_s`; format 13 (PR2) adds
  `setpoint_ramped_w` and `setpoint_followed_w`.
- **D5, events for every transition,** as today, with the new states;
  the viewer's journal row prints them, and the MQTT `pcs/state` topic
  carries the new numbering.

## Signal map impact

`blockNN.pcs.state` reinterpreted (breaking, map 0.5.0). Additions:
`blockNN.pcs.setpoint_followed_kw` (the filtered target the converter is
actually pursuing) at 2000 + 10b + 5. *As built:* there, exact (a
controller value), folded into 0.5.0 since it is unreleased. The
setpoint-miss bit reads the followed target; the site's power-limited
bit discounts what connected converters are still ramping toward, so a
plant arriving at the grid code's gradient is not limited and a plant
that cannot arrive still is (a staggered return reads limited as
before).

## Checkpoint impact

One format bump (D4).

## Test plan

- **Unit: the graph.** Every allowed transition and every forbidden one.
- **Unit: the startup clock.** A released block reaches `Run` after the
  sum of the published durations, to the tick.
- **Property: setpoint following.** A step is followed without
  overshoot and settles within the published time; the ramp never
  exceeds the gradient. *As built:* `kernel/setpoint.rs` unit tests and
  `bess-models/tests/setpoint_following.rs` (a site step over the
  control surface reached inside the ramp with no block or site alarm, the
  switch-off, a checkpoint mid-ramp).
- **Integration: the trip and return** library case regenerated and its
  header updated with the new timing.
- **Switch-off proof** and golden regeneration, called out.
