# M2 phase 2: alarm tree and event surface, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, three words, published as u16, low byte warnings, high byte
  trips.** Rationale in architecture.md. The alternative, one flat site
  alarm register, is what cheap gateways do and would erase the causality
  the emulator exists to demonstrate. The proposed rack layout (bit
  positions frozen in PR1):

  | Bit | Condition | Behavior |
  |---|---|---|
  | 0 | cell over-temperature warning | hysteresis |
  | 1 | cell under-temperature warning | hysteresis |
  | 2 | SoC high (window edge) | hysteresis |
  | 3 | SoC low | hysteresis |
  | 4 | imbalance warning | hysteresis |
  | 5 | derate active | hysteresis |
  | 6 | rack isolated | mirrors state |
  | 8 | cell over-temperature trip | latched |
  | 9 | imbalance high trip | latched |
  | 10 | cell under-temperature trip | latched |

  Block and site words get their own tables in the same PR. Gaps are
  headroom; a bit is never reused.
  **Frozen in PR1** (`bess_core::alarms::layout`, with published names
  and a test that no bit or name is laid out twice). The rack table above
  stands as proposed. Block and site:

  | Word | Bit | Condition | Behavior |
  |---|---|---|---|
  | block | 0 | PCS setpoint not met, past a 10 s deadband | hysteresis |
  | block | 1 | container air over temperature | hysteresis |
  | block | 2 | HVAC failure in one of the block's containers | mirrors state |
  | block | 8 | PCS fault | mirrors `PcsOpState::Fault`, which only a reset leaves |
  | site | 0 | power limited: blocks deliver less than the site setpoint | hysteresis |
  | site | 1 | partial availability: a PCS in fault or a rack isolated | mirrors state |
  | site | 2 | HV breaker open | mirrors state |
  | site | 8 | protection trip | latched; laid out now, raised from phase 3 |

  Site "power limited" reads delivered power, not `available_*`. The
  capability sums every rack's limit, but the even split has no
  redistribution pass, so a block with hot racks claims its full rating
  while leaving its share undelivered; a capability-based bit never fired
  in the chain test.
- **D2, alarm evaluation lives in the owning layer.** `BmsLogic` evaluates
  rack bits (it has the thresholds already, from phase 1), the kernel
  evaluates block and site bits from PCS, thermal, and grid state during
  the tick. The alternative, a central alarm evaluator reading the whole
  tree, would be simpler to write and would put cause and alarm in
  different files forever.
