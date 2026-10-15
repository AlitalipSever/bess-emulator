# v0.5.0: M2, the plant learns to misbehave

Draft of the release note, edited from the inventory in [README.md](README.md).
The maintainer creates the tag with it:
`gh release create v0.5.0 --title "v0.5.0: M2, the plant learns to misbehave" --notes-file docs/design/m2/release-notes.md`
(drop this preamble first).

---

Through v0.4.0 the plant always worked. Now it derates when its cells are
hot, drifts apart and balances, raises alarms with causes behind them, goes
into maintenance, trips and comes back block by block, plans real days at
real prices, and does any of it on demand from a YAML file, reproducibly,
while its telemetry can lie the way real telemetry lies and the physics
underneath stays right.

## Highlights

- **A BMS with limits that mean something.** Each rack's charge and
  discharge limit follows cell temperature through the cell maker's own
  table (EVE MB31): nothing charges below 0 C, nothing flows above 60 C.
  Racks drift apart as cells do, and passive balancing bleeds them back
  inside a published window; the bleed heat is booked where it goes, and
  the year's round trip moved from 84.22% to 84.17% for it.
- **Alarms with causality.** Three alarm words, rack, block and site, with
  a frozen bit layout, latching where a real BMS latches, and an operator
  reset over REST that answers with what it could not clear. The phase's
  acceptance is a causal chain the tests hold: a cooling failure on a hot
  afternoon, then derate, then a missed setpoint, then the site's limit,
  minutes apart. Every raise and clear is an event on every surface.
- **Events became a surface.** MQTT publishes the alarm words retained and
  on change, and every raise, clear, PCS transition and applied fault
  under `bess/gw01/events/`, numbered by the kernel's log; Modbus carries
  the words and an event counter; the WebSocket stream carries the events
  since its last message; Prometheus counts them.
- **Availability tells the truth.** Maintenance mode, isolated racks, a
  tripped converter, the site's protection trip and the staggered return
  behind it, block k released k x 30 s later: the available power on
  every surface sums the blocks that can actually deliver, and reads zero
  behind an open breaker, with the house load on a third meter.
- **Real prices over real days.** The planner ranks the 2024 German
  day-ahead series (SMARD, CC BY 4.0, bundled) one Europe/Berlin day at a
  time, the DST days over 23 and 25 hours, and a fiscal meter closes
  35,040 quarter-hours a year, 92 and 100 on those two days. The annual
  round trip moved to 83.66%, almost all of it the market rule meeting
  real prices; the cost is itemized rather than tuned away, and a planner
  that orders its hours is M4's.
- **The scenario engine.** `bess-emulator --scenario s.yaml`, or `POST
  /api/v1/scenario` with the file as the body, plays a schema 1 YAML file
  from tick 0: ISO 8601 offsets, tree-path targets validated against the
  site, ten physical faults through one kernel door, ranges drawn once
  from a stream of their own. `GET /api/v1/scenario` narrates the whole
  resolved schedule with every drawn value, `/health` names the file, and
  the checkpoint carries its hash so a resume refuses another story.
- **Telemetry that lies.** Seven data faults on MQTT and Modbus: a dropout
  that stores what it could not send and a backfill burst that replays it
  with old device timestamps, frozen values, unit errors that peg a
  register, alarm chatter, a drifting clock, `NaN` in a payload. REST, the
  stream, Prometheus and the checkpoint stay truthful on purpose, so the
  gap between what the plant did and what SCADA saw is there to measure.
  The kernel cannot tell, and a test says so.
- **A library held to the incident record.** Fifteen cases in
  `scenarios/`, each with a header that says what to watch, a taxonomy
  tag, a length and a committed digest; `--assert` plays a case headless
  against its record, CI plays all of them on every pull request, and a
  gate holds the failure mix to EPRI's 2024 analysis of its failure
  incident database (controls 46%, balance of system 43%, cells 11%).

## Breaking changes

Pre-1.0, so these are called out rather than avoided. The full list, one
entry per change an integrator against v0.4.0 can trip over, is the
inventory in `docs/design/m2/README.md`.

