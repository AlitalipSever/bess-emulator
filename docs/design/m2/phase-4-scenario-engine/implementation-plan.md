# M2 phase 4: scenario engine, implementation plan

Two PRs, each landing green. Pinned on 2026-10-14 against the tree as
phase 3 left it (checkpoint format 7, map 0.3.0, `kernel.rs` at 296
lines, `sim.rs` at 193, `http.rs` at 220, `cli.rs` at 52, `balance.rs` at
281, `Event` with three variants, no YAML dependency in the workspace).
Design D1 to D12, each proposed there and confirmed or revised in the PR
that lands it.

Before the first commit of PR1: Ali's yes on the YAML parser (design D9),
because `cargo add` is the install the library rule guards. Everything
else in PR1 can be written and tested with the parser absent (the schema
types are plain serde), so the yes gates one commit, not the PR.

## PR1: the crate and the kernel door

What the PR does, in one paragraph: the kernel gains one enum that names
every physical fault it accepts and one method that applies any of them;
the one fault no earlier phase built, a rack that leaks, becomes a field
and a term; a new crate turns a YAML file into a schedule and plays it;
and the determinism contract is extended over both, so that a file plus
a seed is a reproduction in fact and not in prose.

### The kernel (D3, D8, D11)

- `bess-core/src/kernel/event.rs`, new: `Event` moves here from
  `kernel.rs` unchanged, gains `FaultApplied(FaultAction)`, and is
  re-exported from `kernel` so no caller notices the move. `kernel.rs` is
  at 296 lines; the variant would take it past the soft limit, and "what
  a tick reports" is a concern of its own.
- `bess-core/src/kernel/fault.rs`, new: `FaultAction` (the ten variants
  of design D3, `Copy, Eq, Hash, Serialize, Deserialize`), `FaultError`
  (`Command(CommandError)`, `Reset(ResetError)`, both `#[from]`), and
  `impl Simulation { pub fn apply(&mut self, action: FaultAction) ->
  Result<bool, FaultError> }`. Each arm calls the existing command and
  maps its answer to the bool; `RackSelfDischarge` sets the rack field
  from the integer rate (`excess_ppm_per_day as f64 * 1.0e-6 / 86_400.0`)
  and answers `true` when the rate changed. The node check runs first,
  so a refused action changes nothing and leaves no mark; on an accepted
  one, `apply` pushes `FaultApplied` to `pending_events` and then calls
  the command, whose own pending events (a trip's raise, the PCS
  transitions) land behind it, so the log reads cause, then effect, on
  every tick, as the architecture says. Unit tests beside it: one per
  variant against the direct command (tree equality plus the extra
  event, in that order), the no-op answers, the refusals.
- `bess-core/src/state/plant.rs`: `RackState::self_discharge_excess_per_s:
  f64`, doc comment stating it is zero on a healthy rack and set only by
  a fault. `state/init.rs` initialises it to zero.
- `bess-core/src/state.rs`: `ScenarioId { name: String, content_hash:
  u64 }` (`Clone, PartialEq, Eq, Serialize, Deserialize`) and
  `SiteMeta::scenario: Option<ScenarioId>`; `init.rs` sets `None`.
  `SiteState::new` keeps its signature: the shell or the crate sets the
  field on the tree before `Simulation::from_state`, as the fixtures
  already prepare trees.
- `bess-core/src/digest.rs`, new: `pub fn fnv1a_64(bytes: &[u8]) -> u64`.
  `checkpoint.rs` and `alarms/log.rs` call it instead of carrying their
  own copies; no digest value moves (same function, same bytes). The
  event log's fold stays where it is, it only stops restating the
  constants.
- `bess-core/src/checkpoint.rs`: `FORMAT_VERSION` 7 to 8 with its line
  in the format comment; the version tests move with it by construction.
- `bess-emulator/src/events.rs`: the `topic` and `payload` arms for
  `FaultApplied` (design D11), with a test each, in this PR because the
  match is exhaustive and the workspace must build at every commit. The
  REST status surface waits for PR2.
- Struct literals of `RackState` gain the field: `state/init.rs`,
  `bms.rs`, `bms/alarms.rs`, `bms/balance.rs`, `cell.rs` and `thermal.rs`
  tests (the files `grep -l balancing_active:` lists today). One line
  each.

### The leak (D3)

