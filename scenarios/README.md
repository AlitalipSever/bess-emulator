# The scenario library

One YAML file per case, each a reproducible story the reference plant
GW-01 can be made to play: `bess-emulator --scenario scenarios/<case>.yaml`
plays it live on every surface, `--assert scenarios/<case>.digest.json`
plays it headless and holds the result to the committed record. The file
format is `docs/design/m2/phase-4-scenario-engine/design.md`; what each
fault does to each surface is in COMPATIBILITY.md.

## The contract

Every case in this directory:

- **states what it demonstrates** in its header comment, and what a
  consumer should observe on which surface, so a reader can pick a case
  by the lesson, not the file name;
- **carries a `taxonomy` tag**: `bos`, `controls` or `cell` for a failure
  case, following the failed-element classes of EPRI's BESS Failure
  Incident Database, or `calendar`, `maintenance`, `grid` or `market`
  for a story that is not a failure of the plant;
- **states its `length`**, how long the story is, so a headless run knows
  when to stop, and its `seed` and `start`, so the run is the same one
  every time;
- **has a record beside it**, `<case>.digest.json`, written by `--record`
  and compared by `--assert`: the digest of the final tree, the event
  log's count and hash, and the fiscal meter's closed periods. CI runs
  every case against its record on every pull request. A record moves
  for one of two reasons: a deliberate physics change, which regenerates
  it with the same loud discipline as the annual calibration record, or
  a regression, which is what the gate is for.

## The gate

The failure cases are held to the public incident record. EPRI's 2024
analysis of its database classified 26 incidents by the element that
failed: controls 46%, balance of system 43%, cells 11%. The library's
`bos`, `controls` and `cell` cases keep each share within 15 percentage
points of that, with the cell class the smallest and never empty; the
calendar and maintenance cases sit outside the gate, since they are not
failures. The snapshot, its retrieval date and the library's composition
are pinned in CALIBRATION.md, and `bess-scenario`'s `library` test holds
the directory to them, with the composition numbers in its message when
a case is added or removed on the wrong side.

## The cases

| Case | Taxonomy | Length | What a consumer sees |
|---|---|---|---|
| `hvac-failure-hot-week` | bos | 10 days | the hottest week: one container's cooling fails, its racks derate and the block misses its setpoint, but one container is not enough to limit the site; a repair, then a leaking rack and its isolation; a dropout with backfill on the first afternoon |
| `hvac-failure-both-units` | bos | 30 h | both units of a block fail overnight; the next evening's discharge on the warmest day: over-temperature, derate, setpoint miss, site limit, minutes apart |
| `pcs-trip-and-reset` | bos | 3 h | a converter trips in the evening discharge, the other nineteen carry the site on their overload budgets for twenty minutes, then it falls one block short; an operator reset brings it back |
| `pcs-overload-budget` | bos | 2 h | two converters trip on the warmest evening; the eighteen left run at 110 % for exactly ten minutes, then fall to their rating in one tick and the site reports its limit |
| `protection-trip-staggered-return` | bos | 2 h | the breaker opens, the POI reads zero, the reset starts the staggered return, block by block |
| `hvac-failure-winter-night` | bos | 1 day | a container loses its heater on a January night and cannot take the morning charge |
| `scada-dropout-with-backfill` | controls | 2 h | silence, then the store replayed with old device timestamps ahead of the live stream |
| `frozen-scada-day` | controls | 1 day | one block's telemetry holds on both surfaces while the block keeps cycling |
| `unit-error-audit-trap` | controls | 4 h | the site's quantities a thousand times too large; pegged Modbus registers |
| `alarm-chatter-storm` | controls | 2 h | a block's alarm words flap twenty times a minute; the kernel raised nothing |
| `clock-drift-and-nan` | controls | 8 h | MQTT timestamps run ahead and step back; two minutes of `NaN` payloads |
| `curtailment-order` | controls | 1 h | a grid operator caps the plant at 60 % for forty minutes in the evening discharge: the walk down at 10 %/min, `external_limitation` raised and `power_limited` not, the revert and the walk back |
| `rack-self-discharge-drift` | cell | 6 days | one leaking cell widens a rack's spread to the imbalance warning on day 5 |
| `dst-spring-forward` | calendar | 23 h | the short local day: 23 planned hours, 92 closed quarter-hours |
| `dst-fall-back` | calendar | 25 h | the long local day: 25 planned hours, 100 closed quarter-hours, the 02:xx quarters twice |
| `block-maintenance-window` | maintenance | 1 day | a block out for a working day, its standby tare gone, handed back at 16:00 |
| `rack-isolation-and-restore` | maintenance | 2 days | a rack isolated for a day of cycling and restored with the state of charge it was left with |
| `substation-earthing-work` | maintenance | 3 h | the HV breaker opened by hand, the bus isolated and earthed for two hours of work, then the sequence back and the staggered return |
| `fcr-block` | market | 4 h | a four-hour FCR block of 20 MW on an idle summer night: the droop answers the frequency within seconds, the working range is published, the dead-band recentering nudges the state of charge, the award clears at 04:00 |
| `frequency-event` | grid | 2 h | the grid dips 300 mHz and the plant raises four megawatts at once, rises 400 mHz and it cuts eight, falls to 47.4 Hz and the kernel trips the site; the reset and the return after |

Twenty cases: thirteen failure cases (six `bos`, 46%; six `controls`,
46%; one `cell`, 8%) and seven that are calendar, maintenance, grid or
market.

## Adding a case

Write the file, give it a header, a tag and a length, record it with
`cargo run --release -p bess-emulator -- --scenario scenarios/<case>.yaml
--record scenarios/<case>.digest.json`, add its row above, and run
`cargo test -p bess-scenario --test library` to see whether the
composition still holds. A data-fault case's record is the same with
or without its data events, since the kernel cannot tell; its lesson
is on the wire, and the record is there so the physics underneath
cannot move unnoticed either.
