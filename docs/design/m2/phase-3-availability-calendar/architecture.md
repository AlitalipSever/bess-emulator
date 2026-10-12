# M2 phase 3: availability and calendar, architecture

What exists after this phase: a plant that can be partially there, on
purpose (maintenance) or not (protection trip), and a clock that knows
what country it is in. Two subjects share the phase because both are
prerequisites the scenario engine triggers but must not implement:
scenarios say "block 7 enters maintenance at 06:00", the kernel owns what
that means.

The first half (maintenance, isolation, trip and return) is PR1 and is
pinned below against the tree as phase 2 left it. The calendar half is
PR2 and stays at the level the milestone design fixed.

## Maintenance and isolation

- **Racks** already carry `in_service`; isolation becomes reachable (a
  kernel command, later a scenario action) instead of a config-time fact.
  Everything else an isolated rack has to do is already true by
  construction: `BasicBms::rack_limits` returns zero for a rack out of
  service, `step_block` asks it for nothing, `average_soc` skips it, and
  `rack.isolated` has mirrored `in_service` since phase 2. The container
  thermal model keeps stepping it: an isolated rack still has temperature,
  and its electronics still draw (`AuxDemand::racks` counts every rack,
  in service or not).
- **Blocks** gain an operating mode, `BlockState::mode: BlockMode`, in
  service or maintenance. A block in maintenance is de-energized on the AC
  side: it is asked for nothing, so its PCS settles in `Standby` through
  the transition it already has, it reports zero capability, and its PCS
  stops paying the standby tare (a locked-out converter is switched off,
  not idling). Its containers keep their HVAC, which is realistic and
  keeps the auxiliary story honest: maintenance does not switch the house
  load off. The racks keep stepping with a zero request, so the spread
  and the balancing go on as they would in a real container under
  service. The block word says so: a `maintenance` bit that mirrors the
  mode, the way `rack.isolated` mirrors the contactor.
- **EMS availability reporting**: the site publishes available charge and
  discharge power against nameplate. Both the state (`EmsState` has
  carried `available_charge_w` and `available_discharge_w` since M0) and
  the map points (`site.available_discharge_kw` and
  `site.available_charge_kw`, inputs 8 and 10) already exist; what this
  phase changes is their truthfulness. Today the kernel sums every block's
  AC capability, a PCS in fault included, and never asks about the
  breaker. After this phase the values are correct under maintenance,
  isolation, derating, trips and the return, and the partial-availability
  site bit from phase 2's table also reads a block in maintenance. No new
  availability point is added.

## The dispatchable set

One predicate decides both what the plant controller allocates to and
what the site reports as available, so the two can never disagree:

```
dispatchable(block k) =
    block.mode == InService
    and block.pcs.op_state != Fault
    and k < ems.blocks_released
connected = substation.hv_breaker == Closed
```

The site target is split evenly over the dispatchable blocks (today: over
the non-faulted ones). Site availability is the sum of the AC capability
of the dispatchable blocks, and zero while the site is not connected. A
block outside the set contributes zero to both, which is what "reduce the
sum by construction, never by a separate subtraction" means in code: the
capability a maintained block or an unreleased block could have is not
available, so it is not reported.

## Protection trip and the staggered return

A protection trip is the site-level fault: the HV breaker opens, every PCS
drops to fault, POI power is zero within the tick, and the site
word latches `protection_trip` (bit 8, laid out in phase 2 for exactly
this). The interesting part is the return, which ARCHITECTURE.md has
promised since M0: after reset the site comes back in a staggered
sequence, blocks reconnecting one after another with a fixed spacing,
each through its existing operating-state transitions (the five-state
startup machine with precharge and synchronization stays in M3; what M2
owns is the sequencing, pulled forward from M3's scope because the trip
that needs it is an M2 fault).

The sequencer is EMS-layer state, two fields on `EmsState`:
`blocks_released`, how many blocks the plant controller has cleared to
start, counted from block 0, and `next_release_s`, seconds until it clears
the next one. A trip sets the count to zero. A site reset closes the
breaker and starts the clock at zero, so the first block is released on
the next tick and every further one `spacing_s` later. A released block
joins the dispatchable set, takes its share, and its PCS goes `Standby` to
`Run` on that tick; the POI climbs in one step per block. A plant that
never tripped has every block released from tick 0. The spacing is a
plant-controller parameter owned by the kernel, beside the block and site
thresholds, not a model parameter: no layer trait owns the start order of
a site.

No relay internals, no protection logic, per the non-goals: the trip is
caused by a scenario or a command, the observable aftermath is what we
model. The same door also takes a single PCS to fault (`pcs: trip` in the
phase 4 fault set, which no earlier phase built), one state change with
the transition the reset already reverses.

## The dark site and the house load

