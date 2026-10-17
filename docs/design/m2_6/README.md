# M2.6: alarm view mini-iteration

Status: approved on 2026-10-17, in progress (phase 2 PR1). A mini-iteration in
the M0.5/M1.5/M2.5 tradition: it deepens the view layer and nothing else,
so M2.5's one-module discipline and M3's scope are untouched. The
decisions it needs before implementation starts are listed at the end.
The layout follows [docs/design/README.md](../README.md).

## Goal

M2 taught the plant to misbehave and put every alarm on Modbus, MQTT, REST,
the WebSocket stream and Prometheus. The browser build saw none of it:
someone opening the viewer today sees a red diode blink on a container and
nothing else. There is no list of what is active, no history of what
raised and cleared, no severity, and no way to start one of the fifteen
library cases. The alarm story the emulator exists to tell is only
readable through the Docker sandbox.

After M2.6 a person opening the viewer in a browser can pick a library
case from a list inside the viewer and start it, see which alarms are
active on which node and how severe they are, follow the events as they
raise and clear, and connect each of them to what the 3D scene shows at
that moment. The first consumer outside this repository is a public study
page on the emulator's alarm system, which embeds the viewer and needs to
start a case from the page.

## What the browser build can and cannot do today

Pinned on 2026-10-17 against `develop` after PR #26 (v0.5.0 plus the npm
licence fix). Read, not assumed:

- **One module, one canvas.** `bess-wasm` compiles `bess-core`,
  `bess-models` and `bess-scene` into one WASM module (4.47 MB as shipped
  in the npm package). JS calls `WebHandle::start(canvas)` and nothing
  else; the scene and the panels read the state tree from Rust memory.
  The host page cannot read plant data and does not want to.
- **The viewer owns the loop.** `ViewerApp` (feature `sim`) holds a
  `Simulation`, steps it against wall time at 60x, and runs jumps on a
  per-frame tick budget (`viewer/jump.rs`). Its commands are speed,
  external setpoint, restart on a date, fast-forward to a date.
- **Alarms reach the view as one bit.** `instances/live.rs` reads
  `rack.alarm_bits != 0` per container and blinks one red LED. The block
  word, the site word, the severity byte and the event stream are read by
  nothing in the view. `Simulation::step` returns the tick's events and
  the viewer drops them. The detail panel shows the PCS state and no
  alarm name.
- **The spelling of a node lives in the native shell.** `node_path`,
  `alarm_name`, `fault_path` and the severity names are in
  `bess-emulator/src/events.rs`, which the scene cannot depend on (the
  shell is tokio and sockets). The names themselves are in
  `bess_core::alarms::layout`.
- **The scenario engine is one dependency edge away.** `bess-scenario`
  is pure, compiles for `wasm32-unknown-unknown` and CI checks that on
  every change (M2 phase 4 PR2), but neither `bess-scene` nor `bess-wasm`
  depends on it. The loop that fires what is due before a tick exists
  three times natively (`sim/scenario.rs`, `headless.rs`, the test helper
  `tests/support/play.rs`) and not at all in the viewer.
- **No protocols, no file system.** The seven data faults corrupt MQTT
  and Modbus through a tap on the native read paths; the browser has
  neither surface, so a data fault has nothing to act on there, exactly
  as REST and the stream stay truthful by design. A scenario file can
  only reach the browser embedded in the module or handed in through JS.
- **Single-threaded.** A long catch-up has to be split across frames or
  the canvas freezes, which M1.5 already solved for jumps.

## Phases

| Phase | Theme | Depends on |
|---|---|---|
| [1: Alarms on screen](phase-1-alarms-on-screen/) | alarm summary and journal in the panel, severity-coded scene, click-through both ways, the operator reset from the viewer; one spelling of a node shared with the native surfaces | nothing |
| [2: The library in the browser](phase-2-library-in-the-browser/) | the scenario library embedded, a picker and a progress strip in the viewer, one run loop that fires what is due during live stepping and jumps, a JS door for the host page, the release | 1 (a case without a visible alarm list is a timer and a blinking diode) |

Phases land in order. A phase is done when its PRs are merged, its tests
are green in CI, and its documents record what was actually built.

## Gate

