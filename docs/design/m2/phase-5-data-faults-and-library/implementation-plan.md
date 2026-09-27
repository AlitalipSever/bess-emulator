# M2 phase 5: data faults, library, gate, implementation plan

Three PRs, each landing green.

## PR1: the tap

- Fault matrix on MQTT and Modbus (design D1, D2, D3), driven by the
  phase 4 schedule; REST and WebSocket untouched.
- Surface-seeded PRNG streams; kernel-cannot-tell property test.
- Per-cell unit tests and load-time rejections.

Accept: the dropout-with-backfill integration test passes, and the state
digest of a heavily faulted run equals the clean run's.

## PR2: the library and the gate

- `scenarios/` with its README contract and the launch cases (physical,
  data, calendar, maintenance), each with taxonomy tag and committed
  digest (D4, D5).
- `--assert` mode; the library in CI on every PR; runtime measured and
  the budget decision recorded here.
- EPRI database snapshot pinned: shares, retrieval date, CALIBRATION.md
  section, gate test against the tags.

Accept: deleting a balance-of-system case or adding three cell cases
fails CI with the composition numbers in the message.

## PR3: release v0.5.0

- Annual record regenerated (D6); CALIBRATION.md M2 entry (composition
  table plus the re-measured annual figures and what moved them:
  derating, real prices).
- COMPATIBILITY.md: map 0.3.0 row, the alarm meaning change called out;
  the "two known breaks" paragraph drops to one (M3's PCS state machine
  remains).
- ROADMAP.md: M2 flipped to done; ARCHITECTURE.md scenario section
  updated from sketch to fact; README quickstart shows a scenario run.
- Release notes from the milestone README's inventory.
- Version to 0.5.0; the record's engine-version check makes this a
  regeneration PR, as M1 PR8 learned.

Accept: tag pushed, docker quickstart green, npm publish script run per
the M1.5 release path.

## Open questions

- Launch library size: a dozen is the proposal; the EPRI shares may
  force more cases to make the smallest class nonzero without breaking
  the ratio, resolved when the snapshot is pinned (design D4).
- Whether `--assert` should also compare the revenue meter's period
  series for the calendar cases (leans yes: it is small, and DST is the
  point of those cases).
