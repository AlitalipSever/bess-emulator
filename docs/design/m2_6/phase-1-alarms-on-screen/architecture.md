# M2.6 phase 1: alarms on screen, architecture

What exists after this phase: the three alarm words and the event stream,
which every native surface has published since M2, are visible in the
browser viewer. An alarm summary lists what is active, by node and
severity; an alarm journal lists what raised and cleared, in the kernel's
order and with its numbering; the 3D scene shows severity where the cause
lives; a row in the list selects the object in the scene and a selected
object lists its alarms; and the operator reset, an HMI button since M2
phase 2, exists in the viewer. One helper moves into `bess-core` so that
the viewer and the native surfaces spell a node the same way.

Pinned on 2026-10-17 against `develop` after PR #26: `Event` with four
variants, `alarms::layout` with the frozen bit tables and `name()`,
`SiteState::event_log` with count and digest, `Simulation::step`
returning the tick's events, `Simulation::reset_alarms(ResetScope)`, the
viewer stepping in `viewer.rs` and `viewer/jump.rs`, the panels in
`panels/{site,controls,detail}.rs`, the scene's per-frame instances in
`instances/live.rs`.

## Three things the kernel already offers, and which the view reads

The kernel carries alarms in three forms, and the view uses each for what
it is:

- **The words** (`RackState::alarm_bits`, `BlockState::alarm_bits`,
  `SiteState::alarm_bits`) are the present. The alarm summary is derived
  from them every frame: walk the tree, collect the set bits, name them
  through `alarms::layout::name`, sort. Nothing is stored. A summary that
  was built from events would drift after a restart, a scenario load or a
  frame the viewer skipped; one built from the words cannot, because the
  words are the truth every surface is a projection of.
- **The events** `step` returns are the past, one tick at a time. The
  viewer appends them to a bounded journal with the simulation time of
  the tick that shows their effect and the kernel's own number (the log
  count before the tick, plus the event's position), the same `seq` and
  `ts` MQTT and the stream carry. The journal is view state: it is not in
  the tree, not in the checkpoint, and it is cleared whenever the plant
  is replaced.
- **The log's count** (`event_log.count`) is what the "events since you
  last looked" figure in the panel header is read from. No second tally.

The invariant that follows: the summary and the journal never feed each
other. If they disagree (a bit set with no raise in the journal while the
journal has not wrapped), that is M2 phase 2's first invariant broken in
the kernel, not a view bug, and a test can say so.

## Where the code goes

`bess-scene` keeps its rule: the scene and the panels read `&SiteState`
and emit `ViewerCommand`s; the viewer (feature `sim`) is the only place
that touches the kernel. New files, each one concern:

| File | Concern | Needs GL |
|---|---|---|
| `bess-scene/src/alarms.rs` | the alarm vocabulary as the view uses it: an active entry (node, bit, name, severity), the walk of the tree that collects them, the sort order, the severity palette lookup | no |
| `bess-scene/src/alarms/journal.rs` | the bounded ring of `(seq, unix_time_s, Event)`, append and iterate newest first, the unseen count | no |
| `bess-scene/src/panels/alarms.rs` | the summary table and the journal table, the reset buttons, the filter toggles | yes |
| `bess-scene/src/instances/live.rs` | changes in place: severity-coded indications per container, PCS skid and substation, a mark on a failed HVAC unit | no |
| `bess-scene/src/viewer.rs` | records each tick's events into the journal; handles `ViewerCommand::ResetAlarms` | yes |
| `bess-core/src/alarms/names.rs` | the published spelling of a node (`block02/container1/rack05`), of a fault action's node, and of a severity, moved from `bess-emulator/src/events.rs` | no |

`alarms.rs` and its journal stay buildable without the `sim` feature,
like `Scenery` and `Stop`, because they are panel vocabulary and the
panels build without a kernel in hand.

## One spelling of a node

The MQTT topics, the stream payloads and the REST reset all spell a node
through `events.rs` in the native shell, and COMPATIBILITY.md publishes
that spelling. The viewer needs the same words for its rows, and
`bess-scene` cannot depend on the shell. The spelling moves down to
`bess-core`, beside `alarms::layout::name` which already owns half of it:
`AlarmNode::path()`, `FaultAction::node_path()`, `Severity::name()`.
`events.rs` calls them. No surface changes a byte; the shell's payload
tests, which hold the strings field for field, are the proof, and so is
the MQTT acceptance test against its own broker.

This is the phase's only change to a kernel crate. It is a move of three
functions and the strings they format; it touches no state, no model, no
checkpoint and no digest.

## The scene

Severity is shown where the cause lives, which is the M2 principle
applied to pixels:

| Object | Reads | Trip (high byte) | Warning (low byte) | Informational |
|---|---|---|---|---|
| container status diode | the worst rack word in it | red, steady | amber, steady | out-of-service tone when a rack is isolated and nothing else is up |
| HVAC end unit | `hvac.failed` | amber mark on the unit; the fan is already still | | |
| PCS skid diode (new) | the block word | red (`pcs_fault`) | amber (`setpoint_not_met`, `container_over_temp`, `hvac_failure`) | out-of-service tone (`maintenance`) |
| substation marker (new, on the transformer) | the site word | red (`protection_trip`) | amber (`power_limited`), red steady for `hv_breaker_open` since the site is off the grid | `partial_availability` as amber |

Nothing blinks. The current diode blinks whenever a rack word is nonzero
or power flows; the convention the operator standards reserve blinking
for is an unacknowledged alarm, and this plant has no acknowledgement
(design D6). Power keeps its pulse on the SoC gauge, which is where it
was always readable.

A selected object keeps its yellow frame. Clicking a summary or journal
row selects the object that holds the node: a rack row selects its
container, a block row its PCS skid, a site row the transformer. The
camera does not move; the auto-orbit and the frame are enough to find a
container on a 160 m site, and a camera focus is an open question rather
than a requirement.

## The reset

`ViewerCommand::ResetAlarms(ResetScope)` goes to `Simulation::reset_alarms`,
the same door REST uses. The answer, the bits whose cause is still present,
is shown in the panel until the next tick raises them again, which the
journal then shows as the kernel does it. A site reset on a tripped site
closes the breaker and starts the staggered return, as M2 phase 3 defined;
the viewer adds no semantics of its own.

## Invariants

- The summary equals the set bits of the three words at the frame it is
  drawn; it is never carried over between frames.
- The journal's entries carry the kernel's numbering, and consecutive
  entries differ by one until the ring wraps, after which the oldest
  entry's number says how many were dropped.
- The view writes nothing into the tree. The reset is the kernel's own
  command through the kernel's own door. The golden digest, the annual
  record and the library's records do not move.
- The spelling of a node in a viewer row is byte-identical to the
  spelling on MQTT and the stream, held by one function in one crate.