No calibration gate, for the M1.5 reason: the iteration adds no physics,
and a view iteration that needed one would mean the scene had started
deciding something. What replaces it:

- **Digest equality.** A library case played through the viewer's own
  run loop, with deliberately awkward per-frame budgets, must leave the
  tree and the event log at the digest the headless run committed in
  `scenarios/`. The browser plays the same story as CI, byte for byte.
- **The kernel-side gates as boundary tests.** The golden determinism
  digest, the signal map diff, the annual record and the library's
  records all have to come back unmoved. One naming helper moves into
  `bess-core` in phase 1; it changes no behavior, and those gates are
  what proves it.
- **By eye, on a running viewer.** M1.5's standing lesson: every test
  asks whether a number is right, none can ask whether anything is
  visible. The maintainer looks at it before a PR merges.

## Versioning

- **Crate:** v0.6.0 at iteration end if M2.6 ships before M2.5
  (recommended, see the open questions), otherwise the next minor after
  it. A mini-iteration with source-level breaks takes a minor bump, the
  M0.5 and M1.5 precedent; this one has at least two (below).
- **Signal map:** unchanged at 0.3.0. The view is not a published
  surface, and CI diffs the map on every pull request to keep that true.
- **Checkpoint:** unchanged at format 8. The alarm journal is view state;
  the scenario identity a run carries is already in the tree since M2
  phase 4.
- **Browser module API (npm `bess-emulator`):** `WebHandle` gains methods
  in phase 2. Additions only; `start` and `destroy` keep their signatures.
  COMPATIBILITY.md gains a short section stating the rules for this
  surface, which it has not named so far.
- **Source-level breaks, pre-1.0 and called out:** `ViewerCommand` gains
  variants (a downstream `match` stops being exhaustive); `bess-scene`
  gains `bess-scenario` as a dependency behind `sim`; `FastForward`
  takes the viewer's run rather than a bare `Simulation`;
  `bess-emulator`'s `events.rs` helpers become thin calls into
  `bess-core`, and a `bess-scenario` helper replaces three hand-written
  copies of the fire-then-step loop.
- **Dependencies:** none proposed. Each candidate considered is named in
  the phase designs with its rationale and the alternative taken, per the
  repository's rule that a new crate enters only with the maintainer's
  explicit yes.

## Release-note inventory for the release

Filled as PRs land.

