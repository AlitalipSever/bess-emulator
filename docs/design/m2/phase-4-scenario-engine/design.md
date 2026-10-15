# M2 phase 4: scenario engine, design

Each decision is proposed here and confirmed or revised in its PR. Pinned
on 2026-10-14 against the tree as phase 3 left it; the draft of
2026-10-05 is kept where it still holds and revised in place where the
code that landed since made it wrong (D3, D7, D8, D9 to D12 are new or
changed; the revision history is at the end). PR1 landed on 2026-10-15
and confirmed D1 to D4 and D8 to D12, D2 and D8 in revised form and D3
and D11 with additions; D5 to D7 are PR2's. The outcome is marked in
each.

## The schema, v1

```yaml
schema: 1
name: hvac-failure-hot-week
taxonomy: bos                 # phase 5's gate reads this tag
seed: 42                      # optional; the file wins over a flag (D5)
start: 2024-07-08T00:00:00Z   # optional; UTC, whole seconds
speed: 60                     # optional; 1 to 3600
events:
  - at: PT14H
    target: block[2].container[0].hvac
    fault: failure
  - at: PT38H
    target: block[2].container[0].hvac
    fault: repair
  - at: P2D
    target: block[2].container[1].rack[5]
    fault: self_discharge
    rate_pct_per_month: [10, 30]     # a range is drawn at load (D1)
  - at: P9DT6H
    target: block[2].container[1].rack[5]
    fault: isolate
  - at: PT14H20M
    surface: mqtt
    fault: dropout
    duration: PT20M
  - at: PT14H40M
    surface: mqtt
    fault: backfill_burst
```

Every field, its type and its rule:

| Field | Type | Rule |
|---|---|---|
| `schema` | integer | must be 1; any other value is rejected with the version named |
| `name` | string | 1 to 64 characters of `[a-z0-9-]`; published on `/health` and in the checkpoint |
| `taxonomy` | enum | `bos`, `controls`, `cell`, `calendar`, `maintenance`; optional, but phase 5's library requires it |
| `seed` | integer, u64 | optional |
| `start` | string | optional; `YYYY-MM-DDThh:mm:ssZ`, UTC only, no offset, no fraction; converted with `bess_data::calendar::days_from_civil` |
| `speed` | number | optional; 1 to 3600 inclusive, the shell's own clamp made a rule |
| `events` | list | may be empty (an empty scenario is the plain run with a name) |
| `events[].at` | string | ISO 8601 duration, `PnDTnHnMnS` with any subset of the parts, whole seconds only; `P1W` is accepted as seven days; months and years are refused (a DST year makes them ambiguous) |
| `events[].target` | string | a state-tree path (D2); present on a physical event, absent on a data event |
| `events[].surface` | enum | `mqtt`, `modbus`; present on a data event, absent on a physical event |
| `events[].fault` | string | one of the names in D3 for the target's kind, or in D4 for the surface |
| `events[].duration` | string | ISO 8601 duration as `at`, at least PT1S; required, optional or forbidden per fault (D4) |
| `events[].rate_pct_per_month` | number or `[lo, hi]` | `self_discharge` only; 0.1 to 100; a range draws uniformly at load |
| `events[].points` | string | data faults `freeze`, `unit_error`, `nan_burst`, `chatter`; a path prefix per phase 5 D2, `*` for the whole surface; default `*` |
| `events[].exponent` | integer | `unit_error` only; the power of ten; default 3; must not be 0 |
| `events[].rate_per_min` | number | `chatter` only; flaps per minute, 0.1 to 60 |
| `events[].drift_s_per_h` | number | `timestamp_drift` only; seconds of device-clock drift per hour, nonzero |

An event carries exactly one of `target` and `surface`. Unknown fields
are refused at every level (`deny_unknown_fields`): a misspelled parameter
is a scenario that does not do what its author thinks, and the parser
saying so is the cheapest possible review.

## Decisions

