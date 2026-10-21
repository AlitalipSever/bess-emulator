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
  *As built:* 10 s, 1 s, 5 s, labeled estimates (CALIBRATION.md); the
  gradient and the dead time are PR2's.
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
  format 12 with `state_elapsed_s`; the filter's values arrive with PR2.
- **D5, events for every transition,** as today, with the new states;
  the viewer's journal row prints them, and the MQTT `pcs/state` topic
  carries the new numbering.

## Signal map impact

`blockNN.pcs.state` reinterpreted (breaking, map 0.5.0). Additions:
`blockNN.pcs.setpoint_followed_kw` (the filtered target the converter is
actually pursuing) at 2000 + 10b + 5.

## Checkpoint impact

One format bump (D4).

## Test plan

- **Unit: the graph.** Every allowed transition and every forbidden one.
- **Unit: the startup clock.** A released block reaches `Run` after the
  sum of the published durations, to the tick.
- **Property: setpoint following.** A step is followed without
  overshoot and settles within the published time; the ramp never
  exceeds the gradient.
- **Integration: the trip and return** library case regenerated and its
  header updated with the new timing.
- **Switch-off proof** and golden regeneration, called out.