- `bess-models/src/bms/balance.rs`: the time term becomes
  `(self.growth_per_s + rack.self_discharge_excess_per_s) * dt_s`. One
  line of physics, applied whether or not the rack is in service, since
  a cell leaks either way. The file is at 281 lines; its tests stay
  where they are and the fault's tests go to
  `bess-models/tests/self_discharge.rs` (public API suffices), which is
  the AGENTS.md answer when tests are what would push a file over.
- `bess-models/tests/self_discharge.rs`, new: the three unit cases of
  the design's test plan, with the arithmetic pinned in the file. The
  healthy string drifts 1.8 %/month (CALIBRATION.md, cell spread); the
  bleed narrows at most 0.24 A over 314 Ah, about 1.8 % of SoC per day
  while it runs, and it runs only at the top of the window, a few hours
  on a plan day, so a rack leaking at ten times the healthy rate
  (18 %/month, 0.6 % of SoC per day) outruns it and crosses the 120 mV
  warning within weeks while its neighbours stay near 30 mV. The test
  picks a rate in that range and holds a day count with margin; the
  exact day is measured in the PR and recorded in CALIBRATION.md, not
  tuned.
- CALIBRATION.md: a short section "M2 (v0.5.0): the rack self-discharge
  fault" after the real-prices section: the healthy baseline it adds
  to, the range the library will use (phase 5 chooses the cases), the
  measured days-to-warning at the test's rate, and the statement that
  the rate is an estimate labeled as such (a soft internal short is
  described in the literature as tens of times a healthy cell's leak,
  and no public dataset pins a figure for a 314 Ah LFP cell).

### The crate (D1, D2, D4, D9, D10, D12)

`crates/bess-scenario/`, workspace member, `version.workspace`,
`[lints] workspace = true`, dependencies `bess-core`, `bess-data`,
`serde`, `thiserror`, and the YAML parser; dev-dependencies
`bess-models` and `serde_json` (the twin test and the goldens). Files,
one concern each, with their line budgets:

- `src/lib.rs` (under 100): crate doc, module declarations, re-exports,
  and `pub fn load(bytes: &[u8], cfg: &PlantConfig) -> Result<Scenario,
  LoadError>`: parse, validate, hash. Nothing else.
- `src/error.rs` (about 120): `LoadError { position: Position, reason:
  Reason }` with `Position::Line { line, column }` for the parser's
  errors and `Position::Event { index, field }` for validation; `Display`
  renders `line 7, column 5: ...` or `events[3].target: ...`. `Reason`
  is an enum with one variant per rejection the test plan lists, so a
  test asserts the variant and a human reads the text.
- `src/schema.rs` (about 200): the serde types exactly as the file
  spells them, `deny_unknown_fields` on every struct, `taxonomy` and
  `surface` as enums, `at` and `duration` as strings here (parsed in the
  next step so a bad duration reports its field). `EventSpec` is one
  struct with optional `target` and `surface` and the optional
  parameters; the one-of rule and the matrices are validation, not
  serde, because serde's untagged enums report "did not match any
  variant" and that is not a message a scenario author can act on.
- `src/schema/duration.rs` (about 150 with tests): `parse(&str) ->
  Result<u64, DurationError>` for `PnW`, `PnDTnHnMnS` and subsets, whole
  seconds. Tests from the design's list.
- `src/schema/target.rs` (about 250 with tests): `Target` enum (the five
  kinds of design D2 with their indices), `parse(&str)`, `validate(&self,
  &PlantConfig) -> Result<(), TargetError>` with the range text, and
  `node(&self) -> AlarmNode` for the kinds that have a word. Tests: every
  row, every refusal.
- `src/validate.rs` (about 250): the schema version, the one-of rule,
  the (kind, fault) table of D3, the (surface, fault) matrix of D4, the
  parameter presence and ranges, the per-target order and no-shared-tick
  rule of D12, `speed` and `start`. Produces `Scenario`: the file's
  identity (name, taxonomy, content hash), its stated seed, start and
  speed as `Option`s, and the events typed (`Physical { tick, action:
  PendingAction }`, `Data { tick, surface, fault, duration, params }`)
  but not yet drawn.
- `src/resolve.rs` (about 200): `RunParams { seed, start_unix_s, speed }`
  and `Scenario::resolve(self, RunParams) -> Resolved`: draws the ranged
  parameters from the salted `Rng` stream (D12), sorts by (tick, index),
  and fixes every `FaultAction`. `Resolved` also carries `ScenarioId`
  and offers `simulation(&self, cfg, models) -> Simulation` (a fresh tree
  with `meta.scenario` set) and `resume(&self, SiteState) ->
  Result<Simulation, ResumeError>` (D8). The precedence rule of D5 is
  not here: it is the shell's, because it is about flags.
