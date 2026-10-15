# M2 phase 5: data faults, library, gate, implementation plan

Three PRs, each landing green.

## PR1: the tap

- Fault matrix on MQTT and Modbus (design D1, D2, D3), driven by the
  phase 4 schedule; REST and WebSocket untouched.
- Surface-seeded PRNG streams; kernel-cannot-tell property test.
- Per-cell unit tests and load-time rejections.

Accept: the dropout-with-backfill integration test passes, and the state
digest of a heavily faulted run equals the clean run's.

### As landed (2026-10-15)

Three commits on top of phase 4 PR2: the crate's two rules, the tap with
its tests, the documents. Where the code parted from this plan:

- **Two load rules, not only rejections of the matrix:** `points` has a
  grammar (D2) and a backfill burst needs a dropout that has ended (D1);
  the matrix rejections themselves landed in phase 4 PR1.
- **No surface-seeded PRNG.** Every fault is a function of its
  parameters and the tick, so there is nothing to draw; the property
  the architecture claims holds by construction and by the test
  `the_kernel_cannot_tell_a_data_fault_happened`, which plays a story
  with all seven kinds over its physical events through the task's own
  `Active` and compares tree and log with the physical story's.
- **The capture is the task's.** A freeze's values are taken when the
  event fires, before the tick steps, not on the first read inside the
  window: deterministic in simulation time, and the same values serve
  both surfaces (Modbus re-encodes them).
- **The Modbus dropout is a transport.** tokio-modbus closes a
  connection only when its stream errors, so the session runs over
  `Wire`, a `TcpStream` whose reads and writes answer `ConnectionReset`
  while the window is open; a new connection gets `Ok(None)` from
  `on_connected`, accepted and closed. A true refusal would mean
  unbinding the listener, which no device does.
- **The MQTT dropout keeps the broker session.** The client stays
  connected and publishes nothing; a device that lost its uplink would
  drop off the broker too (and trigger a last will), which is a
  refinement for when a scenario needs the broker's view.
- **Chatter publishes what a wake sees.** The square wave is exact on
  Modbus (every read computes it); on MQTT an edge goes out when the
  publisher's wall-second wake observes a flip state different from the
  last published, so at high acceleration a fast chatter publishes fewer
  edges than it has, like cadence points collapse at speed. The events
  subtree stays the kernel's: a fake `raised` would need a fake `seq`.
- **The property test and the two integration tests** cost about 13
  seconds together, the integration tests at 1x so a second is a tick:
  the dropout with backfill over the test broker (silence, then the
  burst, born inside the silence, in order, ahead of the live stream)
  and the Modbus dropout over real sockets.

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

### As landed (2026-10-15)

Three commits on top of phase 5 PR1: the length and the headless run,
the library with its gate and the CI step, the documents.

- **`length` is a file field.** The plan had the run length nowhere;
  the record could have carried it, but then a case would have been a
  file plus a number elsewhere. Schema 1 gains the optional field, an
  event past it is refused, and `--assert` and `--record` need it.
- **`--record` and `--assert`,** not one flag with a write mode: the
  bench's `--write` and `--check` pattern. Both play headless, no
  socket bound, through `headless.rs`; the loop is the integration
  tests' `play`, and data faults are skipped, since the kernel cannot
  tell.
- **Fifteen cases, not a dozen:** the EPRI shares at a dozen would have
  put one class a case short of its band, and the two DST days and the
  two maintenance stories are the calendar and maintenance scope of the
  milestone made concrete. Composition and the band are in
  CALIBRATION.md.
- **The calendar cases run the whole local day** from local midnight,
  so their records say 92 and 100 closed periods, which is the point.
  The simulated March 31 and October 27 are the DST days, because the
  planner's calendar is the price year's, mapped by month and day.
- **The runtime budget is measured:** 37 s for the fifteen cases on
  the development machine in the release profile, the ten-day hot week
  14 s of it. Inside a minute, so every case runs on every pull request
  in the calibration job (`scripts/assert-library.sh`); nothing moves to
  release-tag frequency.
- **The gate is a crate test,** `bess-scenario/tests/library.rs`,
  reading `scenarios/` relative to the manifest, so `cargo test
  --workspace` holds the composition without the headless runs.

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

### As landed (2026-10-15)

- **The version bump moved only version strings.** The annual record
  and the fifteen library records were regenerated under 0.5.0 and
  changed in `engine_version` and nowhere else: the release is the
  physics the phases measured, and CALIBRATION.md's new section says
  so with the three figures that did move over the milestone and why.
- **No separate "M2 entry" with a composition table:** the library
  section (PR2) carries the EPRI snapshot and the composition, the
  release section carries the annual figures; two sections for two
  measurements rather than one that repeats both.
- **The release note is a file,** `docs/design/m2/release-notes.md`,
  edited from the inventory, so the tag can be created with
  `--notes-file`; the M1.5 note was composed at tag time.
- **The tag, the Docker image and the npm publish are the maintainer's
  steps** after the merge: the PR carries everything they read.

## Open questions

Settled on 2026-10-15, in PR2:

- **Launch library size: fifteen,** eleven failures and four outside the
  gate; the cell class is one case at the EPRI share (design D4).
- **`--assert` compares the fiscal meter's closed periods,** the count
  rather than the series: the DST cases' 92 and 100 are what the series
  would say, in one number a reader can check against the case's
  header.
