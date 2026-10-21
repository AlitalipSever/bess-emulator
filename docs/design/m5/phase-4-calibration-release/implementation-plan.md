# M5 phase 4: calibration and release, implementation plan

Two PRs.

## PR1: the gate

- `bess-bench`: the two bands, `check_aging`, `--check` covering
  `calibration/m5-aging.json`, the report's rendering of the trajectory
  table with the bands.
- `.github/workflows/ci.yml`: the projection and the gate in the
  calibration job.
- CALIBRATION.md: "M5: the trajectory against the bands", the bands
  written first, then the numbers.

Accept: the gate passes on the committed trajectory; a trajectory edited
outside the band fails it locally.

## PR2: release v0.10.0

- Version bump across the workspace and the wasm package; every record
  regenerated; `docs/design/m5/release-notes.md` drafted from the
  inventory; ROADMAP.md, README.md, COMPATIBILITY.md's checkpoint line;
  the memory note.

Accept: CI green; the records moved in their version string only; the
maintainer's steps (main fast-forward, the tag, the GitHub release, the
npm publish, gridworks' dependency) listed in the closing message.

## Open questions

- Whether the gridworks site's emulator page should get an aged preset
  control in the same release or in its own change; leans its own, the
  wasm package only needs to expose `age_years`.