- **D3, hysteresis bands are model parameters with datasheet-adjacent
  defaults.** **Confirmed in PR1**, values and sources on the defaults
  (`RackAlarmThresholds`, `BlockSiteThresholds`) and in CALIBRATION.md.
  Two definitions settled there: "derate active" is a temperature factor
  below 0.98, not "the derate took power the dispatch asked for" (the BMS
  cannot see the request; that question is the block's setpoint bit), so
  it raises on cold winter mornings; and SoC high and low raise only past
  the window, since the plant sits at its edges every day by design. Warning thresholds sit inside the trip thresholds pinned in
  phase 1, with clear-below offsets sized so a steady plant near a
  threshold does not chatter (the anti short-cycle lesson from the M1
  HVAC, applied to bits).
- **D4, reset semantics.** **Confirmed in PR1** as
  `Simulation::reset_alarms(ResetScope)`: clears the latched bits in
  scope and takes a PCS in scope out of fault, emitting the clears and the
  PCS leaving fault as events. They wait in the tree
  (`SiteState::pending_events`, so a checkpoint in the gap keeps them) and
  go out, and into the log's count, with the next tick. It returns the
  bits whose cause is still present, which the next tick raises again, and
  refuses a scope naming a node the site does not have
  (`ResetError::NoSuchNode`) before changing anything, which is what the
  PR2 REST endpoint will answer with a 4xx. The emulator command and REST
  endpoint are PR2. `Command::ResetAlarms { scope }` with scopes
  site, block, rack. Clears latched bits whose condition is gone, returns
  which bits stayed. REST: `POST /api/v1/alarms/reset`. Rationale for
  scoped rather than per-bit reset: that is what HMI reset buttons do.
- **D5, the event counter is per site, u16, wrapping.** The state keeps a
  u64 count and a running digest (`SiteState::event_log`); the u16 is the
  projection's (`EventLog::counter_u16`), since a counter that wraps in
  the tree would make the digest ambiguous. One register,
  incremented per event emitted. A SCADA poller diffing it knows how many
  events it missed between polls. Per-block counters were considered and
  dropped: the register budget is better spent when someone asks.
  **Confirmed in PR2** as `site.event_counter` at input 43. It equals the
  `seq` of the latest event modulo 65536, so a poller and an MQTT
  subscriber count the same thing.
- **D6, map 0.2.0 to 0.3.0 lands at the release, this phase writes the
  delta.** The meaning change (rack alarm bits documented, base+6 fold now
  nonzero) is the breaking half; additions are the block word, site word,
  event counter, spread and derate status. The spread publishes as
  `cell_dv_mv`, millivolts at 1 mV LSB so a u16 covers the range; the
  state field stays `cell_dv_v` in volts like every other voltage in the
  tree, and the conversion is the projection's job. Blocks have no
  free slots in their original stride (base+0 to 9 are taken), so block
  additions open a second per-block address range, the mechanism M1
  section 6 already reserved for this case. Exact addresses are pinned in
  PR2 against the live map.
  **Confirmed in PR2**, with one revision: the version moved to 0.3.0 in
  this PR, the one that changed the map, as M1's moved in its PR6. The map
  digest test exists to force exactly that, and a binary serving 0.3.0
  points while `/health` says 0.2.0 would be the drift the version is there
  to prevent. "One bump at release" stands in the sense that matters: v0.5.0
  publishes 0.3.0, no 0.2.x ships in between, and phase 3's additions join
  the same unreleased version. The second range opened at 2000 with the
  original stride of 10, so a block's two ranges read alike and phase 3's
  block mode has room beside them. The alarm points also got a publication
  class of their own, `event` (below).
- **D7, checkpoint format 5.** **Landed in PR1**, with the HVAC failure
  flag and the setpoint-miss timer in the same bump. Latched bits and hysteresis side are state
  (a resumed run must not re-raise or silently clear). Block and site
  words are derived each tick except their latched bits, which persist.

## Signal map impact

The milestone's one breaking change plus the phase's additions, per D6.
`site.alarm_count` (input 31) keeps address and meaning and finally counts
nonzero bits. The delta table, with final addresses, is written into this
file by PR2.

**As built (PR2), map 0.2.0 to 0.3.0:**

| Address | Point | Encoding | Class | Change |
|---|---|---|---|---|
| input 31 | `site.alarm_count` | u16 | event (was medium) | meaning: set bits of all three words, no longer racks only |
| input 42 | `site.alarm_bits` | u16 bitfield | event | new: the site word |
| input 43 | `site.event_counter` | u16, wrapping | event | new (D5) |
| base+6 | `blockNN.alarm_bits` | u16 bitfield | event (was medium) | meaning: the rack layout; name kept, it is published |
| 2000 + 10b | `blockNN.block_alarm_bits` | u16 bitfield | event | new: the block word |
| 2000 + 10b + 1 | `blockNN.racks_derated` | u16 count | event | new: the derate status, racks with `derate_active` set |
| 2000 + 10b + 2 | `blockNN.cell_dv_mv` | u16, 1 mV | medium | new: the block's widest rack spread |

Three choices made against the live map:

- **The block word's name.** `blockNN.alarm_bits` was taken by the rack
  fold in M0, and renaming a published topic is a major change of its own.
  The block's word is `block_alarm_bits`, and COMPATIBILITY.md says in one
  line which register is which.
- **Derate status as a count.** The fold at base+6 already says whether
  some rack derates. What it cannot say is how much of the block does, and
  the state holds no per-block capability to publish without a checkpoint
  change, so the status is the number of racks with the bit set.
- **A publication class for alarm points.** ARCHITECTURE.md has always
  listed an `event` class (report by exception); the map had no way to say
  it. `Class::Event` points publish on MQTT when their value changes, QoS 1
  and retained, and never on a cadence; Modbus refreshes them every tick
  like everything else. Every point derived from alarm words or the event
  log is in it, including the two published points that move from medium,
  and a test keeps the class confined to the map's `alarms` module.

## Checkpoint impact

Format 4 to 5 (D7).

## Test plan

- **Property: raise/clear pairing.** Over arbitrary dispatch and injected
  temperatures, the event log and the alarm words never disagree
  (architecture invariant 1).
- **Property: no chatter.** A monotone temperature ramp crosses each
  threshold exactly once in the event log.
- **Unit: latching.** Trip condition on, condition off, bit still set;
  reset clears; reset with condition still on re-raises next tick.
- **Integration: the causal chain.** Scripted hot afternoon plus HVAC
  failure (direct state manipulation in a kernel test; the scenario engine
  arrives two phases later): assert the order derate active, setpoint not
  met, power limited, and assert the timestamps are minutes apart, not
  same-tick, because the thermal masses are what spaces them.
- **Determinism:** the golden snapshot digest now covers the event log
  hash as well; regenerated once, called out.
- **Surface checks:** dashboard-metric existence test extended to the new
  alarm panels; MQTT events subtree exercised in the publisher's tests.
