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
  | 5 | derate active | condition |
  | 6 | rack isolated | condition |
  | 8 | cell over-temperature trip | latched |
  | 9 | imbalance high trip | latched |

  Block and site words get their own tables in the same PR. Gaps are
  headroom; a bit is never reused.
- **D2, alarm evaluation lives in the owning layer.** `BmsLogic` evaluates
  rack bits (it has the thresholds already, from phase 1), the kernel
  evaluates block and site bits from PCS, thermal, and grid state during
  the tick. The alternative, a central alarm evaluator reading the whole
  tree, would be simpler to write and would put cause and alarm in
  different files forever.
- **D3, hysteresis bands are model parameters with datasheet-adjacent
  defaults.** Warning thresholds sit inside the trip thresholds pinned in
  phase 1, with clear-below offsets sized so a steady plant near a
  threshold does not chatter (the anti short-cycle lesson from the M1
  HVAC, applied to bits).
- **D4, reset semantics.** `Command::ResetAlarms { scope }` with scopes
  site, block, rack. Clears latched bits whose condition is gone, returns
  which bits stayed. REST: `POST /api/v1/alarms/reset`. Rationale for
  scoped rather than per-bit reset: that is what HMI reset buttons do.
- **D5, the event counter is per site, u16, wrapping.** One register,
  incremented per event emitted. A SCADA poller diffing it knows how many
  events it missed between polls. Per-block counters were considered and
  dropped: the register budget is better spent when someone asks.
- **D6, map 0.2.0 to 0.3.0 lands at the release, this phase writes the
  delta.** The meaning change (rack alarm bits documented, base+6 fold now
  nonzero) is the breaking half; additions are the block word, site word,
  event counter, spread (`cell_dv_mv`), and derate status. Blocks have no
  free slots in their original stride (base+0 to 9 are taken), so block
  additions open a second per-block address range, the mechanism M1
  section 6 already reserved for this case. Exact addresses are pinned in
  PR2 against the live map.
- **D7, checkpoint format 5.** Latched bits and hysteresis side are state
  (a resumed run must not re-raise or silently clear). Block and site
  words are derived each tick except their latched bits, which persist.

## Signal map impact

The milestone's one breaking change plus the phase's additions, per D6.
`site.alarm_count` (input 31) keeps address and meaning and finally counts
nonzero bits. The delta table, with final addresses, is written into this
file by PR2.

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