- `src/player.rs` (about 150): `Player::new(&Resolved)`, `due(&mut self,
  tick) -> &[Scheduled]` (advances the cursor, returns the slice due at
  exactly this tick), `position(&self) -> Position { fired, pending,
  next }`, `catch_up(tick)` for a resumed run. No reference to any
  simulation type: the shell matches on the action and calls `apply`.
- `tests/goldens.rs`: the design's example parses to the expected
  `Resolved` (seeded draw included, value pinned); an empty `events`
  list loads.
- `tests/rejections.rs`: every rejection of the test plan, asserting the
  `Reason` variant and the `Position`.
- `tests/determinism.rs`: same tuple twice; checkpoint round-trip
  mid-scenario; resume refused under another file and under none; idle
  cost (no events) against the plain run, tree compared with
  `meta.scenario` set aside, event-log digests equal.
- `tests/twin.rs`: the acceptance (below).
- `tests/fixtures/`: the design's example as `hvac-failure-hot-week.yaml`,
  the twin's `hot-afternoon.yaml`, an `empty.yaml`. Test material, not
  the library; the library is phase 5's and lives at `scenarios/`.

### Accept

`bess-scenario/tests/twin.rs`: the phase 2 causal-chain plant
(`alarm_chain.rs`'s prepared tree: July afternoon, block 2's racks at
54 C, full discharge requested), once with the two HVAC failures applied
through `Simulation::apply` before the first tick, once with
`hot-afternoon.yaml` (`at: PT0S` on both units) played by `Player`,
3,600 ticks each: identical event logs (count and digest), and with the
two `FaultApplied` entries dropped, the log `alarm_chain.rs` records
today, so the chain's four raises and their spacing still hold under the
engine.

### Called out in the PR

The golden digest moves once, for schema (the two fields), and the
constant's comment says so; the annual record does not move and `--check`
passes without regeneration. `Event` has a fourth variant: exhaustive
matches outside the workspace gain an arm. Checkpoint format 8. The YAML
parser's rationale and Ali's yes, quoted.

### Not in PR1

Any CLI or REST door; the player running anywhere but tests; data-fault
execution; the library; a repair action for a leaking rack (M5).

### As landed (2026-10-15)

PR1 follows the commits below; where the code parted from this plan,
the plan was wrong in a detail and the code is the record:

- **`apply` needs no node check of its own.** It records where the
  pending queue ends, calls the command (every command already refuses an
  unknown node before changing anything), and on success inserts the mark
  at that position. Same outcome as "check, mark, command" (a refusal
  leaves no trace, the mark precedes the effect) without a second copy
  of the node lookups. No-ops are marked too (design D3).
- **The door's tests are `bess-models/tests/fault_door.rs`,** not unit
  tests beside `kernel/fault.rs`: a `Simulation` needs models and
  `bess-core` has none, which is also where phase 3 tested the commands.
- **`events.rs`'s tests moved to `events/tests.rs`;** the two new ones
  would have taken the file past the soft limit.
- **The crate has eleven source files, not eight.** `scenario.rs` holds
  the validated types (a concern of its own: what a file means once it
  passed), and `validate.rs` came out at 361 lines, so the (kind, fault)
  table went to `validate/physical.rs` and the data matrix to
  `validate/data.rs`; the parent keeps the file-level rules and the
  parameter check both use. `tests/support/play.rs` is the shell's loop
  for the integration tests, included by path. `Target::node()` became
  `reset_scope()` and `kind()`, which are what callers needed;
  `Player::position()` is `progress()` returning `Progress`, since
  `Position` is the error's.
- **The spread stops at 1.0** (design D3): the fastest leak the schema
  allows passed it within a month in the measurement.
- **The leak's integration case runs on one container** (twelve racks,
  half the site rating) for 30 days, 2 seconds in the dev profile; the
  rates and days are in CALIBRATION.md.
- **COMPATIBILITY.md gains the `fault` payload in this PR,** not PR2,
  because the payload ships here.
- **WASM:** `cargo check -p bess-scenario --target
  wasm32-unknown-unknown` passes (2026-10-15), so the crate is
  WASM-clean by measurement; CI's browser build still does not include it.

