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
- **D2, freeze and unit errors take point selectors.** A state-tree path
  prefix selects what is affected (`block[3].*` freezes one block's
  telemetry). Whole-surface freeze is the prefix `*`. Random point
  choice, where a scenario wants it, is drawn at load per the phase 4
  determinism rule.
- **D3, the tap lives in the shell crate, one module per surface
  concern.** No new crate: the tap is projection behavior, and the
  projections live in `bess-emulator`. The AGENTS.md file-size contract
  applies; `mqtt.rs` and `modbus.rs` grow taps as submodules from the
  start rather than swelling in place.
- **D4, library composition.** Around a dozen cases at launch, tagged
  with the EPRI categories; the exact target shares and the tolerance
  the gate holds them to are pinned from the database snapshot in this
  phase, recorded in CALIBRATION.md with retrieval date. The qualitative
  contract is fixed now: balance-of-system plus controls form the
  majority of physical-fault cases, cell-origin cases are the smallest
  class, and calendar and maintenance cases sit outside the failure gate
  entirely (they are not failures; the gate reads only `taxonomy`-tagged
  fault cases).
- **D5, assertion digests are per-scenario committed artifacts.** Beside
  each YAML, the digest file CI compares against, regenerated with the
  same loud discipline as golden snapshots. The digest covers final
  state and event-log hash; intermediate trajectories are deliberately
  not asserted (the M1 lesson: hold the contract, not seven hundred
  samples).
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
resume; the kernel checkpoint stays observation-clean.

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