Opening the breaker for real exposes a question M0 never had to answer,
because M0 never opened it: what feeds the house load while the site is
disconnected? The kernel today drops the auxiliary draw to zero on an open
breaker while the thermal model keeps cooling, a mismatch that was
harmless as long as the branch was dead. This phase answers it the way an
HV-connected plant answers it: station service. The protection, the
controls, the rack electronics and the container HVAC stay supplied
through the station-service feed every such plant carries, the PCS
controls included (a tripped converter is energized and not converting,
which is what its tare pays for, see `AuxDemand`). The main transformer is
de-energized, so its no-load loss stops.

No current crosses an open breaker, so the POI reads exactly zero and
the site import and export meters stand still, as the milestone draft
said. The station-service feed is a separate connection with its own
meter, and the emulator gives it one: a third substation accumulator,
`station_service_wh`, that advances by the house load while the breaker
is open and does not move while it is closed. The site energy balance
gains it as a term (import plus station service minus export), and the
waterfall identity is unchanged, because the house load the substation
reports is the same total on either feed. The meter is published at
input 44 beside the two POI meters. What losing the station-service
supply as well does to a container that still has heat in it stays an
M3 question, as the kernel comment has said since M0, and becomes a
fault there. Design D10 records the decision.

## The commands

Four doors into the kernel, all applied between ticks like the phase 2
reset, with their events waiting in `SiteState::pending_events` so a
checkpoint in the gap loses nothing:

- `set_rack_in_service(block, container, rack, bool)`;
- `set_block_mode(block, BlockMode)`;
- `trip_pcs(block)`;
- `trip_protection()`, with `reset_alarms(Site)` widened to close the
  breaker and start the return when the site is tripped.

A command naming a node the site does not have is refused before anything
changes, as the reset is. None of them has a REST, Modbus or CLI surface
in this phase: the phase 4 scenario player is the operator door (its
design rejects a "fire one action now" endpoint), and until then kernel
tests reach them directly, as the chain test reaches `set_hvac_failed`.
The surfaces still see everything: every command of this phase speaks
through alarm bits and PCS transitions the shells already publish, and no
`Event` variant is added.

## The calendar

Three changes make the replayed calendar real:

- **Real day-ahead prices.** The M0 dispatch plan ranks 24 synthetic
  hourly prices and says so in its own doc comment ("until bess-data ships
  real day-ahead series"). This phase ships them: the 2024 German
  day-ahead series enters `bess-data` exactly as the weather did in M1
  (fetch script, compiled artifact, pinned hash, license recorded). The
  emulator's July 14th already has July 14th's weather; now it has its
  prices too. `EmsStrategy` does not deepen, the same planner just stops
  eating synthetic input; market behaviors stay in M4.
- **Local-day plan boundaries.** Day-ahead days are Europe/Berlin days.
  The kernel stays in UTC; the plan builder slices the price series on
  local midnights, so the two DST days of the reference year (2024-03-31
  and 2024-10-27) genuinely have 23 and 25 plan hours.
- **The 15-minute revenue meter.** Promised in ARCHITECTURE.md, still
  absent (grid.rs says "arrive in" a later milestone); pulled to now
  because the roadmap's DST scope names 92 and 100 quarter-hour market
  periods and there is currently no quarter-hour surface on which they
  could be observed. One monotonic import/export energy series at the
  POI, closed on local quarter-hours. It stands beside, not instead of,
  the SCADA meter accumulators the map has carried since M0
  (`site.meter.import_kwh` and `export_kwh`): those remain the telemetry
  meter, this is the fiscal series, and keeping the two distinct is what
  M2.5's measurement phase later builds on when they receive different
  accuracy classes. Grid-layer topology does not change; this is an
  accumulator, not the M3 electrical work.

## Invariants

- Capability accounting: site availability equals the derated sum over
  non-isolated, in-service racks in blocks that are dispatchable, and is
  zero while the HV breaker is open. Maintenance, trips and the return
  reduce the sum by construction, never by a separate subtraction.
- A maintained plant pays its house load, and so does a tripped one,
  the tripped one through the station-service meter; the auxiliary
  meters keep advancing and the itemized and metered totals keep
  agreeing, breaker open or closed.
- While the breaker is open the POI carries nothing: POI power is
  exactly zero, the import and export meters do not move, transformer
  loss is zero, `site.pcs_online_count` is zero, and the station-service
  meter advances by exactly the house load. While it is closed the
  station-service meter does not move.
- The site energy balance closes with three meters: import plus station
  service minus export equals the change in stored energy plus every
  loss and the house load.
- The return is ordered and spaced: after a site reset, block k first
  runs no earlier than k x spacing after the reset, blocks come back in
  index order, and a checkpoint taken mid-return resumes the same
  sequence and the same log.
- The revenue meter is monotonic, its period boundaries align with
  Europe/Berlin quarter-hours, and DST days close 92 or 100 periods.
- Every command of this phase leaves its trace in the event log through
  existing event kinds: isolation on `rack.isolated`, maintenance on
  `block.maintenance`, a PCS trip on the PCS transition and
  `block.pcs_fault`, a protection trip on `site.protection_trip`,
  `site.hv_breaker_open` and twenty PCS transitions, the reset on their
  clears.
