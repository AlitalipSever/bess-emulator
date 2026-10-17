# M2.6 phase 1: alarms on screen, design

Each decision is proposed here and confirmed or revised in its PR.

## What the operator tools do

Checked before deciding how the panel should look, so the choices below
lean on practice rather than taste. ISA-18.2 (and IEC 62682, its
international counterpart) describe two displays every alarm system
offers: an **alarm summary**, the list of alarms currently in alarm, and
an **alarm journal** or history, the time-ordered record of alarm state
changes. HMI products (AVEVA Plant SCADA, Rockwell View Designer,
Inductive Automation's alarm status table) default the summary to the
columns priority, time of the last state change, source and condition,
sorted by state, then priority, then time, newest first. EEMUA 191 and
ISA-101 reserve colour for abnormal conditions on an otherwise grey
display, with red for the highest priority and yellow or amber for the
next, and reserve flashing for alarms not yet acknowledged. The emulator
has two severities by construction (the low byte warns and clears itself,
the high byte trips and latches), so the mapping is direct: trips red,
warnings amber, and the status bits that are neither (isolated,
maintenance) in a muted out-of-service tone.

## Decisions

- **D1, a summary and a journal, in the standards' shape.** Two tables
  in one collapsible panel section. The summary lists every set bit on
  site: severity, node, alarm name, time raised (from the journal when
  it has the raise, otherwise "before the journal"). Sorted by severity
  (trips first), then node in tree order, so the list reads top-down
  the way the plant is built. The journal lists events newest first:
  `seq`, time, node, what happened (`raised`, `cleared`, `pcs_state`,
  `fault`), with the alarm or fault name. Filters: by severity, by
  block, and "alarms only" to hide PCS transitions and applied faults.
  The alternative, one merged list of "recent alarms", is what the
  blinking diode already is; the standards split present from past
  because an operator asks the two questions separately.

  **Confirmed in PR1.** One section, `panels/alarms.rs`, between the
  weather rows and the controls, open by default. The summary's columns
  are severity, node, name and raised time; the journal's are `seq`,
  time, node, kind and what, where a PCS transition reads
  `run -> fault` (ASCII: egui's bundled fonts have no arrow glyph, which
  is also why the meters row now says export and import). Rows are laid
  out through `ScrollArea::show_rows`, eight visible before a table
  scrolls; the side panel opens at 380 px instead of 330. The side panel as a whole now
  scrolls, because the two tables made it taller than a laptop screen.

- **D2, the spelling of a node moves to `bess-core`.** `AlarmNode::path()`
  (`site`, `block02`, `block02/container1/rack05`),
  `FaultAction::node_path()` (an HVAC unit as
  `block02/container0/hvac`), `Severity::name()`. `events.rs` keeps its
  public functions as one-line calls so the shell's call sites do not
  churn. Options: duplicate the strings in `bess-scene` (two spellings
  of one node, and COMPATIBILITY.md promises one); make `bess-scene`
  depend on `bess-emulator` (the shell carries tokio, axum, sockets, and
  does not build for wasm32); move them (recommended, a move of three
  functions, no behavior change, held by the shell's existing payload
  tests).

  **Confirmed in PR1, two additions.** The move also carries
  `AlarmNode::word()` and `AlarmNode::alarm_name(bit)`, so the `rack.bit11`
  fallback for a free position lives in one place too, and
  `PcsOpState::name()`, since a journal row spells a PCS transition the
  way the payload does. The impls sit in a private `alarms/names.rs`.
  `bess-core` tests pin every node kind and every `FaultAction` variant;
  the shell's payload tests and the MQTT acceptance test pass unchanged.

- **D3, the summary is derived from the words every frame.** Rationale
  in architecture.md. The walk is a pure function `active(&SiteState)
  -> Vec<ActiveAlarm>`, tested against a hand-set tree, so it cannot
  miss a word the way the diode today misses two of three. Cost: 480
  rack words plus 21 more per frame, a few microseconds; the dynamic
  instance pass already walks the same racks.

  **Confirmed in PR1.** `alarms::active` walks the words; `Tone`
  (`OutOfService`, `Warning`, `Trip`) says how a set bit is drawn, with
  `rack.isolated` and `block.maintenance` muted and `site.hv_breaker_open`
  promoted to the trip tone. The text column still shows the kernel's
  severity, so the tone changes a colour and never a published word.

- **D4, the journal is a bounded ring, view state only.** Capacity
  4,096 entries, one per event, each a `Copy` event plus two integers.
  A clean replayed day emits a few hundred events (the native shell's
  `EVENT_BACKLOG` is sized on the same figure); the longest library
  case, ten days, emits a few thousand; the chatter storm never reaches
  the log, since it is a data fault. Cleared on restart and on scenario
  load, because the numbering starts over with the plant. Alternatives:
  an unbounded `Vec` (a browser tab left open at 3600x grows without
  limit); the kernel keeping the log (rejected in M2 phase 2 D5 for the
  checkpoint's sake, and nothing has changed).

  **Confirmed in PR1, capacity measured.** The longest library case,
  `hvac-failure-hot-week`, emits 1,717 events in ten days and
  `hvac-failure-winter-night` 1,686 in one; 4,096 holds every case whole.
  Beside the ring the journal keeps the raise time of every bit whose
  last edge was a raise, so a summary row keeps its time after the ring
  has dropped the raise. Both ways the viewer moves the plant (wall time
  and a jump) step through `viewer/record.rs`, which copies the events
  out of the kernel's buffer without allocating.

- **D5, timestamps are the tick's simulation time.** The same `ts` MQTT
  and the stream carry: the unix time of the snapshot that shows the
  event's effect, `sim.unix_time_s()` after `step`. Shown as the UTC
  clock the panel header already uses, and, while a scenario plays
  (phase 2), also as an offset from scenario start, since the library's
  files speak in offsets.

  **Confirmed in PR1.** Rows read `MM-DD HH:MM:SS` (`clock::format_row`);
  the year is in the panel header. The scenario offset waits for phase 2.

- **D6, no acknowledgement.** The kernel has no acknowledge concept:
  latched bits reset, hysteresis bits clear themselves, and the words
  publish no acknowledged flag. Three options:
  (a) none in the view: steady colours, and the panel header shows the
  number of events since the panel was last expanded, read from
  `event_log.count`; (b) a view-side "seen" state, flashing until the
  row is clicked, held only in the browser tab; (c) a kernel acknowledge
  bit per word, raising an acknowledged event, published on every
  surface. Recommended: (a). Option (b) would make the scene flash for
  a state no surface can see and no reproduction can carry, a fiction
  the emulator otherwise refuses; option (c) is a real alarm-system
  feature (ISA-18.2's acknowledge is an operator action the system
  records) and a signal-map addition, and belongs to an iteration that
  gives the surfaces the same button. Logged in the milestone README as
  a question for the maintainer, since it decides what the scene does
  with motion.

  **Confirmed in PR1, option (a), as approved.** A `SeenMark` holds the
  event count when the section was last open; collapsed, the section
  shows "N events since you last looked". The viewer resets the mark
  with the plant.

- **D7, the scene's severity mapping** is the table in architecture.md.
  Two new indications: a diode on the PCS skid for the block word and a
  marker on the transformer for the site word, both cuboid instances in
  the existing pipeline, no new draw path (the M1.5 D3 rule). `Palette`
  gains `warning` (amber) and `out_of_service` (a muted blue-grey); the
  existing `alarm` red stays for trips. The container diode stops
  blinking (architecture.md, "nothing blinks"). Alternative: keep one
  red for everything, which is what today's diode does and what hides
  the severity byte the layout went to the trouble of freezing.

  **Palette half in PR1:** `warning` is amber `[1.00, 0.76, 0.03]`,
  yellower than the discharge orange and redder than the selection
  yellow; `out_of_service` is `[0.47, 0.55, 0.66]`. The panel uses them
  now, the scene in PR2.

  **Scene half in PR2.** The indications live in a new
  `instances/status.rs`, because `live.rs` was at 295 lines and alarms
  are a concern of their own. A quiet container diode echoes its gauge
  (flow colour while power moves, otherwise neutral) at a steady glow; an
  alarmed one takes the loudest tone of its racks. The HVAC mark is an
  amber strip across the top of the end unit's louvres, the PCS diode
  sits above the skid's grille, the site beacon on the transformer roof.
  Seen in the browser build on 2026-10-17 with the plant discharged at
  +100 MW: twenty amber skid diodes and an amber beacon at once.

- **D8, two-way linking.** A summary or journal row is a button; it sets
  `SceneView::selection` to the object that holds the node (rack to its
  container, block to its PCS skid, site and a protection or breaker
  event to the transformer). The detail panel, in turn, lists the alarms
  of the selected object: a container's rack rows gain an alarms column
  with the set bit names, a PCS shows the block word's names, the
  transformer shows the site word's. The camera stays where it is (open
  question below).

  **Confirmed in PR2.** Node cells in both tables are links;
  `alarms/link.rs` maps a node or an event to the object
  (`object_of`, `object_of_event`). The panel cannot reach the scene, so
  a click leaves a request in `PanelState::select_request` and the viewer
  moves it into `SceneView::selection`. The detail panel's rack table
  gains an alarms column (short names in the loudest tone) and a reset
  column, the PCS a "Block alarms" row, the transformer a "Site alarms"
  row, and a failed HVAC reads "failed" in the container's HVAC row.

- **D9, the operator reset from the viewer.**
  `ViewerCommand::ResetAlarms(ResetScope)`: a site reset button in the
  summary header, a block reset on a selected PCS skid, a rack reset on a
  selected container's rack row, each enabled only while the scope has a
  latched bit. The viewer calls `Simulation::reset_alarms` and shows the
  returned "still present" list under the button until the next tick.
  Alternative: no reset in the viewer and let the scenarios carry their
  own (`pcs-trip-and-reset` does); then a viewer that trips the site by
  running a case has no way back but a restart, which is not what the
  HMI button is for.

  **Confirmed in PR2, one revision.** Buttons are enabled by
  `alarms::latched(state, scope)`: a trip bit in the scope's words, which
  covers a PCS in fault through its mirror bit. The answer is kept until
  the next reset or restart instead of until the next tick, because at
  60x a tick lasts 16 ms and the answer would never be seen; it reads
  "block03 reset at 07-14 14:03:21, nothing still present" or names the
  bits that raise again.

- **D10, no new dependency (needs the maintainer's confirmation).**
  Three candidates were considered on 2026-10-17 and declined:

  | Crate | What it would give | Why not here | Used instead |
  |---|---|---|---|
  | `egui_extras` 0.35 (same org and version line as egui) | `TableBuilder`: resizable columns, a fixed header, virtualized rows | a second UI crate for two tables whose columns do not need resizing; it pulls optional image and syntax-highlighting features that have to be turned off; one more crate to keep in step with each egui bump | `egui::Grid` inside `egui::ScrollArea::show_rows`, which virtualizes rows (only the visible range is laid out) and is in egui already; the header is a first striped row |
  | `egui_plot` 0.35 | a plot widget for a timeline of events | the timeline wanted here is a strip of ticks at times, not axes with zoom; a plot crate for that is weight without a reader | `ui.painter()` on an allocated rect, the way the scene's mode overlay already draws |
  | `egui_kittest` 0.35 | UI snapshot and interaction tests | its snapshot path needs a GPU backend in CI (wgpu), which the workspace deliberately does not carry; its logic path tests egui widgets, not this crate's claims | the existing pattern: every claim lives in a pure function with a test (`alarms.rs`, `journal.rs`), the panel only draws its result |

  **Confirmed in PR1, as approved.** No crate added. `bess-models`
  becomes a dev dependency of `bess-scene` (a workspace crate, already an
  optional dependency behind `sim`) so `tests/alarm_view.rs` can hold the
  view against the chain the kernel produces.

- **D11, `ViewerCommand` grows by one variant** (`ResetAlarms`), a
  source-level break for a downstream exhaustive `match`, called out in
  the release-note inventory as M1.5's two variants were.
  **Confirmed in PR2.** Two more signature changes came with it:
  `panels::detail::selection_detail` and `panels::alarms::section` take
  the command list, since the resets live there.

## Signal map impact

None. CI diffs the map on every pull request.

## Checkpoint impact

None. The journal is view state; no field is added to the tree.

## Determinism and the public surfaces

- The view only reads the tree; the reset uses the kernel's existing
  door. The golden determinism digest, the annual record and the fifteen
  library records must come back unmoved, and the PR says so.
- MQTT topics, stream payloads and the REST reset answer do not change a
  byte: the spelling moves crates, the strings do not. The shell's
  payload tests and the MQTT acceptance test hold that.
- The native viewer example gains the same panel for free; nothing else
  native changes.

## Test plan

- **Unit, the walk:** `alarms::active` on a hand-set tree with one bit
  on a rack, one on a block, one on the site, one trip and one warning,
  returns five entries in the documented order with the documented names;
  an empty plant returns none.
- **Unit, the names:** in `bess-emulator`, the old `node_path`,
  `alarm_name`, `fault_path` and `severity_name` outputs equal the
  `bess-core` ones for every node kind and every `FaultAction` variant
  (the test is written against the strings as published today, then the
  move lands, then it still passes).
- **Unit, the journal:** append past capacity, the oldest entries go,
  the numbering stays the kernel's, newest-first iteration, the unseen
  count resets when read.
- **Property, summary against journal:** over a scripted hot plant (the
  phase 2 chain fixture) stepped for an hour, every bit the summary shows
  has a raise in the journal and no clear after it. This is M2 phase 2's
  first invariant seen from the view side.
- **Instances:** an alarmed container emits a diode in the warning colour
  for a warning-only word and the trip colour once a trip bit is set; a
  block in fault emits the skid diode; a failed HVAC emits its mark. The
  well-formedness test over the instance stream is extended rather than
  duplicated.
- **Boundary gates:** golden digest, `bess-bench --check`,
  `scripts/assert-library.sh`, the signal map diff, all unmoved.
- **By eye:** the maintainer opens the native viewer example and the
  browser build with the chain fixture or (after phase 2) a library case,
  and confirms the three indications read at a glance.

## Open questions

- Whether a row click should also move the camera to the object (a
  `Camera::focus_on(center)` the camera does not have). Leaning no for
  this phase: the frame and the orbit find it, and a camera that jumps
  on every click fights the person steering it.
- The journal's capacity: answered in PR1 (D4), 4,096 against a
  measured maximum of 1,717.
