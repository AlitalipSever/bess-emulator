# M2 phase 5: data faults, library, gate, architecture

What exists after this phase: telemetry that can lie exactly the way real
telemetry lies while the kernel stays right underneath; a scenario library
whose failure mix is calibrated against the public incident record; and a
CI mode that runs a scenario headless and fails on drift. Then v0.5.0.

## The tap

Data faults corrupt the observation, never the state. The tap is a
deterministic transformer sitting on each publishing surface's read path:

- **Modbus:** between the snapshot's register bank and the wire. Freeze
  holds registers at captured values, unit errors scale a point by a
  power of ten, dropout closes or refuses the TCP session.
- **MQTT:** between point iteration and publish. Dropout stops
  publishing, freeze repeats last payloads, NaN bursts emit NaN payloads
  (a fault MQTT can express and Modbus u16 cannot, which is realistic:
  each protocol lies in its own dialect), timestamp drift skews the
  device-clock field, chatter flaps alarm topics, backfill replays the
  dropped-out interval's messages with old timestamps after a dropout
  ends.
- **REST and WebSocket stay truthful.** That is a product decision, not
  an accident: the debugging surfaces tell the truth so a user can see
  the gap between what the plant did and what SCADA saw. It is also the
  fault-finding workflow the emulator exists to teach.

The tap's schedule comes from the phase 4 player; its own randomness (which
point freezes, jitter within a burst) draws from a PRNG stream seeded by
(seed, surface), so kernel trajectories are untouched by observation
faults, byte for byte: the golden-snapshot digest of the state is
identical with and without data faults, and a test says so.

## The library

`scenarios/` at the repository root, one YAML per case, each carrying its
EPRI taxonomy tag, plus a README that is the library's contract: what a
case demonstrates, what a consumer should observe. Cases cover the
physical set (HVAC failure in a heat week, PCS trip, protection trip with
staggered return, self-discharge drift), the data set (dropout with
backfill, frozen SCADA day, unit-error audit trap, alarm chatter), the
calendar (both 2024 DST days), and maintenance windows.

## The gate

The EPRI BESS Failure Incident Database is the public record of what
actually breaks: balance-of-system and controls dominate, cells are rare.
The gate is a CI test that reads every scenario's taxonomy tag and holds
the library's composition to the database's shares, with the snapshot
(shares, retrieval date) pinned in CALIBRATION.md. It is a distribution
check over the library, not a simulation measurement: what is calibrated
is the mix of stories we ship, which is exactly what the roadmap's gate
sentence promises and nothing more.

## CI assertion mode

`bess-emulator --scenario s.yaml --assert digest.json`: run headless as
fast as possible, compare the final state digest and event-log hash,
exit nonzero on drift. Every library case carries its digest; CI runs
the library on every PR the way it already runs the annual gate. A
scenario whose digest moved is either a deliberate physics change (the
M1 regeneration discipline applies, loudly) or a regression caught.

## The release

v0.5.0 closes the milestone: COMPATIBILITY.md's first announced break
(alarm bits mean something) ships with map 0.3.0, ROADMAP.md flips M2 to
done, CALIBRATION.md carries the M2 entry, and the release notes are the
milestone README's inventory, edited.