## PR2: the surfaces

What the PR does, in one paragraph: the shell learns to start from a
file and to be told a file while running, the player runs in the
simulation task ahead of every tick, and the plant narrates its own
story over REST and on the health endpoint.

### CLI (D5)

- `bess-emulator/src/cli.rs`: `--scenario <PATH>`; `seed`, `start_unix`
  and `speed` become `Option<T>` with the defaults moved into the help
  text and the resolution step.
- `bess-emulator/src/cli/run_params.rs`, new (about 150 with tests):
  `resolve(&Args, Option<&Scenario>) -> Result<RunParams, PrecedenceError>`
  with the five-cell table of the design's test plan and an error that
  names both values ("--seed 7 conflicts with the scenario's seed 42").
- `main.rs`: read the file, `bess_scenario::load` against
  `PlantConfig::gw01()`, resolve the parameters, log the scenario name
  and hash next to the seed and speed, hand `Resolved` to `sim::spawn`.
  A load error prints the position and reason and exits nonzero before
  any socket is bound.

### The simulation task (D6)

- `bess-emulator/src/sim/scenario.rs`, new (about 200): the player's
  home in the shell. `Active { resolved: Arc<Resolved>, player: Player }`,
  `fire_due(&mut self, sim: &mut Simulation)` called by the task before
  `sim.step` (physical actions go to `apply`, with the `Err` logged at
  `warn` and the run continuing: a scenario that trips a block the site
  does not have was refused at load, so an error here is a bug, not an
  operator mistake; data actions advance the cursor and log at `info`
  that they are scheduled only), and `status(&self, tick) -> Value`, the
  JSON `GET` answers.
- `sim.rs`: `Command::LoadScenario { resolved: Arc<Resolved>, reply }`,
  applied like the others between ticks: a fresh `Simulation` from
  `resolved.simulation(cfg, models)`, speed from the file when stated,
  the player reset, the snapshot republished at tick 0. `Snapshot` gains
  `scenario: Option<Arc<Value>>`, the status as of this tick, replaced
  only when something fired, so the per-tick clone stays one `Arc`.
  `spawn` takes the `Resolved` the CLI loaded, if any.

### REST and health (D6, D7)

- `bess-emulator/src/http/scenario.rs`, new (about 200), beside
  `http/alarms.rs` and in its style, with `http/scenario/tests.rs`:
  - `POST /api/v1/scenario`: body is the YAML text, `Content-Type:
    application/yaml` or `text/yaml` (JSON would mean escaping a YAML
    document inside a string, and `curl --data-binary @file` is the
    whole point). Load against the site's configuration; answer 200 with
    the status object once the task has switched, 400 with `{"error",
    "position"}` for a body that does not parse or validate, 422 for a
    file that is well formed and names a node the site does not have
    (the same split as the reset endpoint: the body is the problem, not
    the endpoint), 415 for another content type, 503 when the task is
    gone.
  - `GET /api/v1/scenario`: `{"loaded": false}` when the process has no
    scenario; otherwise `{"loaded": true, "name", "content_hash", "seed",
    "start_unix_s", "speed", "tick", "fired", "pending", "next": {...} |
    null, "events": [{"index", "at_s", "tick", "kind": "physical" |
    "data", "target" | "surface", "fault", "fired": bool, ...parameters}]}`.
    The resolved schedule is included in full (the open question, settled
    below).
- `http.rs`: the two routes; `health` gains `"scenario": {"name",
  "content_hash"} | null`.
- COMPATIBILITY.md, "Other surfaces": the two endpoints with their
  bodies and codes, the health field, the `fault` event payload; the
  checkpoint line gains format 8.
- README.md: the Surfaces table's REST row gains `scenario`; the CLI
  gets a one-line mention of `--scenario`. The quickstart's "run a
  scenario" step arrives with the library in phase 5, when there is a
  file worth pointing a newcomer at; a quickstart into a test fixture is
  a quickstart into the test tree.

### Documents

- ARCHITECTURE.md, "Scenario engine": the YAML sketch is replaced by the
  v1 shape (offsets, not clock times) and the paragraph says which half
  is built (physical, this phase) and which is scheduled only (data,
  phase 5).
