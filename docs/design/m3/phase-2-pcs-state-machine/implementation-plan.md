# M3 phase 2: PCS state machine and setpoint dynamics, implementation plan

Two PRs, each landing green.

## PR1: the machine

- `PcsOpState` seven values with SunSpec codes; `PcsState` gains the
  elapsed timer; `PcsModel::advance`; the return sequencer enters
  `Precharge`; availability counts from `Ramp`; events; viewer names;
  map 0.5.0 with the COMPATIBILITY.md row; checkpoint bump.
- Tests: the graph, the startup clock, the regenerated trip case.

Accept: `protection-trip-staggered-return` shows twenty blocks walking
through four states each in the journal, and the site's return takes the spacing plus the startup time.

*As built (feature/m3-phase2-pr1, 2026-10-21):* `kernel/startup.rs`
(`StartupSequence`, defaults 10/1/5 s), `PcsOpState` eight values with
`sunspec_code`, `is_connected`, `is_starting`, `is_converting`;
`PcsState::state_elapsed_s` (checkpoint 12); reset leaves `Off`,
maintenance enters `Off`; the startup walk runs before the share is cut
so a block that connects this tick takes its share this tick; map 0.5.0
row extended (breaking). Tests: the sequence's graph and clock, every
trip and maintenance test re-timed to release + 16 s, the library's two
trip cases and their headers. Golden regenerated (schema), fifteen
records regenerated (schema and the trip cases' timing); the annual
record unmoved.

## PR2: setpoint dynamics

- Dead time, ramp limit, first-order lag per block in `advance`;
  `blockNN.pcs.setpoint_followed_kw`; the setpoint-miss alarm reads the
  followed target, not the written one, so a legitimate ramp is not a
  miss.
- Tests: following property; the alarm's silence during a ramp.

Accept: an external setpoint step over Modbus is reached after the
published dead time and ramp, visible on the stream, without raising
`setpoint_not_met`.

*As built (PR2):* `SetpointDynamics` in `kernel/setpoint.rs` beside the
startup sequence, applied in `step_block` between the share and
`dc_request_w`; `Simulation::set_setpoint_dynamics` for research and
the switch-off; the block miss and the site shortfall both read the
follower. The annual record moved: the plan's hourly steps are ramps
now, so the night charge and the day's discharge each start ten minutes
of ramp later than the step; CALIBRATION.md has the figures. Golden and
all fifteen library records regenerated; four timing tests
(`trip_return`, `availability`, `aux_inventory`, and the return's step
count, which became a no-fall check) wait for the ramp. Checkpoint 13.

## Open questions

- SunSpec numbering for `Precharge` and `ContactorClose` (SunSpec has
  one `Starting`); either both carry `Starting` on the wire with the
  finer state only on MQTT and REST, or GW-01 extends the enumeration
  in a documented private range. Leans the first: the wire follows the
  standard, the richer surfaces say more. *Resolved in PR1:* the first.
