# M2.6 phase 2: the library in the browser, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the library is embedded at build time (needs the maintainer's
  answer).** `bess_scenario::library` holds the fifteen files through
  `include_str!`, bytes as written, and the YAML parser comes into the
  browser module with it. Options considered on 2026-10-17:

  | Option | For | Against |
  |---|---|---|
  | embed the files and the parser (recommended) | works offline and from any host; the content hash is the file's; the same `load` and validation as the shell; a custom file can be handed in through JS | module size: the parser (`serde-saphyr`, `granit-parser`, their encoding layer) plus 15 KB of YAML; estimated at a few hundred kilobytes on a 4.47 MB module, measured in the PR and recorded |
  | generate a Rust table of pre-resolved schedules from the files (a generated file checked by a test, the signal-map pattern) | no parser in the browser; smallest module | a second representation of every case that can drift from the file; `seed` and ranges are resolved at generation, so the viewer could not honour a different session seed; no custom files; a generator step in the build |
  | fetch the files over HTTP from the host | nothing embedded | a data path from JS into the module, which the M0 architecture refuses for data; couples the module to the host's paths; does not work in the native example |

  Acceptance for the recommended option: the measured growth is in the
  release-note inventory. If it exceeds half a megabyte, the PR says so
  and the maintainer decides whether the table option replaces it; the
  design does not change otherwise.

  **Measured in PR1, over the threshold.** `bess_wasm_bg.wasm` at the
  end of phase 1: 4,544,287 bytes raw, 1,988,127 gzip, 1,593,835
  brotli. With the library, the parser and the scenario panel:
  5,470,019 raw (+925,732, +20 %), 2,320,230 gzip (+332,103),
  1,820,729 brotli (+226,894). The raw growth is above the half
  megabyte this decision set, so per its own rule the maintainer
  decides. What a browser downloads is the compressed figure, +227 KB
  with brotli. `encoding_rs` comes in through `serde-saphyr`'s
  `deserialize` feature but is not on the `from_slice` path (that is a
  UTF-8 check and `from_str`), so the linker drops it; the growth is
  the parser (`granit-parser`), its error rendering
  (`annotate-snippets`, `unicode-width`), the derived deserializers
  and the panel code. The table option would remove the parser and
  with it `load_scenario_yaml`.

- **D2, one run loop, one helper.** `viewer/run.rs` (architecture.md)
  and `Player::fire_due` in `bess-scenario`, adopted by the three native
  callers in the same PR. The alternative, a fourth hand-written copy in
  the viewer, is the thing the M2 phase 3 PR refused for the calendar
  arithmetic: the loop is six lines and the fourth copy is the one that
  forgets to apply in schedule order. The helper keeps the crate's
  purity: it applies through the kernel's door and returns what came
  due; it does not step and does no IO. `Refused` carries the event
  index and the `FaultError`, so a refusal (which can only be a bug,
  since the file was validated against this site) names its event, as
  the headless run's message does today.

  **Confirmed in PR1, one shape revised.** `Player::fire_due` returns
  `Fired { due, refused }` rather than `Result<&[Scheduled], Refused>`:
  a `Result` would hide the slice on a refusal, and the shell needs it
  to open the data faults' windows on the same tick. Every physical
  event is applied even after a refusal; `refused` names the first.
  The shell's "fault applied" log line loses its `changed` field,
  which the helper does not return. The headless run turns a refusal
  into its old message (`events[N] refused: ...`), the test helper
  asserts there is none, and the fifteen records hold.

- **D3, starting a case follows REST (M2 phase 4 D6).** A fresh plant at
  the file's seed and start, the file's speed if stated, the log back at
  `seq` 1, no external setpoint, the journal cleared. The viewer's
  session seed and current date stand in for what the file leaves out.
  Alternative: splice the schedule into the running plant, rejected in
  M2 for the reproducibility story and rejected here for the same
  reason; the digest gate would have nothing to compare against.

  **Confirmed in PR1.** `Session::play` builds the run from the file
  through `Resolved::simulation`; the session seed, the plant's current
  date and the session speed stand in for what a file leaves out (every
  library file states its seed and start; none states a speed). The
  journal, the unseen mark, the reset answer and any jump go with the
  old plant; the selection and the camera stay.

- **D4, what the time controls do during a story.** Restart on a date
  and the dataset stops leave the story and say so on the button
  ("Restart there, leaving the story"); fast-forward keeps it; "run to
  the next event" lands on the next scheduled tick and "run to the end"
  on the file's length. Landing on the tick rather than a minute before
  it: the event fires on the next frame, so the person sees the last
  honest frame before the fault and the first after it, and the jump's
  arrival rule ("never overshoots") already guarantees the landing.
  Alternative: refuse restarts while a story plays; a refused button is a
  worse answer than an honest label, and a person who wants to leave a
  story should be able to.

  **Confirmed in PR1.** Both jumps go through `FastForward`, which checks
  arrival before firing, so "run to the next event" lands on the event's
  tick with the event not yet fired; the session test holds that. The
  restart button reads "Restart there, leaving the story" while a case
  plays, and the dataset stops say the same on hover.