- docs/design/m2/README.md: release-note inventory entries (the kernel
  door and `FaultAction`; `Event::FaultApplied`; checkpoint 8; the rack
  field; `--scenario` and the precedence rule; the two endpoints; the
  health field; the MQTT `fault` payload; "data faults are scheduled, not
  executed, until phase 5"); the versioning paragraph's checkpoint line;
  the WASM question struck through with its answer.
- This file and design.md: D1 to D12 marked confirmed or revised, the
  open questions below closed.

### Accept

`cargo run --release -p bess-emulator -- --scenario
crates/bess-scenario/tests/fixtures/hvac-failure-hot-week.yaml --speed
3600` runs the story end to end; `GET /api/v1/scenario` narrates it live
(`fired` climbing through the schedule, `next` naming the repair); the
stream shows the `fault` event for block 2 followed minutes later by the
derate and setpoint raises the chain test holds; `/health` names the file
and its hash; and `POST` of the same file with `curl --data-binary`
restarts the story from tick 0. `http/scenario/tests.rs` is the code
side of the acceptance.

### Called out in the PR

Three CLI flags changed type (`Option`), with no change to their defaults
or help. No map change, no digest change, no record change.

### Not in PR2

Data-fault execution and the tap (phase 5 PR1); `scenarios/` and
`--assert` (phase 5 PR2); scenario loading in the WASM viewer (not in
M2); an unload endpoint; a "fire one action now" endpoint (D6).

### As landed (2026-10-15)

Four commits, each green, in a different order from the list below:
the scenario crate's additions and CI first, then the task with the REST
door and health in one commit (an unconstructed `LoadScenario` does not
pass `-D warnings` on its own), then the command line, then the
documents. Where the code parted from the plan:

- **Two PR1 review findings landed here:** a `duration` of PT0S is
  refused (`Reason::ZeroDuration`), and CI checks `bess-scenario` for
  wasm32 beside the browser build, so the "WASM-clean" claim is measured
  on every change rather than once by hand.
- **`Target::of(&FaultAction)` and `Display for Target`:** the resolved
  schedule carries actions, not paths, and the status spells targets as
  the file does; the round trip is tested for every row of D2. The
  parser's tests moved to `schema/target/tests.rs` for the room.
- **The status has no tick of its own.** `Snapshot::scenario` is rebuilt
  only when something fires, as planned, so `tick` cannot live in it;
  `GET` adds the snapshot's tick, and the load's answer carries tick 0.
- **The external setpoint does not survive a load** (D6, made explicit).
- **`POST` validates against `PlantConfig::gw01()`,** as `spawn` builds
  the plant; the handle carries no configuration. The day the site is
  parametric, the configuration moves into the handle.
- **The error body:** `error` is the whole line the CLI prints, position
  and reason, and `position` the position alone, so a client reading one
  field has everything and one pointing at an editor has the position.
- **The task is a `Task`:** `sim.rs` grew from 193 to 276 lines because
  the loop owns a third thing beside the kernel and the speed; a struct
  with `command` and `tick` replaced an `apply` that would have taken
  seven arguments. `sim/scenario.rs` is 125 lines, `http/scenario.rs`
  113 with 182 of tests beside it, `cli/run_params.rs` 181 with its
  tests inside.
- **The acceptance command runs without `--speed 3600`:** the fixture
  states `speed: 60`, and under D5 a flag that disagrees is an error,
  which is the right outcome; the command above was written before D5
  said so. Measured instead, on the dev build: a run under the fixture
  narrates six events with the leak's 6,962 ppm/day visible; `POST` of
  `hot-afternoon.yaml` with `curl --data-binary` answers tick 0 and
  fires both failures on the first tick, 183 ticks in after three wall
  seconds at 60x; `/health` names each file and its hash; 415, 400 and
  422 carry their bodies; `--speed 3600` against the fixture and a
  schema 2 file end the process before a socket is bound. The sockets
  test in `http/scenario/tests.rs` holds the same story, 503 included.
- **A data fault's `duration_s` is left out of its JSON where the fault
  takes none,** so a backfill burst shows without a null.

## Code layout (AGENTS.md: one file, one concern)

- `bess-core`: `kernel/event.rs`, `kernel/fault.rs`, `digest.rs` new;
  `kernel.rs` loses `Event` and stays under the soft limit; `state.rs`
  gains `ScenarioId` (221 lines today, under 250 after).
- `bess-models`: `balance.rs` gains one term; `tests/self_discharge.rs`
  new.
- `bess-scenario`: eight source files as listed, none over 250 lines
  by budget; a file that crosses 300 while being written is split before
  the commit, not after.
- `bess-emulator`: `sim/scenario.rs`, `http/scenario.rs` and its tests,
  `cli/run_params.rs` new; `sim.rs`, `http.rs` and `cli.rs` grow by the
  wiring only (`sim.rs` 193 to about 240, `http.rs` 220 to about 240).
- AGENTS.md's table does not change: nothing on it is touched beyond a
  localized edit, and nothing new goes over.

## Tests, by crate

- `bess-core`: `kernel/fault.rs` unit tests (every variant against its
  command, no-ops, refusals, the mark precedes the effect in
  `pending_events`); checkpoint 8 round trip and 7 refused; `digest.rs`
  pins one known FNV-1a value so a refactor of the callers cannot move it
  silently.
- `bess-models`: `tests/self_discharge.rs`; `tests/determinism.rs`'s
  `GOLDEN_DIGEST` regenerated once.
- `bess-scenario`: `schema/duration.rs` and `schema/target.rs` unit
  tests; `tests/goldens.rs`, `tests/rejections.rs`,
  `tests/determinism.rs`, `tests/twin.rs`.
- `bess-emulator`: `events.rs` (topic and payload of `fault`);
  `cli/run_params.rs` (the five cells); `http/scenario/tests.rs` (load,
  reset, status, the four error codes, health); `sim/scenario.rs` (a
  data event advances the cursor and touches nothing).
- CI: `cargo check -p bess-wasm --target wasm32-unknown-unknown` keeps
  passing without `bess-scenario` in the browser build, and
  `cargo check -p bess-scenario --target wasm32-unknown-unknown` is run
  once by hand in PR1 and its result recorded here, so the "WASM-clean"
  claim is a measurement.

## Commits, in order

PR1:

1. This document, design.md and architecture.md (the pin).
2. `bess-core`: `digest.rs`, `kernel/event.rs`, `kernel/fault.rs`, the
   two tree fields, checkpoint 8, `events.rs`'s arms; the golden digest
   moves here and the constant's comment names the two fields.
3. `bess-models`: the term, `tests/self_discharge.rs`, the CALIBRATION.md
   section.
4. `bess-scenario`: manifest and `cargo add` of the parser (after the
   yes), schema, durations, targets, errors, validation, the goldens and
   rejections.
5. `bess-scenario`: resolution, the player, `tests/determinism.rs`,
   `tests/twin.rs`; the README inventory entries that PR1 owns.

PR2:

1. `cli.rs`, `cli/run_params.rs`, `main.rs`.
2. `sim/scenario.rs`, the command, the snapshot field.
3. `http/scenario.rs` with tests, the routes, the health field,
   COMPATIBILITY.md, README.md.
4. ARCHITECTURE.md, the README inventory, the two design files marked
   confirmed or revised.

Each commit leaves `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace` and `cargo run
-p bess-bench -- --check-docs` green. Run `rustup update stable` first:
CI's clippy floats and has been ahead of this machine in the last three
PRs.

## Open questions

Settled on 2026-10-14, at the pin:

- **`GET /api/v1/scenario` includes the resolved schedule.** Every event
  with its tick and its drawn parameters: it is small (a library case has
  a handful of events) and it is the reproduction record, the one place
  a drawn value is visible.
- **The WASM shell does not load scenarios in M2.** The milestone README
  owned the question; the answer is later, with the next view iteration.
  The crate compiles for the target either way, measured in PR1.
- **Where the acceptance file lives:** `crates/bess-scenario/tests/
  fixtures/`, not `scenarios/`; the library and its README contract are
  phase 5's.

Settled on 2026-10-15, in PR1:

- **D9, the parser:** `serde-saphyr` 2.0.0, Ali's yes; deserialize only.
- **Spans on values:** the parser has them (`Spanned<T>`). Each event
  is read spanned, so a semantic rejection names `events[i].field` and
  the line the event starts on; per-value spans were not needed.

Settled on 2026-10-15, in PR2:

- **The three decisions the plan left silent** (the status and the
  tick, the setpoint on a load, the configuration `POST` validates
  against) are recorded under "As landed" above and in design.md D6 and
  D7.

Open for PR2 and phase 5:

- **The taxonomy tag set** (`bos`, `controls`, `cell`, `calendar`,
  `maintenance`): phase 5 pins the failure categories against the EPRI
  snapshot and may add tags; an added enum variant is an addition to the
  file format, and the enum sits in one file.
