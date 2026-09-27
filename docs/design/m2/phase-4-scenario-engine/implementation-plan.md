# M2 phase 4: scenario engine, implementation plan

Two PRs, each landing green.

## PR1: the crate and the kernel door

- `bess-scenario`: schema v1 types, parser, validation with
  line-carrying errors (design D1 to D4).
- `bess-core`: `FaultAction` enum and `Simulation::apply` (the kernel
  door), scenario actions entering the event log.
- The player: load-time resolution to tick indices, load-time parameter
  draws, deterministic cursor.
- Checkpoint format 7 to 8 (D8).
- Tests: schema goldens and rejections, determinism with checkpoint
  round-trip, idle-cost digest equality.

Accept: the phase 2 causal-chain kernel test has a YAML twin producing an
identical event log.

## PR2: the surfaces

- CLI `--scenario` with file-wins precedence (D5).
- REST `POST /api/v1/scenario` (load and reset) and
  `GET /api/v1/scenario` (position, fired, pending); `Command` enum
  grows accordingly.
- Health endpoint and bench/Parquet metadata carry scenario name and
  content hash (D7).
- Docs: README quickstart gains a "run a scenario" step; examples/
  clients unchanged (they poll, the plant misbehaves, that is the demo).

Accept: `bess-emulator --scenario scenarios/hvac-failure-hot-week.yaml`
(the file itself lands in phase 5; a fixture stands in here) runs the
story end to end, and `GET /api/v1/scenario` narrates it live.

## Open questions

- Whether `GET /api/v1/scenario` should include the resolved schedule
  (every event with its tick) or just counts and next-up; leans resolved
  schedule, it is small and it is the reproduction record.
- Whether the WASM shell exposes scenario loading now or in the next view
  iteration. The milestone README owns that question; the crate compiles
  for WASM either way, so the cost of later is zero.
