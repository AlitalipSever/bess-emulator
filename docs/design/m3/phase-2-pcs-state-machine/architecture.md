# M3 phase 2: PCS state machine and setpoint dynamics, architecture

What exists after this phase: a converter that starts up the way one
does and follows a setpoint the way one does. `PcsOpState` grows from
three values to seven: `Off`, `Standby`, `Precharge`, `ContactorClose`,
`Synchronize`, `Ramp`, `Run`, `Fault`; the staggered return M2 built
now walks each released block through precharge, contactor close and
synchronization before it ramps, and every transition is a
`PcsStateChanged` event with its real timing.

## The model

- **States and timers.** Each transitional state has a duration from
  the datasheet or the grid code (precharge seconds, contactor close
  sub-second, synchronization seconds); the state and its elapsed time
  are state. `Off` is a block under maintenance; `Standby` energized
  and idle; `Fault` as today.
- **Setpoint dynamics.** Per block, the AC target passes a dead time
  (controller cycle plus communication), then a ramp limit (kW/s from
  the grid code's power gradient), then a first-order lag (the current
  controller's settling). The filter state is state. The site's
  available power is unchanged; what changes is how fast the plant
  arrives at it.
- **Where it lives.** In `PcsModel`: the trait gains one method,
  `advance(&self, pcs: &mut PcsState, released: bool, dt_s)`, called
  before `dc_request_w`, which returns zero outside `Run` and `Ramp`
  and the ramp's share inside `Ramp`. The EMS still distributes; the
  converter follows.

## What it is not

- Not a protection model: the trip's cause stays the M2 fault.
- Not reactive power control: M4.

## Invariants

- Transitions are a closed graph; a block cannot reach `Run` without
  passing the three startup states, and `Fault` is reachable from any
  energized state.
- A setpoint step is followed monotonically, never overshoots, and
  settles within the published time.
- With all durations zero and the filter bypassed, the plant is the
  phase 1 plant byte for byte (switch-off proof).
