# M2.6 phase 2: the library in the browser, architecture

What exists after this phase: the fifteen library cases are inside the
browser module and a person can start one from a list in the viewer; the
viewer plays it with the same engine, the same door and the same
numbering as the native shell, during live stepping and during jumps
alike; a progress strip shows what has fired, what is next and what a
data event would have done on a surface the browser does not have; and a
host page can start a case or hand in a file of its own through a small
JS door. Then the release.

Pinned on 2026-10-17 against `develop` after PR #26: `bess-scenario` with
`load`, `Scenario::resolve`, `Resolved::simulation`, `Player::due` and
`catch_up`; the native fire-then-step loop in three places
(`bess-emulator/src/sim/scenario.rs::Active::fire_due`,
`bess-emulator/src/headless.rs::run`, `bess-scenario/tests/support/play.rs`);
the viewer's wall-time loop in `viewer.rs` and its jump in
`viewer/jump.rs`, both stepping a bare `Simulation`; `WebHandle` with
`new`, `start`, `destroy`.

## One run loop in the viewer

Today the viewer advances the plant in two places, the wall-time path and
the jump, and neither knows about a schedule. Adding a scenario to one and
not the other would mean a jump across an event's tick silently skips it,
which `Player::due` would then hand out late, after the jump lands. So the
two paths stop stepping a `Simulation` and step a `Run`:

```
viewer/run.rs
  Run { sim: Simulation, story: Option<Story> }
  Story { resolved: Arc<Resolved>, player: Player }
  Run::tick(&mut self, weather) -> &[Event]
      fire what the player has due before this tick, through the
      kernel's door; step; return the tick's events
```

`FastForward::advance` takes `&mut Run` and calls `Run::tick`, so a jump
fires events on their tick like any other frame, and the existing test
that a jump lands on the same state as a plain run now also holds with a
story loaded: the digest at the end of a jumped case equals the digest of
the headless run, which is the iteration's gate.

The fire-then-step step itself is written once more in this phase, and
that is one time too many. The native shell already has it three times.
The helper the four callers share goes into `bess-scenario`, which so far
"never steps a simulation" by its own doc comment and keeps that: the
helper applies what is due and returns what came due, it does not step.

```
bess-scenario
  Player::fire_due(&mut self, sim: &mut Simulation) -> Result<&[Scheduled], Refused>
      every scheduled event with tick <= sim.state().tick, physical ones
      applied through Simulation::apply in schedule order, all of them
      returned so a shell can open its taps for the data ones
```

`Active::fire_due`, `headless::run` and the test helper call it; the
viewer's `Run` calls it. The library's fifteen records are what proves
the native refactor changed nothing: CI plays every case against its
digest on every pull request.

## The library inside the module

A new module `bess_scenario::library` embeds each file of `scenarios/` at
build time (`include_str!`), keyed by its `name`, with the bytes as
written (so the content hash is the one `/health` and the records carry),
the taxonomy, the length, and a one-line headline read from the file's
header comment, which the library contract already requires every case
to have. The existing `library` integration test, which reads the
directory and holds the EPRI mix, gains one assertion: the embedded set
and the directory are the same files, byte for byte, so a case added to
the directory without being embedded fails CI rather than quietly missing
from the browser.

The YAML parser comes along. It is pure Rust, `unsafe` forbidden, and CI
has checked the crate for `wasm32-unknown-unknown` since M2 phase 4; what
changes is that `bess-wasm` now links it, and the module grows by the
parser plus some fifteen kilobytes of YAML. The size is measured in the
PR, before and after, and recorded in the release-note inventory. The
alternative that keeps the parser out, a generated Rust table of
pre-resolved schedules, is design D1's losing option.

## Starting a case

Starting a case in the viewer means what loading one over REST means
(M2 phase 4 D6): the plant is thrown away and a fresh one is built at the
file's seed and start, at the file's speed if it states one, with the
event log back at `seq` 1 and no external setpoint. The viewer's own
defaults stand in for what the file leaves out, as the shell's current
values do over REST: the session seed, and the date the plant is
currently on. The journal is cleared with the plant; the selection and
the camera stay, since they are the person's, not the plant's.

While a story plays:

- The wall-time loop and the jump fire what is due, through `Run`.
- **Restart on a date** and the **dataset stops** leave the story: a
  schedule is offsets from a start, and a plant restarted elsewhere is
  not the file's run. The buttons say so while a story is loaded. The
  scenario identity leaves the tree with the plant, as `SiteMeta::
  scenario` was designed to.
- **Run forward to a date** keeps it.
- Two new buttons: **run to the next event**, a jump that lands on the
  next scheduled tick (the event fires on the frame after landing, so
  the person sees the plant just before and just after), and **run to
  the end**, a jump to the file's `length` when it states one.
- After the length the plant keeps running, as it does natively; the
  strip says the story is over.

## The progress strip

Below the picker: the case's name and headline, the EPRI tag, the
elapsed offset against the length, `fired` of `total`, the next event
with its offset and target, and the whole schedule as a list with each
row marked fired or pending. A physical event shows its target as the
file spells it and its drawn rate when it has one. A data event shows its
surface and fault and a note that it acts on MQTT and Modbus only, which
this view does not have; this is the same truthfulness REST and the
stream keep by design, and the strip is where the person learns that the
`alarm-chatter-storm` case will show them nothing here, and why. A thin
timeline painted on a strip puts the same schedule on a time axis with a
cursor at now.

## The JS door

`WebHandle` gains four methods, all optional for a host that only wants
`start`:

| Method | In | Out | Does |
|---|---|---|---|
| `scenarios()` | nothing | JSON string: array of `{name, taxonomy, length_s, headline}` | lists the embedded library |
| `start_scenario(name)` | a library name | nothing, or throws with a message | starts that case, as the picker does |
| `load_scenario_yaml(text)` | a scenario file's text | nothing, or throws with the `LoadError` text and position | validates the file against GW-01 and starts it |
| `scenario_status()` | nothing | JSON string in the shape of `GET /api/v1/scenario`, or `{"loaded": false}` | the story's progress at the current tick |

JSON strings rather than JS objects, so no serializer crate enters the
browser build (design D10); the host page calls `JSON.parse`. The status
is built by the same function the REST endpoint uses, which for that
moves from the native shell into `bess-scenario`; the shell's status test
holds the shape. The door reaches the app through `eframe::WebRunner::
app_mut`, between frames, and each method is a one-line call into a
`ViewerApp` method that the native viewer example and the tests can call
without a browser.

A host page that embeds the viewer can therefore do what the study page
needs: offer its own buttons, start the case the article is about, and
read where the story is to keep its prose in step. It cannot read plant
data, which is the M0 rule and stays one.

## What changes where

| Crate | Change |
|---|---|
| `bess-scenario` | `library` module (embedded cases, headline from the header comment); `Player::fire_due`; the status JSON builder moved in from the shell (`serde_json` becomes a normal dependency of the crate; it is a workspace dependency already) |
| `bess-emulator` | `Active::fire_due`, `headless::run` and `tests/support/play.rs` call the helper; `sim/scenario.rs::status` calls the moved builder; nothing on the wire changes |
| `bess-scene` | `viewer/run.rs`; `jump.rs` over `Run`; the picker and the progress strip in `panels/scenario.rs`; `ViewerCommand` gains `StartScenario(name)`, `LoadScenarioYaml(text)`, `RunToNextEvent`, `RunToEnd`; `bess-scenario` as a dependency behind `sim` |
| `bess-wasm` | the four methods |
| `COMPATIBILITY.md` | a section for the browser module: `WebHandle`'s methods are the contract, additions are minor, a signature change is major; the panels and the scene are not a contract |
| `README.md`, `ROADMAP.md` | the browser build plays the library; the iteration flipped to done at release |
| `bess-core`, `bess-models`, `bess-data`, `bess-bench` | nothing |

## Invariants

- A library case played through `Run`, live or jumped with any budget,
  leaves the tree and the event log at the digest in `scenarios/`. The
  browser and CI play one story.
- `Run` with no story is the plain viewer loop: identical trajectory,
  identical events. The engine costs nothing when idle, as M2 phase 4
  promised natively.
- Leaving a story (restart on a date) leaves a plant whose tree carries
  no scenario identity, so nothing downstream can mistake it for the
  file's run.
- The four `fire_due` callers produce what the three hand-written loops
  produced: the library's records are the proof, on every pull request.
- The status a host page reads is the status REST publishes, built by
  one function.