- **D5, data faults are listed, not executed.** The browser has no MQTT
  and no Modbus, so a data event has no surface to corrupt; the strip
  lists it, marked "on MQTT and Modbus only", and the physics plays on.
  This is the REST and stream rule of M2 phase 5 seen from the browser.
  Alternative: port the tap and a register projection into the view to
  show "what SCADA would have seen" beside the truth. That is a real
  feature, a second data path through the map in the browser, and a
  whole iteration; it is recorded as an idea, not scheduled.

  **Confirmed in PR1.** A data event is greyed in the schedule with a
  note that it acts on MQTT and Modbus only; the picker marks a case
  whose events all act on the wire ("on the wire") and a case with some
  ("2 on the wire").

- **D6, the JS door (needs the maintainer's confirmation of the set).**
  The four methods in architecture.md. Strings in, JSON strings out,
  errors as thrown `JsValue` strings carrying the `LoadError` text and
  position. `start` and `destroy` are unchanged. Each method is a thin
  call into a `ViewerApp` method, so the logic (name lookup, load,
  restart) is tested natively and the wasm layer is only marshalling. The
  status is built by the function that builds `GET /api/v1/scenario`,
  moved from `bess-emulator/src/sim/scenario.rs` into `bess-scenario`;
  the shell's test of that shape holds it in its new place. Options for
  the status: no status door (the host keeps to control methods and
  shows nothing of the story; simplest, and enough if the study page
  only starts a case); a status door with the REST shape (recommended:
  a page that starts a case will want to say where it is, and inventing
  a second shape for the same facts is the thing COMPATIBILITY.md
  exists to prevent); a callback or event stream into JS (a data bridge,
  refused since M0).

  **Confirmed in PR2, one addition.** `scenarios()` does not need a
  running viewer: it builds the catalogue from the embedded files for
  GW-01, so a host page that starts the canvas lazily (the study page
  does) can list the cases first. The other three reach the session
  through `WebRunner::app_mut`, which needs `App::as_any_mut`; before
  `start` they throw "the viewer is not running". Seen in a plain page
  over the browser build: `fired` read 0 at tick 0, 1 at tick 2162 and
  2 by tick 5258, `start_scenario("no-such")` threw with the library's
  names, and an off-site file threw with its position.

- **D7, a case's headline is its header comment.** The library contract
  requires every case to "state what it demonstrates in its header
  comment"; `library` reads the leading comment block, joins its lines,
  and a test requires it to be present and under a stated length. The
  alternative is a `description` field in schema 1: the cleaner source,
  but a file-format change on a format the library made a compatibility
  surface of (a v0.5.0 binary refuses a file with a field it does not
  know, by `deny_unknown_fields`), for a sentence the files already
  carry. Deferred until schema 2 has a second reason to exist.

  **Confirmed in PR1.** `Case::headline` joins the leading comment
  lines; the library test requires one per case, at most
  `HEADLINE_MAX` (600) characters. The longest today is about 450.

- **D8, the picker.** The cases grouped by taxonomy in the README's
  order (`bos`, `controls`, `cell`, `calendar`, `maintenance`), each row
  the name, the length and the headline, a start button per row; the
  data-fault cases carry a small mark that says their lesson is on the
  wire. No search, no sorting; fifteen rows fit on one screen.

  **Confirmed in PR1, two details.** The taxonomy heading sits in the
  name column of one grid, so the button column keeps its width and the
  rows align across groups; one-day lengths read `1 d`. Starting a case
  folds the library, so the strip and the alarm section are what come
  into view.

- **D9, versioning.** v0.6.0 if M2.6 ships before M2.5 (the milestone
  README's first open question), the records regenerated for the version
  string alone, the M1.5 precedent: a regenerated record that moves only
  its version string is the evidence that the iteration was as
  physics-free as it claimed. The npm package follows the crate version
  through `scripts/publish-npm.sh`, which the maintainer runs.

  **Confirmed in PR2.** 0.6.0; the fifteen records and
  `calibration/m1-annual.json` regenerated, and the diff is
  `engine_version` alone. M2.5's README moves to v0.7.0.

- **D10, no new dependency (needs the maintainer's confirmation).** One
  candidate considered and declined:

  | Crate | What it would give | Why not here | Used instead |
  |---|---|---|---|
  | `serde_wasm_bindgen` 0.6 | `serde` types to and from JS objects directly | a crate for four methods whose payloads the host parses anyway; `wasm-bindgen`'s own `serde-serialize` feature is deprecated for the same job | JSON strings over `wasm-bindgen`'s existing `String` marshalling; the host calls `JSON.parse`; the shape is the REST shape, documented once |

  `serde_json` becomes a normal dependency of `bess-scenario` for the
  moved status builder; it is a workspace dependency already and was a
  dev-dependency of the crate, so no new crate enters the workspace or
  the browser build (`bess-core` already links it for the checkpoint).

  **Confirmed in PR2.** No crate added; `serde_json` moved from a dev to
  a normal dependency of `bess-scenario`.

- **D11, COMPATIBILITY.md names the browser module.** A short section
  under "Other surfaces": the npm package `bess-emulator` exposes
  `WebHandle`; its methods and their JSON shapes are the contract;
  adding a method or a JSON field is minor, changing a signature or
  removing a field is major; the egui panels, their layout and the scene
  are not a contract and change without notice. The section exists
  because a host page is about to wire itself to the door, and a
  surface someone wires to is an API.

  **Confirmed in PR2.** A bullet under "Other surfaces", from v0.6.0.

- **D12, `ViewerCommand` grows by four variants** (`StartScenario`,
  `LoadScenarioYaml`, `RunToNextEvent`, `RunToEnd`), a source-level break
  for a downstream exhaustive `match`, called out in the inventory.
  **Revised in PR1: three.** `StartScenario(&'static str)` (a library
  name), `RunToNextEvent`, `RunToEnd`. Loading a file's text is a
  `Session` method only: no panel issues it, and a `String` payload
  would cost `ViewerCommand` its `Copy`.

## Signal map impact

None.

## Checkpoint impact

None. `SiteMeta::scenario` already carries a run's identity; the viewer
sets it through `Resolved::simulation` as the shell does, and a restart
on a date leaves it `None` as a plain run does.

## Determinism and the public surfaces

- A case played in the viewer leaves the digest the headless run leaves:
  the gate, held by a test (below) and by the person who compares the
  viewer's event numbers with the record's count.
- REST, MQTT, Modbus, the stream and `/health` change nothing. The
  helper and the moved status builder are refactors inside the native
  shell; the library's fifteen records and the shell's status tests are
  the proof.
- The browser module API grows by four methods, additive; the section in
  COMPATIBILITY.md states the rules from this release.

## Test plan

- **The embedded library equals the directory:** names, bytes, count;
  the EPRI gate's test gains the assertion.
- **Every case has a headline** under the stated length, read from the
  header comment.
- **Digest equality through `Run`:** every library case up to two days
  long (eleven of the fifteen, counted as thirteen when PR1 landed; the ten-day and six-day cases run in CI's
  `assert-library.sh` already and would cost minutes here) played
  through `Run` with a per-frame budget of 997 ticks, the awkward number
  `jump.rs` already uses, ends at the recorded `state_digest`, event
  count and event digest. A second pass alternates live ticks and jumps
  across the event ticks.
- **An event on a jumped-over tick fires once,** on its tick, whatever
  the budget; the resulting log equals the live log.
- **Idle cost:** `Run` without a story against the plain viewer loop
  over six hours, identical state and events.
- **Leaving a story:** a restart on a date during a case yields a tree
  with `meta.scenario == None` and an empty journal; `scenario_status`
  then reads `{"loaded": false}`.
- **The door's logic, natively:** `ViewerApp::start_scenario("no-such")`
  errors with the library's names in the message;
  `load_scenario_yaml` with a file naming `block[25]` errors with the
  position; the status after N ticks counts `fired` as the schedule says.
  The wasm layer itself is not run in CI (no browser there, by the
  repository's standing rule); it is exercised by the maintainer in the
  host page before the release.
- **The native refactor:** `scripts/assert-library.sh`, the shell's
  scenario status tests and its headless tests, all unmoved.
- **Boundary gates:** golden digest, `bess-bench --check`, the signal map
  diff, unmoved; the record regenerated once for the version string at
  release and the diff shown to be that string alone.
- **Size:** `bess_wasm_bg.wasm` measured before and after PR1, the
  numbers in the inventory.
- **By eye:** the maintainer starts `hvac-failure-both-units` in the
  browser build, runs to the next event, watches the chain appear in the
  phase 1 panel and on the scene, and starts `alarm-chatter-storm` to
  confirm the strip explains why nothing happens.

## Open questions

- Whether the viewer should offer a file picker of its own for a custom
  scenario. It would need a file-dialog crate (`rfd`) or a JS `<input
  type="file">` the module reads through web-sys; both are more surface
  than the JS door, which covers the case from the host. Left to the
  host page for now.
- Whether "run to the next event" should land a short while before the
  tick instead of on it, so the last honest frame is on screen for
  longer than one frame. D4 lands on the tick; the maintainer's eye
  decides in PR1.
- Whether the picker should also appear in the native viewer example's
  window title bar or only in the panel. Panel only, unless the example
  grows a menu for other reasons.