**Signal map 0.3.0, the first announced break.** `blockNN.alarm_bits` and
`site.alarm_count` stop reading zero; the rack words behind the first have
a layout, the second counts every set bit on site. Both move to the new
publication class `event`: on MQTT they publish when they change, retained,
instead of on a cadence. The scale column of the kW and kWh points in the
CSV was corrected from 0.001 to 1 (the registers are unchanged). Additions:
the block and site alarm words, the event counter, the rack spread, the
derate count, block mode, the station-service meter, the fiscal meter's
period points, in a second per-block range at 2000 + 10 x block.
COMPATIBILITY.md has the table.

**Checkpoint format 8**, up from 3, bumped at every schema-changing phase;
v0.4.0 files are rejected by version, no migration pre-1.0.

**Kernel.** `Event` gains `AlarmRaised`, `AlarmCleared` and
`FaultApplied(FaultAction)`, so an exhaustive `match` has three arms to
write, and `Event` moved to `kernel/event.rs` (re-exported). The commands
`set_rack_in_service`, `set_block_mode`, `trip_pcs`, `trip_protection` and
the fault door `Simulation::apply` return a `Result`; `set_hvac_failed`
does too, instead of panicking on an unknown container. `DayAheadEms::
default_profile` is gone; `from_price_year` replaces it. `Simulation::
reset_alarms` is new.

**Shell.** `--seed`, `--start-unix` and `--speed` are optional with the
same defaults; a scenario's own values win over them, and a flag that
disagrees with the file is an error. MQTT publishes telemetry at all now:
through v0.4.0 the publisher connected and never published a cadence point
(an overflow in its first timestamp check), so anyone who wired a
subscriber and saw nothing was right.

**Additive.** `POST /api/v1/alarms/reset`; `POST` and `GET
/api/v1/scenario`; `scenario` on `/health`; `--scenario`, `--record` and
`--assert`; the `bess-scenario` crate; `bess_core::digest`; new Prometheus
families and Grafana dashboard version 4; `alarm_bits`, `event_count`,
`mode`, the station-service and revenue fields in the summary and the
stream.

## Honest limits at M2

The SoC realism chain is not here: open-circuit hysteresis, reported versus
true SoC, measurement imperfections are M2.5, designed in `docs/design/m2_5/`.
The rack mean SoC does not self-discharge; only the spread drifts, and the
one leak the engine injects widens the spread rather than draining the
rack. The five-state PCS machine, the second announced break, is M3. A
planner that orders charge before discharge, or looks at state of charge,
is M4, which is why 28 of the 366 local days export nothing. The MQTT
dropout keeps its broker session and goes quiet; dropping off the broker
with a last will is a refinement for when a scenario needs the broker's
view. The scenario engine does not reach the browser build yet. Everything
measured, with sources and gaps, is in CALIBRATION.md.

## Why not 1.0

1.0 is the release where COMPATIBILITY.md takes effect and register
addresses stop moving. One major change against a published point is
still scheduled, the PCS state machine in M3; the other one announced
since M1 shipped here. ROADMAP.md states what 1.0 has to pass.

## Try it

```sh
docker compose up            # Grafana at http://localhost:3000
```

```sh
# the headless plant behind its protocol surfaces:
cargo run --release -p bess-emulator

# then make something go wrong:
cargo run --release -p bess-emulator -- --scenario scenarios/hvac-failure-both-units.yaml
curl -s localhost:8080/api/v1/scenario | jq .next

# or hold a case to its record, as CI does:
cargo run --release -p bess-emulator -- \
  --scenario scenarios/protection-trip-staggered-return.yaml \
  --assert scenarios/protection-trip-staggered-return.digest.json
```

In the browser, from npm:

```sh
npm i bess-emulator
```

Full order of work and calibration gates: [ROADMAP.md](ROADMAP.md). Every
measurement with its sources and known gaps: [CALIBRATION.md](CALIBRATION.md).
What each fault does to each surface: [COMPATIBILITY.md](COMPATIBILITY.md).
