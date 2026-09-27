# M2 phase 2: alarm tree and event surface, architecture

What exists after this phase: alarms with a documented layout, raised by
the layer that owns their cause, latched where reality latches, published
on every surface, and clearable by an operator action. The `event`
publication class, designed in M0 and idle since, becomes code.

## Three alarm words

Alarms live where their cause lives, so there are three words, not one:

- **Rack word** (`RackState::alarm_bits`, exists since M0, reads zero
  today): BMS and cell conditions. Over/under temperature warning and
  trip, SoC window violations, imbalance warning and high, derate active,
  rack isolated.
- **Block word** (new, on the block): PCS and container conditions. PCS
  fault, PCS setpoint not met, HVAC failure, container over-temperature.
- **Site word** (new, on the site): protection and plant conditions.
  Protection trip, HV breaker open, site power limited, partial
  availability.

Each word is a u16 as published. The rack field stays u32 in state for
headroom, but the documented layout occupies the low 16 bits, because the
existing per-block register at base+6 folds rack words with OR into a u16
and that register's address does not move. The fold is the SCADA
convention: a block tells you that some rack has the condition, the MQTT
tree and Parquet tell you which.

Bit positions carry severity by convention: the low byte is warnings
(self-clearing), the high byte is trips (latched). The exact assignment is
frozen in this phase's first PR and documented in the signal map; after
1.0 a bit's meaning can never change, which is why the layout is decided
now, while it still can be wrong cheaply.

## Latching and reset

Warnings track their condition with hysteresis (raise above one threshold,
clear below another), because a chattering alarm is realistic only when a
scenario asks for chatter, not as the default behavior of a clean plant.
Trips latch: the condition clearing does not clear the bit. This is the
86 lockout convention from protection practice, modeled as observable
behavior rather than relay internals, per the non-goals.

Reset is an operator action: a new kernel command clears latched bits
whose cause is gone, and `PcsOpState::Fault` finally gets the exit its M0
doc comment promised ("requires a scenario or operator action to clear").
The surfaces expose it as a REST endpoint and, in phase 4, a scenario
action. A reset with the cause still present re-raises on the next tick,
which is exactly what a real plant does to an impatient operator.

## Events

The kernel `Event` enum widens: alarm raised, alarm cleared, each carrying
the node path, bit, and severity, alongside the existing PCS transition.
Events are facts about a tick, emitted by the kernel exactly once; the
surfaces fan them out:

- **MQTT:** report by exception under an `events/` subtree, immediate,
  regardless of the owning signal's decimation class.
- **Modbus:** the three alarm words at their addresses, plus a wrapping
  u16 event counter so a poller can detect that something happened between
  polls even if it also missed what.
- **WebSocket:** events join the summary stream.
- **Prometheus:** an events-total counter by severity.

The state tree remains the single truth: alarm words are state, events are
the kernel's log of state transitions, and no surface invents either.

## The chain, end to end

Phase 1 built the physics chain (heat, derate, missed setpoint). This
phase makes each link speak: derate active on the rack word, setpoint not
met on the block word, power limited on the site word, each raised by its
own layer's evaluation, in causal order across ticks. The integration test
that scripts a hot afternoon with a failed HVAC and asserts the order of
the raised alarms is the milestone's heart, and it is a kernel test, no
scenario engine required.

## Invariants

- An alarm bit set in a published word always has a corresponding raise
  event earlier in the log, and a cleared bit a clear event.
- Warnings are functions of state with hysteresis; trips are latched until
  reset; no third behavior.
- Event emission is deterministic: same tuple, same event log, byte for
  byte.
