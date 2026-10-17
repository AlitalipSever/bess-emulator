# M2.6 phase 2: the library in the browser, implementation plan

Two PRs, each landing green.

## PR1: the library, the helper, and the picker

- `bess-scenario`: `library` module with the fifteen embedded cases, the
  headline from the header comment, the directory-equality assertion in
  the existing `library` test (design D1, D7).
- `bess-scenario`: `Player::fire_due` (D2); `Active::fire_due`,
  `headless::run` and `tests/support/play.rs` adopt it; the library's
  records hold.
- `bess-scene`: `viewer/run.rs`, `jump.rs` over `Run`, the start
  semantics (D3), the time controls' behavior during a story and the two
  new jumps (D4), `panels/scenario.rs` with the picker, the progress
  strip and the painted timeline (D5, D8); `bess-scenario` behind `sim`;
  the four `ViewerCommand` variants (D12).
- Tests: digest equality through `Run`, the once-per-tick firing across
  jumps, idle cost, leaving a story.
- The module's size before and after, measured with the `publish-npm.sh`
  build path into a local output directory, recorded in the inventory.

Accept: `hvac-failure-both-units` started from the picker in the native
viewer example and in the browser build ends, run to the end, at the
event count its record states; the phase 1 summary shows the chain in
order along the way; `scripts/assert-library.sh` and every native test
are unmoved; the size growth is recorded and under the D1 threshold or
explicitly accepted.

## PR2: the JS door, the contract, and the release

- `bess-scenario`: the status builder moved in from
  `bess-emulator/src/sim/scenario.rs`; the shell calls it (D6).
- `bess-scene`: `ViewerApp::{scenarios, start_scenario,
  load_scenario_yaml, scenario_status}`, tested natively.
- `bess-wasm`: the four `WebHandle` methods as thin calls through
  `WebRunner::app_mut` (D6, D10).
- `COMPATIBILITY.md`: the browser module section (D11). `README.md`: the
  browser build plays the library, the door's four methods. `ROADMAP.md`:
  M2.6 flipped to done. This folder's documents marked as built.
- The release: version bump per D9, the calibration record and the
  fifteen digests regenerated for the version string and the diff shown
  to be that string alone, release notes edited from the inventory, the
  tag. The npm publish is the maintainer's step, as for v0.5.0.

Accept: a plain HTML page that imports the npm build, calls `start`,
then `start_scenario("pcs-trip-and-reset")`, and polls
`scenario_status()` shows `fired` going 0, 1, 2 at the offsets the file
states, with the phase 1 journal showing the trip and the reset; `cargo
check -p bess-wasm --target wasm32-unknown-unknown` is green in CI; the
boundary gates are unmoved.

## Open questions

- Carried from design.md: a file picker in the viewer, landing before or
  on the event tick, the native example's chrome.
