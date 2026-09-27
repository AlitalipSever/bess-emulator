# M2 phase 4: scenario engine, design

Each decision is proposed here and confirmed or revised in its PR.

## The schema, v1

```yaml
schema: 1
name: hvac-failure-hot-week
taxonomy: bos            # EPRI category, used by the phase 5 gate
seed: 42
start: 2024-07-08T00:00:00Z
speed: 60
events:
  - at: PT14H
    target: block[2].container[0].hvac
    fault: failure
  - at: PT38H
    target: block[2].container[0].hvac
    fault: repair
  - at: PT14H20M
    surface: mqtt
    fault: dropout
    duration: PT20M
```

## Decisions

- **D1, offsets, not clock times.** `at` is an ISO 8601 duration from
  `start`. The M0 sketch used clock times ("14:00"); offsets won because
  they are unambiguous across DST days, which this milestone makes real.
  A calendar scenario says what it means by choosing `start`.
- **D2, target paths are state-tree paths.** The same addressing the MQTT
  topics use, validated at load against `PlantConfig`. One addressing
  scheme everywhere is worth more than a friendlier alias syntax.
- **D3, physical fault set v1** (the `FaultAction` enum): `hvac: failure`
  and `repair`, `pcs: trip`, `protection: trip`, `rack: self_discharge`
  with a rate, `rack: isolate` and `restore`, `block: maintenance_enter`
  and `exit`, `alarms: reset`. Each is an observable operation phase 2 or
  3 built; the engine adds none of its own physics. Frequencies and
  combinations are library material (phase 5), not engine material.
- **D4, data fault set v1 in the schema now, executed in phase 5:**
  `dropout`, `freeze`, `timestamp_drift`, `unit_error`, `nan_burst`,
  `chatter`, `backfill_burst`, each with `surface: mqtt | modbus` and
  parameters per fault. Declaring them now freezes the file format so
  phase 5 does not churn the library.
- **D5, the file wins.** Scenario-stated seed, start, speed are
  authoritative; conflicting CLI flags are an error (architecture
  rationale: reproductions must not drift silently). Flags the file does
  not state keep their CLI meaning.
- **D6, loading over REST resets the run.** A scenario describes a run
  from its start; splicing events into a running plant would break the
  reproducibility story that is the engine's whole value. A "fire one
  action now" debug endpoint was considered and rejected for M2: the
  kernel's `apply` covers tests, and an operator door for unscripted
  faults can wait until someone asks.
- **D7, scenario identity in the output.** The health endpoint and the
  bench/Parquet metadata carry the scenario name and the file's content
  hash next to the seed and dataset version, completing the reproduction
  triple as an observable fact.
- **D8, checkpoint format 7.** Player cursor and the resolved schedule
  hash go into the checkpoint; a checkpoint taken mid-scenario refuses to
  resume under a different scenario file (hash mismatch), the same
  pinned-artifact discipline as the datasets.

## Signal map impact

None. The scenario surface is REST and CLI; the plant's registers do not
know they are in a story, which is the point.

## Checkpoint impact

Format 6 to 7 (D8).

## Test plan

- **Determinism:** same tuple twice, byte-identical, with and without a
  mid-scenario checkpoint round-trip (architecture invariant 1).
- **Unit: schema.** Golden parse of the example above; rejection cases
  (bad target, wrong fault for target kind, non-monotone times, unknown
  schema version) each with line-carrying errors.
- **Unit: precedence.** File-versus-flags conflicts rejected (D5).
- **Integration:** the phase 2 causal-chain test re-expressed as a YAML
  scenario produces the same event log as its direct-`apply` twin, which
  proves the engine adds sequencing and nothing else.
- **Idle cost:** empty-scenario run digest equals plain-run digest.