- **D1, offsets, not clock times.** `at` is an ISO 8601 duration from
  `start`. The M0 sketch used clock times ("14:00"); offsets won because
  they are unambiguous across DST days, which phase 3 made real. A
  calendar scenario says what it means by choosing `start`. The parser
  is the crate's own (`schema/duration.rs`, a grammar of five tokens);
  no date-time crate enters the workspace for a format this small.
  Resolution: an offset of `n` seconds is tick `n`, fired before the
  kernel steps that tick, so `PT0S` acts on the initial state exactly as
  the chain test's `set_hvac_failed` before its first step does.
  **Confirmed in PR1:** `schema/duration.rs` reads `PnW` and `PnDTnHnMnS`
  in any subset, whole seconds; months, years, fractions, negatives and a
  time part without `T` are refused by name. Weeks stand alone, as ISO
  8601 has them: `P1W2D` is refused with a pointer to days.

- **D2, target paths are state-tree paths.** Bracket-indexed tree
  addresses, validated at load against `PlantConfig`:

  | Path | Kind | Indices checked against |
  |---|---|---|
  | `site` | site | nothing |
  | `block[N]` | block | `blocks` |
  | `block[N].pcs` | pcs | `blocks` |
  | `block[N].container[C].hvac` | hvac | `blocks`, `containers_per_block` |
  | `block[N].container[C].rack[R]` | rack | `blocks`, `containers_per_block`, `racks_per_container` |

  This is deliberately not the flattened point-name spelling the surfaces
  publish (`block02.pcs.p_ac_kw`): a point is an aggregated projection,
  while a scenario addresses the tree itself, container and rack
  included. Indices are zero-based and unpadded, as in the tree and in
  `AlarmNode`; the surfaces' `block02` spelling is theirs. A rejection
  says what the site has at that level ("block[25]: this site has
  block[0] to block[19]"; "block[2].container[0].fan: a container has
  `hvac` and `rack[0]` to `rack[11]`"), which is what keeps the two
  spellings from becoming a trap. `container[C]` without a child is not
  a target: nothing acts on a container as a whole in v1.
  **Confirmed in PR1, one step instead of two:** `Target::parse(path,
  cfg)` resolves and range-checks together, because the refusal text needs
  the site's range at whichever level failed. The messages are the ones
  above, held by the module's tests; a padded index (`block[02]`) is
  refused too.

- **D3, physical fault set v1**, the `FaultAction` enum, one row per
  (kind, fault) pair the validator accepts:

  | Target kind | `fault` | `FaultAction` | Kernel call | Built in |
  |---|---|---|---|---|
  | hvac | `failure` | `HvacFailure { block, container }` | `set_hvac_failed(.., true)` | phase 2 |
  | hvac | `repair` | `HvacRepair { block, container }` | `set_hvac_failed(.., false)` | phase 2 |
  | pcs | `trip` | `PcsTrip { block }` | `trip_pcs` | phase 3 |
  | site | `protection_trip` | `ProtectionTrip` | `trip_protection` | phase 3 |
  | rack | `self_discharge` | `RackSelfDischarge { block, container, rack, excess_ppm_per_day }` | sets the rack field | this phase |
  | rack | `isolate` | `RackIsolate { block, container, rack }` | `set_rack_in_service(.., false)` | phase 3 |
  | rack | `restore` | `RackRestore { block, container, rack }` | `set_rack_in_service(.., true)` | phase 3 |
  | block | `maintenance_enter` | `BlockMaintenanceEnter { block }` | `set_block_mode(.., Maintenance)` | phase 3 |
  | block | `maintenance_exit` | `BlockMaintenanceExit { block }` | `set_block_mode(.., InService)` | phase 3 |
  | site, block, rack | `alarm_reset` | `AlarmReset(ResetScope)` | `reset_alarms` | phase 2 |

  All but one are operations phase 2 or 3 built; the engine adds none of
  its own physics. The exception is `self_discharge`, which neither phase
  built (phase 3's plan records why), so this phase owns it: a per-rack
  excess self-discharge rate in the rack state, added to the time term of
  the phase 1 spread law, so one cell leaking faster than its neighbours
  widens the rack's spread over days, outruns the bleed at the top of the
  window, and surfaces through the phase 2 imbalance bits. The action
  carries the rate as an integer, parts per million of SoC per day
  (`1 %/month` is 333 ppm/day), so that `FaultAction`, and with it
  `Event`, stays `Copy + Eq` and the event digest never hashes a float's
  formatting; the YAML speaks in percent per month because datasheets do
  (EVE MB31: 3 %/month for a healthy cell). The kernel turns the integer
  into the rack's `self_discharge_excess_per_s` with one formula at apply
  time. The `still_present` list a reset returns is dropped by `apply`:
  the bits raise again on the next tick, which the log shows. Frequencies
  and combinations are library material (phase 5), not engine material;
  an external setpoint is a control, not a fault, and stays a REST and
  Modbus matter.
  **Confirmed in PR1, with two additions.** `FaultAction` carries the
  ten rows and `FaultAction::name()` their file names, shared by the
  parser and the MQTT payload. An accepted action that changes nothing (a
  second trip, a repair of a working unit) answers `false` and is still
  marked, since the log records what was done. The spread law now stops
  the spread at 1.0, the whole window, which the fastest leak the schema
  allows would otherwise pass in a month; below that ceiling nothing
  moves. Measured in CALIBRATION.md: the bleed absorbs a leak up to about
  10 %/month, and above it ten times the healthy drift reaches the
  imbalance warning on day 20. The example's `[10, 30]` range therefore
  straddles the point where the fault becomes visible, which phase 5's
  library should know when it picks its cases.

- **D4, data fault set v1 in the schema now, executed in phase 5.** The
  validator enforces this matrix from this phase, with the reason in the
  rejection; phase 5 owns the runtime semantics:

  | `fault` | `mqtt` | `modbus` | `duration` | Parameters |
  |---|---|---|---|---|
  | `dropout` | yes | yes | required | none |
  | `freeze` | yes | yes | required | `points` |
  | `unit_error` | yes | yes | optional (none: to the end of the run) | `points`, `exponent` |
  | `chatter` | yes | yes | required | `points`, `rate_per_min` |
  | `timestamp_drift` | yes | no: Modbus has no wire timestamp | required | `drift_s_per_h` |
  | `nan_burst` | yes | no: a u16 cannot say NaN | required | `points` |
  | `backfill_burst` | yes | no: a register bank has no replay | forbidden | none |

  Declaring the set and the matrix now freezes the file format so phase 5
  does not churn the library. The parameter names are phase 5's to
  confirm when the tap gives them meaning; until then they are checked
  for presence and range only. Whether a `backfill_burst` must follow a
  `dropout` on the same surface is a phase 5 rule.
  **Confirmed in PR1.** `rate_per_min` (chatter) and `drift_s_per_h`
  (timestamp drift) are required for their faults; `points` defaults to
  `*` where taken, `exponent` to 3. A parameter given to a fault that
  does not take it is refused, and a physical event takes no
  `duration`.

- **D5, the file wins.** Scenario-stated seed, start and speed are
  authoritative; an explicit CLI flag that disagrees is an error naming
  both values. Flags the file does not state keep their CLI meaning, and
  an explicit flag that agrees with the file is fine. "Explicit" means
  given on the command line: `Args` carries the three as `Option` and the
  defaults (42, 2026-01-01, 60) are applied by the resolution step, not
  by clap, so a default can never collide with a file. Over REST there is
  no flag to conflict with: the file's values apply and the shell's
  current values stand in for what the file leaves out.
  **Confirmed in PR2.** `cli/run_params.rs` applies the table; the three
  flags are `Option` and the defaults (42, 1767225600, 60) live there.
  The refusal reads `--seed 7 conflicts with the scenario's seed, 42;
  drop the flag or change the file`. Over REST the plant's current seed,
  start and speed, read from the latest snapshot, stand in for what the
  file leaves out.

- **D6, loading over REST resets the run.** A scenario describes a run
  from its start; splicing events into a running plant would break the
  reproducibility story that is the engine's whole value. The simulation
  task replaces its `Simulation` with a fresh one at the scenario's seed
  and start, the event log starts again at `seq` 1, and every surface
  sees tick 0 next (retained MQTT words republish on their next change,
  as after a reconnect). A "fire one action now" debug endpoint was
  considered and rejected for M2: the kernel's `apply` covers tests, and
  an operator door for unscripted faults can wait until someone asks.
  There is no unload endpoint either: loading another file is the way to
  change the story, and restarting the process the way to leave it.
  **Confirmed in PR2, one point made explicit:** the external setpoint
  does not survive a load. It is a control on the old plant, not part of
  the file, and a fresh plant that started under a setpoint the file's
  author never saw would not be the file's run. The speed stays the
  shell's where the file states none. The tick 0 snapshot goes out
  before the load is answered, so a client reading `GET` after the
  `200` never sees the old plant.

- **D7, scenario identity in the output, revised.** `/health` carries
  `scenario: {"name", "content_hash"}` or `null`, next to the kernel and
  map versions, and `GET /api/v1/scenario` carries the same two fields
  with the schedule. The content hash is FNV-1a 64 over the file's bytes
  as given, printed as `0x` and sixteen hex digits, the same function the
  checkpoint digest and the dataset pins use (`bess_core::digest`, shared
  from this phase). The draft also named the bench and "Parquet
  metadata": there is no Parquet anywhere in the workspace, the bench's
  record is JSON, and the bench does not run scenarios in M2 (phase 5
  D6), so `RunRecord` is unchanged.
  **Hash confirmed in PR1:** `Scenario::id` carries the name and the
  FNV-1a 64 of the bytes as given, from `bess_core::digest`.
  **Surfaces confirmed in PR2.** `/health` reads the pair from the tree's
  `meta.scenario`, so it is the run's identity as a checkpoint would
  store it, not the shell's memory of a load. `GET /api/v1/scenario`
  carries the schedule in full, each target spelled as the file spells
  it (`Target::of` and `Display`, added for this) and each event marked
  fired or not; the tick is the snapshot's.

- **D8, checkpoint format 8, revised.** `SiteMeta` gains
  `scenario: Option<ScenarioId { name, content_hash }>`: the scenario is
  run identity, and that struct is where run identity lives. `RackState`
  gains `self_discharge_excess_per_s` (D3). Format 7 files are rejected by
  version. The draft put the player's cursor and a schedule hash in the
  tree as well; neither is needed. The schedule is a pure function of
  (file bytes, seed, configuration) and the cursor a pure function of the
  schedule and the tick, so a resumed run rebuilds both from the file and
  the tree. What a resume must check is that it has the same file, and
  `bess_scenario::Resolved::resume(SiteState)` refuses a tree whose
  `scenario` hash is not its own, or that carries no scenario at all.
  **Confirmed in PR1, revised in one point:** `Resolved::resume` also
  refuses a tree whose seed is not the one the schedule was drawn with,
  since a range drawn under another seed is another schedule. It takes
  the configuration and models as `simulation` does. `Player::catch_up`
  rebuilds the position from the tree's tick; the determinism test
  resumes at the tick a reset is due and matches the continuous run.

- **D9, the YAML parser (needs Ali's yes before `cargo add`).** The
  workspace has no YAML dependency. `serde_yaml` is deprecated since
  March 2024 (archived at 0.9.34), and `serde_yml`, its first fork, is
  itself deprecated and carries an unsoundness advisory, so neither is an
  option. Candidates on 2026-10-14:

  | Crate | What it is | For | Against |
  |---|---|---|---|
  | `serde-saphyr` 2.0.0 | pure-Rust serde deserializer on the saphyr parser family, `unsafe` forbidden, MIT or Apache-2.0, released October 2026, wasm32 in its CI | errors carry line and column with a snippet; duplicate keys rejected by default; no `unsafe`, which matches the workspace's own `forbid`; serde derive with `deny_unknown_fields` | deserialization only (this phase never writes YAML); newer code base than the C-derived ones |
  | `serde_norway` 0.9.42 | maintained fork of `serde_yaml`, drop-in, MIT or Apache-2.0, released October 2026 | the API everyone knows; `Error::location()` | runs on `unsafe-libyaml-norway`, an auto-translated C parser full of `unsafe`; inherits a design its own author retired |
  | `yaml-rust2` 0.9 | maintained fork of `yaml-rust`, pure Rust, no serde | smallest surface; markers on every node | the schema would be mapped by hand, which is the work serde exists to avoid |
  | `noyalib` | newer pure-Rust parser with a serde-compatibility feature | pure Rust | little track record yet |

  Proposal: `serde-saphyr`, pinned to its 2.x line, as a dependency of
  `bess-scenario` only. No build script, no network, no telemetry: it is
  a parser. The install step checks `cargo tree -p bess-scenario` for
  what it pulls in and that nothing in that set carries a `build.rs`
  that reaches the network. If the crate turns out to lack something the
  schema needs (spans on values, say), `serde_norway` is the fallback
  and the design does not change.
  **Confirmed in PR1 (Ali's yes, 2026-10-15):** `serde-saphyr` 2.0.0
  with `default-features = false, features = ["deserialize"]`, in
  `bess-scenario` only. It and its parser `granit-parser` forbid
  `unsafe`; what they pull in has two build scripts (num-traits' autocfg
  probe, encoding_rs's rustversion cfg), neither reaches the network.
  The encoding layer (`encoding_rs`, `simdutf8`) uses `unsafe` for SIMD,
  as everywhere it is used. Spans on values exist (`Spanned<T>`): each
  event is read spanned, so a semantic rejection carries the line its
  event starts on as well as `events[i].field`.

- **D10, `bess-scenario` depends on `bess-data` for the calendar.**
  `start` is a civil UTC timestamp; turning it into unix seconds needs
  `days_from_civil`, which lives in `bess-data/src/calendar.rs` since
  phase 3 PR2 because two compilers needed it. A third copy in a third
  crate is the thing that PR refused; the alternative, moving the
  calendar into `bess-core`, is a cross-crate refactor that no fourth
  caller yet justifies and is recorded here as the move to make when one
  appears. The artifacts `bess-data` bundles come along; the crate is
  still WASM-clean (`bess-models` has carried them into the browser
  build since M1).
  **Confirmed in PR1:** `validate.rs` converts `start` with
  `days_from_civil` and checks the date with its inverse.

- **D11, `Event::FaultApplied(FaultAction)`.** Every action `apply`
  accepts goes out as an event with the next tick, ahead of its
  consequences, through `pending_events` like a reset's clears. Phase 3's
  commands added no event kind because every command of that phase
  already spoke through a mirror bit or a PCS transition; self-discharge
  has no mirror (a BMS does not know a cell leaks, it sees the spread),
  and more generally the event log is the determinism contract's record
  of what happened, which from this phase includes what was done. This is
  the one new `Event` variant of the milestone after phase 2's two; code
  matching on `Event` exhaustively has one more arm to write, and the
  milestone README's inventory says so. `FaultAction` derives
  `Copy, Eq, Hash, Serialize, Deserialize` so `Event` keeps its derives;
  that is why D3's rate is an integer. On MQTT the event publishes under
  the node it acted on, `events/<node>/fault`, with payload `{"seq",
  "ts", "node", "event": "fault", "fault": "<name as in the YAML>",
  "rate_ppm_per_day"?: n, "scope"?: "<node>"}`; a `ProtectionTrip` and a
  site reset act on `site`.
  **Confirmed in PR1, one spelling added:** an HVAC unit has no alarm
  word and so no `AlarmNode`; its fault publishes under
  `events/block02/container0/hvac/fault`, the path the tree gives it,
  beside the rack paths. A reset's `scope` is the reached node's path.
  The arms and their tests landed with the variant, since the match is
  exhaustive.

- **D12, the schedule and the draws.** `Resolved` holds the events sorted
  by (tick, file index), so two events due on one tick fire in file
  order. A range-valued parameter is drawn once at resolution from
  `Rng::from_seed(seed ^ SCENARIO_STREAM ^ index)` with a salt of its own,
  like the commissioning-spread stream in `state/init.rs`, so the draws
  neither disturb nor depend on the kernel's stream. Two events on the
  same target may not share a tick (which of two actions on one node
  wins is not a question the engine should answer) and must appear in
  the file in time order per target (a reader groups by target, and a
  file that lists a repair before its failure is a file with a mistake
  in it); the file as a whole need not be sorted.
  **Confirmed in PR1:** the salt is `SCENARIO_STREAM` in `resolve.rs`;
  the example's leak draws 6,962 ppm/day (20.9 %/month) under seed 42,
  pinned in `tests/goldens.rs`. The same-tick and order rules apply to
  physical targets; data events on one surface are phase 5's to rule on,
  as D4 already leaves the dropout and backfill relation to it.

## Signal map impact

None. The scenario surface is REST and CLI; the plant's registers do not
know they are in a story, which is the point. The MQTT events subtree
gains the `fault` payload (D11), an addition.

## Checkpoint impact

Format 7 to 8 (D8): `SiteMeta::scenario`, `RackState::self_discharge_excess_per_s`.

## Golden digest

Moves once, in PR1, for schema alone: the tree gains the two fields, and
with them set aside it is identical to phase 3's, value for value. The
annual record does not move: the plain run has no scenario and no rack
leaks. **Confirmed in PR1:** `0xa24f_1646_1b6a_d90e` to
`0xf3fb_8ad4_75c9_980f`; the new tree with `"scenario":null` and the 480
`"self_discharge_excess_per_s":0.0` removed from its serialization hashes
to the old value, and `bess-bench --check` passes without regeneration.

## Test plan

- **Determinism:** same tuple twice, byte-identical state and event log,
  with and without a mid-scenario checkpoint round-trip (architecture
  invariant 1); a checkpoint under one file refuses to resume under
  another, and under no file.
- **Unit: durations.** The grammar's accepted forms and its refusals
  (fractions, months, years, missing `T` before time parts, negative).
- **Unit: targets.** Every row of D2 parses; every malformed or
  out-of-range path is refused with the site's actual range in the
  message.
- **Unit: schema.** Golden parse of the example above into the expected
  `Resolved`; rejection cases, each checked for its position and reason:
  unknown schema version, unknown field, both `target` and `surface`,
  neither, wrong fault for the kind, wrong fault for the surface, a
  forbidden or missing `duration`, a parameter out of range, two events
  on one target sharing a tick, out of order per target, `speed` outside
  the clamp, `start` with an offset.
- **Unit: precedence.** Each cell of the file-versus-flag table (D5):
  file only, flag only, both equal, both different, neither.
- **Unit: the kernel door.** For every `FaultAction` variant, `apply`
  leaves the tree exactly as the direct command does, plus the
  `FaultApplied` event in `pending_events`; `false` on a no-op; a node
  the site lacks is refused with nothing changed.
- **Unit: self-discharge.** The excess term widens the spread at the
  stated rate on an idle rack; a healthy rack's spread is unchanged by
  the field at zero; a leaking rack at the top of the window reaches the
  imbalance warning in days while a healthy neighbour does not.
- **Integration: the twin.** The phase 2 causal-chain test re-expressed
  as a YAML scenario (the same prepared hot plant, the two HVAC failures
  as events `at: PT0S`) produces, played, the same event log as the same
  plant scripted through `apply`, and with the `FaultApplied` events
  removed the log the direct commands produce. The engine adds
  sequencing and a mark, nothing else.
- **Idle cost:** a loaded scenario with no events against the plain run:
  same event-log digest, same tree once `meta.scenario` is set aside.
- **Surfaces:** `POST` with the example loads and resets (tick back to 0,
  `seq` back to 1), answers 200 with the status; a malformed body is 400
  with the position, a well-formed file naming `block[25]` is 422; `GET`
  before any load says so; after N ticks `fired` and `pending` agree with
  the schedule; `/health` carries the name and hash; the `fault` payload
  on the stream names the node and the fault.
- **Determinism digest:** `GOLDEN_DIGEST` regenerated once (schema),
  called out in the constant's comment and the PR.

## Revision history

- 2026-10-05: draft, D1 to D8.
- 2026-10-14: pinned against the tree after phase 3. D3 gains the
  (kind, fault) table and the integer rate; D5 says what "explicit"
  means and what REST does; D7 drops the bench and the Parquet mention;
  D8 drops the cursor from the checkpoint and adds `ScenarioId` to
  `SiteMeta`; D9 to D12 added (parser, calendar dependency, the event
  variant, the schedule rule). Error positions revised from "line" to
  line and column for parse errors and event index and field for
  semantic ones.
- 2026-10-15: PR1 outcomes marked in D1 to D12; the
  semantic positions gain the event's line (D9).
- 2026-10-15, PR2: D5 to D7 confirmed; D6 says what happens to the
  external setpoint; a data fault's `duration` must be at least a second
  (D4's table, a PR1 review finding).
