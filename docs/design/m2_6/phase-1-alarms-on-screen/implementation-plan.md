# M2.6 phase 1: alarms on screen, implementation plan

Two PRs, each landing green.

## PR1: the alarm model and the panel

- `bess-core`: `alarms/names.rs` with `AlarmNode::path()`,
  `FaultAction::node_path()`, `Severity::name()` (design D2);
  `bess-emulator/src/events.rs` calls through; a parity test written
  first against the strings as published today.
- `bess-scene`: `alarms.rs` (the active walk, sort order, severity
  palette lookup) and `alarms/journal.rs` (the ring), both without the
  `sim` feature (D3, D4).
- `bess-scene`: `panels/alarms.rs`, the summary and the journal with
  their filters, the "events since you last looked" count (D1, D5, D6a).
  `side_panel` gains the section between the weather rows and the
  controls; `PanelInput` gains the journal.
- `bess-scene/src/viewer.rs`: records each tick's events into the
  journal with the tick's time and the kernel's numbering, in both the
  wall-time path and the jump path; clears it on restart.
- `Palette` gains `warning` and `out_of_service` (D7); the colours are
  used by the panel in this PR and by the scene in PR2.
- Tests: the walk, the names parity, the journal, the summary-against-
  journal property.

Accept: with the phase 2 chain fixture (both HVAC units of one block
failed on the hot afternoon) running in the native viewer example, the
summary shows `block.hvac_failure` at once, then `rack.derate_active`
rows, `block.setpoint_not_met` and `site.power_limited` appear in that
order and minutes apart, the journal numbers them consecutively, and the
golden digest, the annual record and the library's records are unmoved.

## PR2: the scene and the reset

- `instances/live.rs`: the container diode coded by the worst rack
  severity and no longer blinking, the HVAC failure mark, the PCS skid
  diode for the block word, the transformer marker for the site word
  (D7). Instance-stream tests extended.
- Two-way linking: row click sets the selection; the detail panel lists
  the alarms of the selected container's racks, PCS skid or transformer
  (D8).
- `ViewerCommand::ResetAlarms(ResetScope)` (D9, D11): the three buttons,
  enabled on a latched bit in scope, the "still present" answer shown.
- Release-note inventory entries in the milestone README; design.md and
  architecture.md updated as built.

Accept: in the native viewer example with the chain fixture, the hot
block's containers read amber, its skid reads amber once the setpoint
bit raises, the transformer marker reads amber once the site bit raises;
a PCS trip through `Simulation::apply` turns its skid red, the block
reset button clears it and the journal shows the PCS leaving fault; the
kernel-side gates are unmoved.

## Open questions

- Carried from design.md: camera focus on row click, journal capacity.
