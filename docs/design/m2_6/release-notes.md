# v0.6.0: M2.6, the alarms on the screen

Draft of the release note, edited from the inventory in [README.md](README.md).
The maintainer creates the tag with it:
`gh release create v0.6.0 --title "v0.6.0: M2.6, the alarms on the screen" --notes-file docs/design/m2_6/release-notes.md`
(drop this preamble first).

---

v0.5.0 taught the plant to misbehave and put every alarm on Modbus, MQTT,
REST, the stream and Prometheus. The viewer saw none of it: a red diode
blinked on a container, and the fifteen library cases could only be
played through the Docker sandbox. v0.6.0 puts both on the screen, in the
native window and in the browser, and adds no physics.

## Highlights

- **An alarm summary and an alarm journal.** The two displays the
  alarm-management standards name (ISA-18.2, IEC 62682). The summary
  lists every set bit of the three words, trips first, then in the order
  the plant is built, with when each raised. The journal lists what it
  raised and cleared, newest first, numbered and timed as the MQTT
  payload of the same event. Filters by severity and block, and a count
  of what arrived since you last looked.
- **Severity where the cause lives.** The container diode takes the
  loudest word of its racks, a diode on the converter skid the block
  word, a beacon on the transformer the site word, and a failed HVAC
  unit carries a mark. Trips red, warnings amber, equipment out of
  service on purpose muted; nothing blinks, since the plant has no
  acknowledgement and the standards keep flashing for that.
- **Rows and objects linked, and the reset.** A node in either list
  selects its object in the scene; a selected object lists its alarms.
  The operator reset is a button at each scope, through the same kernel
  door REST uses, and says what it could not clear.
- **The library in the browser.** All fifteen cases are compiled into
  the module. Pick one, start it, and follow it on a strip: what it
  demonstrates, how far along, what fired, what is next, on a timeline,
  with run-to-next-event and run-to-end. Data faults are listed and
  explained rather than executed: the browser has no MQTT and no Modbus
  for them to corrupt. A case played in the viewer lands on the digest
  CI holds it to, and a test plays thirteen of them through the viewer's
  own loop to prove it.
- **A door for the host page.** `WebHandle` gains `scenarios()`,
  `start_scenario(name)`, `load_scenario_yaml(text)` and
  `scenario_status()`, JSON strings in and out, the status in the exact
  shape `GET /api/v1/scenario` publishes and built by the same function.
  A page can start the case its text is about and keep its prose in step
  with the plant. It still cannot read plant data.

## Breaking changes

Pre-1.0, so these are called out rather than avoided. The signal map
stays 0.3.0, the checkpoint stays format 8, and nothing on the wire
changes. The breaks are Rust source breaks in the view crate, for
anyone building on `bess-scene` directly:

- `ViewerCommand` gains `ResetAlarms`, `StartScenario`, `RunToNextEvent`
  and `RunToEnd`; an exhaustive `match` has four arms to write.
- `PanelInput`, `PanelState` and `Palette` gain fields; the panel
  functions `alarms::section`, `detail::selection_detail`,
  `controls::time_travel` and `controls::stops` take new arguments.
- `FastForward::advance` takes a `Run` and the journal; `ViewerApp`'s
  internals moved into `Session`.
- The container diode no longer blinks.

**Additive.** `bess_core`'s published spelling of a node
(`AlarmNode::path`, `alarm_name`, `Severity::name`, `PcsOpState::name`,
`FaultAction::node_path`), which the shell now calls; `bess_scenario::
library`, `Player::fire_due` and `bess_scenario::status`; the four
`WebHandle` methods, which COMPATIBILITY.md now names as the browser
module's contract.

## Honest limits at M2.6

Two library cases no longer show what their headers promise: the plan
does not discharge in their fault windows on the replayed day, so
`hvac-failure-both-units` shows the cooling failure and its repair but
not the derate chain, and `pcs-trip-and-reset` trips a converter in
standby. Their records are faithful to their files; re-timing them is a
library change of its own. The browser module grew from 4.54 MB to
5.47 MB raw, 1.59 MB to 1.82 MB brotli, for the scenario parser and the
library. A row click selects but does not move the camera. The SoC
realism chain moved to M2.5, now v0.7.0.

## Try it

```sh
cargo run --release -p bess-scene --features sim --example viewer
```

In the browser, from npm:

```js
import init, { WebHandle } from "bess-emulator";
await init();
const handle = new WebHandle();
await handle.start(document.getElementById("bess-canvas"));
handle.start_scenario("protection-trip-staggered-return");
```

Full order of work: [ROADMAP.md](ROADMAP.md). The browser module's
contract: [COMPATIBILITY.md](COMPATIBILITY.md).
