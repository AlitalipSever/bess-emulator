# Design documents

How milestone design is written down in this repository, from M2 on.

## Layout

One folder per milestone, one folder per phase inside it, three files per
phase:

```
docs/design/
  m2/
    README.md                     milestone overview
    phase-1-bms/
      architecture.md             what changes structurally
      design.md                   the decisions, with rationale and sources
      implementation-plan.md      PR breakdown and acceptance criteria
    phase-2-alarms/
      ...
```

The milestone README carries what no single phase owns: the goal, the phase
list with dependencies, the calibration gate, versioning (crate, signal map,
checkpoint), the release-note inventory, and the open questions. Everything
phase-specific lives in the phase folder.

## What each file answers

- **architecture.md**: what exists after this phase that did not exist
  before, said structurally. Traits added or widened, state tree changes,
  new crates, data flow, invariants. This is the part that outlives the
  phase.
- **design.md**: the numbered decisions (D1, D2, ...), each with rationale,
  the alternative that lost, and the public source it leans on. Plus the
  phase's signal map delta, checkpoint impact, and test plan.
- **implementation-plan.md**: the PR sequence with acceptance criteria per
  PR, and the phase's open questions.

## Rules carried over from M1

These worked and stay:

- **Decisions are proposed in the document and confirmed or revised in the
  PR that lands them.** The document is then updated in that same PR, so it
  records what was done and why, not what was once hoped.
- **A calibration gate names the source it actually used**, never the source
  it was expected to use.
- **Golden snapshot regenerations are called out** in the PR that causes
  them and in the constant's own comment.
- **The release-note inventory is written as PRs land**, in the milestone
  README, so the release note at the end is an edit, not archaeology.

## History

M0, M0.5, M1 and M1.5 predate this layout and live as single files beside
the milestone folders. They are records, not templates; the M1 file is the
closest ancestor of this structure and shows where the three-files split
came from.
