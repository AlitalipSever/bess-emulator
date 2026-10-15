# M2 phase 5: data faults, library, gate, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, fault semantics per surface.** The v1 matrix (legal pairs are
  declared and validated since phase 4 D4; this phase owns the runtime
  semantics):

  | Fault | MQTT | Modbus |
  |---|---|---|
  | dropout | publishing stops for the duration | TCP sessions closed, connects refused |
  | freeze | last payloads repeat, timestamps advance | registers hold captured values |
  | timestamp_drift | device-clock field ramps away from server time | not applicable (no timestamp on the wire), rejected at load |
  | unit_error | chosen point scaled by 10^k | same, on the register |
  | nan_burst | NaN payloads on chosen points | rejected at load (u16 cannot say NaN) |
  | chatter | alarm topic flaps at given rate | alarm register bit flaps |
  | backfill_burst | dropped interval replayed with old timestamps after reconnect | not applicable, rejected at load |

  Rejecting the impossible combinations at load, with the reason, was
  chosen over silently skipping them: a scenario that claims a Modbus
  NaN burst is a scenario that misunderstands Modbus, and the parser
  saying so is documentation.
  **Confirmed in PR1, with the wire semantics made exact.** MQTT: a
  dropout stops publishing while the client stays connected to the
  broker (the retained words stay there, as they would for a device gone
  silent), and stores what the device would have sent, in order, up to
  65,536 messages; a backfill burst replays the store ahead of anything
  live, each message with the `ts` and `seq` it was born with, 512 per
  wake so the burst does not overflow the client's queue; a freeze
  repeats the value a point had when the event fired, captured by the
  task at that tick so the capture is deterministic in simulation time;
  a timestamp drift adds the drift so far to every message's `ts`,
  words and events included, and steps back when the window closes, as
  an NTP correction would; a NaN burst writes the literal `NaN`; a
  chatter is a square wave on bit 0 of the selected alarm words with a
  period of `60 / rate_per_min` simulated seconds, an edge published
  whenever a wake observes one, and the events subtree is left to the
  kernel, since a fake `raised` would need a fake `seq`. Modbus: a
  dropout resets every session on its next request (the session's
  transport is a wire the tap cuts) and accepts and closes a new
  connection at once, "refused" from the poller's side; a unit error
  saturates at the register's encoding, the pegged register being the
  audit trap; a chatter flips bit 0 of the alarm-word registers on the
  same square wave. Two rules joined the load step: a `duration` is at
  least PT1S, and a backfill burst needs a dropout on its surface that
  has ended by its offset (D4's open question in phase 4).
- **D2, freeze and unit errors take point selectors.** A state-tree path
  prefix selects what is affected (`block[3].*` freezes one block's
  telemetry). Whole-surface freeze is the prefix `*`. Random point
  choice, where a scenario wants it, is drawn at load per the phase 4
  determinism rule.
  **Confirmed in PR1, revised in two points.** The grammar is `*`,
  `site`, `block[N]` or `block[N].pcs`, spelled as targets are, because
  the surfaces publish nothing finer than a block's PCS; a container or
  a rack is refused by name. The shell matches a selector against the
  published spelling (`block02.pcs.`). Which points a kind reaches is a
  rule, not a draw: a freeze every selected point, a unit error and a
  NaN burst the cadence points only (a word is not a quantity), a
  chatter the alarm words only, and never the control registers. No
  random point choice was built: no v1 fault asks for it, and the
  architecture's surface-seeded stream does not exist, since every
  fault is a function of its parameters and the tick.
- **D3, the tap lives in the shell crate, one module per surface
  concern.** No new crate: the tap is projection behavior, and the
  projections live in `bess-emulator`. The AGENTS.md file-size contract
  applies; `mqtt.rs` and `modbus.rs` grow taps as submodules from the
  start rather than swelling in place.
  **Confirmed in PR1.** `tap.rs` (217 lines) holds what the surfaces
  share, the windows built from the schedule, the selection and the
  arithmetic of a value, a flap and a drift, and is carried by the
  snapshot as one `Arc` replaced when a window opens; `mqtt/tap.rs`
  (128) is the outbox with the store and the replay, `modbus/tap.rs`
  (106) the register transform and the wire. The publisher moved to
  `mqtt/publisher.rs` and the Modbus tests to `modbus/tests.rs` for the
  room; `mqtt.rs` is 121 lines, `modbus.rs` 134.
- **D4, library composition.** Around a dozen cases at launch, tagged
  with the EPRI categories; the exact target shares and the tolerance
  the gate holds them to are pinned from the database snapshot in this
  phase, recorded in CALIBRATION.md with retrieval date. The qualitative
  contract is fixed now: balance-of-system plus controls form the
  majority of physical-fault cases, cell-origin cases are the smallest
  class, and calendar and maintenance cases sit outside the failure gate
  entirely (they are not failures; the gate reads only `taxonomy`-tagged
  fault cases).
  **Confirmed in PR2, pinned.** The snapshot is EPRI's May 2024 white
  paper (3002030360), figure 6: controls 46%, balance of system 43%,
  cells 11% of 26 classified incidents; CALIBRATION.md carries it with
  the retrieval date and the root-cause figure beside it. Fifteen cases
  at launch, eleven failures (five `bos`, five `controls`, one `cell`),
  two calendar, two maintenance. The gate holds each class within 15
  percentage points of its share, cells the smallest and never empty:
  one case is about nine points at this size, so one case of drift
  passes and two do not, which is the accept criterion ("deleting a
  balance-of-system case or adding three cell cases fails") made exact.
  Data faults are `controls`, the sensing and communication class; the
  emulator has one cell story, the leak, so the cell class is one case
  until the model has another.
- **D5, assertion digests are per-scenario committed artifacts.** Beside
  each YAML, the digest file CI compares against, regenerated with the
  same loud discipline as golden snapshots. The digest covers final
  state and event-log hash; intermediate trajectories are deliberately
  not asserted (the M1 lesson: hold the contract, not seven hundred
  samples).
  **Confirmed in PR2, with the run length in the file.** A case states
  its `length` (schema 1 gains the optional field; an event past it is
  refused), so the file alone describes the case and the record beside
  it, `<case>.digest.json`, is what `--record` wrote and `--assert`
  compares: name, content hash, engine version, seed, start, ticks, the
  final tree's digest, the event log's count and hash, and the fiscal
  meter's closed periods, the number a calendar case is about (the
  second open question, answered yes in that form: the count, not the
  series). The engine version is part of the record, so a release
  regenerates every digest loudly, as the annual record is.
- **D6, the annual record gains an M2 sibling.** `bess-bench` runs the
  year with derating, real prices, and no scenario, regenerating the
  annual record; the M2 CALIBRATION.md entry adds the EPRI library
  composition table. Data faults stay out of the annual run: calibration
  measures the plant, not the narrator.

## Signal map impact

None beyond what phases 2 and 3 added. The map ships as 0.3.0 in the
release PR, version history row written in COMPATIBILITY.md, the alarm
meaning change called out as the break it is.

## Checkpoint impact

None. Tap state is shell state, reconstructed from the schedule on
resume; the kernel checkpoint stays observation-clean. **Confirmed in
PR1:** `Taps` is built from `Resolved` and the map; the one thing it
holds that the schedule does not, a freeze's captured values, is taken
by the task at the tick the event fires, and the MQTT store a dropout
fills is the publisher's and starts empty on a resume.

## Test plan

- **Property: the kernel cannot tell.** State digest with and without
  data faults is identical for any schedule (architecture's core claim).
- **Unit: each matrix cell.** One test per allowed fault-surface pair,
  one rejection test per disallowed pair (D1).
- **Integration: dropout with backfill.** Subscriber sees silence, then
  the burst with old device timestamps and monotone server receive
  times, which is the pipeline trap the fault exists to teach.
- **Gate: library composition** against the pinned shares (D4).
- **CI: the library runs** with `--assert` on every PR; runtime budget
  measured here, and if the dozen cases exceed a few minutes the slower
  ones move to release-tag frequency, decided on measurement, not taste
  (the M1 budget discipline).