- **Phase 1 PR1, the alarm section.** The viewer's side panel gains an
  alarm summary (every set bit of the three words, trips first, then in
  tree order, with the time it raised) and an alarm journal (the events
  the viewer saw, newest first, with the kernel's `seq` and `ts`), with
  filters by severity and block and an "alarms only" toggle. Collapsed,
  the section header says how many events arrived since it was last
  open. The whole side panel now scrolls.
- **Phase 1 PR1, one spelling of a node.** `AlarmNode::path`,
  `AlarmNode::word`, `AlarmNode::alarm_name`, `Severity::name`,
  `PcsOpState::name` and `FaultAction::node_path` in `bess-core`;
  `bess-emulator`'s `events.rs` helpers call them. No surface changes a
  byte.
- **Phase 1 PR1, source-level breaks in `bess-scene`:** `PanelInput`
  gains `journal`, `PanelState` gains `alarm_filter` and `alarms_seen`,
  `Palette` gains `warning` and `out_of_service`, and
  `FastForward::advance` takes the journal. Code that builds these by
  struct literal or calls `advance` needs the new argument.
- **Phase 1 PR2, severity in the scene.** The container diode shows the
  loudest tone of its racks and no longer blinks; a PCS skid diode shows
  the block word, a beacon on the transformer the site word, and a failed
  HVAC unit carries an amber mark. Trips red, warnings amber, a rack
  isolated or a block in maintenance muted blue-grey.
- **Phase 1 PR2, rows and objects linked, and the reset.** A node in the
  summary or the journal selects its object in the scene; the selected
  object lists its alarms. The operator reset is in the viewer: site in
  the alarm section, block on the PCS skid, rack on its row, each
  enabled only while something in scope is latched, with the answer
  shown beside the site button.
- **Phase 1 PR2, source-level breaks in `bess-scene`:** `ViewerCommand`
  gains `ResetAlarms(ResetScope)`; `PanelState` gains `select_request`
  and `reset_answer`; `selection_detail` takes the command list.
- **Phase 2 PR1, the library in the viewer.** All fifteen cases are
  compiled into the module. The side panel lists them by taxonomy and
  starts one with a click. While a case plays, a strip shows its
  headline, the elapsed offset against its length, what has fired and
  what is next, on a painted timeline, with run-to-next-event and
  run-to-end. Data faults are listed as acting on MQTT and Modbus only.
  Alarm times read as offsets from the case's start. Every case up to
  two days, played through the viewer's loop, lands on its committed
  digest.
- **Phase 2 PR1, one firing helper.** `Player::fire_due` in
  `bess-scenario` replaces the three hand-written fire-then-step loops
  (shell, headless run, test helper). Nothing on the wire changes.
- **Phase 2 PR1, module size:** `bess_wasm_bg.wasm` 4.54 MB to 5.47 MB
  raw, 1.59 MB to 1.82 MB brotli. Above the half-megabyte threshold of
  phase 2 D1, so the maintainer decides.
- **Phase 2 PR1, source-level breaks in `bess-scene`:**
  - `ViewerCommand` gains `StartScenario`, `RunToNextEvent` and `RunToEnd`.
  - `FastForward::advance` takes a `Run`.
  - `PanelInput` gains `library` and `story`.
  - `alarms::section`, `controls::time_travel` and `controls::stops` take
    the story state.
  - `ViewerApp`'s internals moved into `Session`.
  - `bess-scenario` becomes a dependency behind `sim`.
- **Phase 2 PR1, build:** the Dockerfile copies `scenarios/`. CI runs the
  viewer's tests, which it never did.

## Open questions, and the decisions needed before implementation

Approved by the maintainer on 2026-10-17, each as recommended below:
M2.6 ships before M2.5 as v0.6.0, no new dependency, no
acknowledgement concept in the view, the library and the parser
embedded with the size measured, the four-method JS door, the headline
from the header comment. Each is still confirmed or revised in the PR
that lands it, per the repository's rule.

The detailed options and recommendations are in the phase design files;
this is the list of answers the work cannot start without.

1. **Placement and order.** M2.6 is independent of M2.5 (no kernel model
   changes, no shared files beyond ROADMAP.md). Ship it first as v0.6.0,
   with M2.5 moving to v0.7.0 and a one-line edit in its README, or
   after M2.5 as v0.7.0. Recommended: first. It is small, carries no
   calibration gate, and the study page that embeds the viewer is
   waiting on it. The number 2.6 names its place between M2 and M3, not
   its shipping order, the same way M0.5 was pulled forward from M3.
2. **No new dependency.** Phase 1 D10 declines `egui_extras` (tables),
   `egui_plot` (a timeline) and `egui_kittest` (UI snapshot tests) and
   says what is used instead; phase 2 D10 declines `serde_wasm_bindgen`.
   Confirm, or name the one to take.
3. **Acknowledgement.** Phase 1 D6 proposes no acknowledgement concept in
   the view (a steady indication, an "events since you last looked"
   count, no blinking), because the kernel has none and a view-side one
   would be a fiction. The alternatives are a view-side "seen" state or a
   kernel acknowledge bit, which is a signal-map change and out of this
   iteration.
4. **Embedding the library.** Phase 2 D1 embeds the fifteen YAML files
   and the YAML parser in the WASM module, with the size cost measured
   as an acceptance criterion. The alternative keeps the parser out of
   the browser by generating a Rust table from the files, and loses the
   ability to load a custom file from the host page.
5. **The JS door's shape.** Phase 2 D6 proposes four methods on
   `WebHandle` (`scenarios`, `start_scenario`, `load_scenario_yaml`,
   `scenario_status`), JSON strings in and out, the status in the exact
   shape `GET /api/v1/scenario` already publishes. Confirm the set; in
   particular whether a status door is wanted at all, since the host page
   could keep to the control methods.
6. **A case's one-line description.** Phase 2 D7 reads it from the file's
   header comment, which the library contract already requires, rather
   than adding a `description` field to schema 1. The field would be the
   cleaner source and a file-format change; the comment costs nothing.
