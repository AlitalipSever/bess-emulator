# M3 phase 2: PCS state machine and setpoint dynamics, implementation plan

Two PRs, each landing green.

## PR1: the machine

- `PcsOpState` seven values with SunSpec codes; `PcsState` gains the
  elapsed timer; `PcsModel::advance`; the return sequencer enters
  `Precharge`; availability counts from `Ramp`; events; viewer names;
  map 0.5.0 with the COMPATIBILITY.md row; checkpoint bump.
- Tests: the graph, the startup clock, the regenerated trip case.

Accept: `protection-trip-staggered-return` shows twenty blocks walking
through four states each in the journal, and the site's return takes
the spacing plus the startup time.

## PR2: setpoint dynamics

- Dead time, ramp limit, first-order lag per block in `advance`;
  `blockNN.pcs.setpoint_followed_kw`; the setpoint-miss alarm reads the
  followed target, not the written one, so a legitimate ramp is not a
  miss.
- Tests: following property; the alarm's silence during a ramp.

Accept: an external setpoint step over Modbus is reached after the
published dead time and ramp, visible on the stream, without raising
`setpoint_not_met`.

## Open questions

- SunSpec numbering for `Precharge` and `ContactorClose` (SunSpec has
  one `Starting`); either both carry `Starting` on the wire with the
  finer state only on MQTT and REST, or GW-01 extends the enumeration
  in a documented private range. Leans the first: the wire follows the
  standard, the richer surfaces say more.
