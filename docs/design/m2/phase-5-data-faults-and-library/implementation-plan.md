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
