# M2 phase 3: availability and calendar, architecture

What exists after this phase: a plant that can be partially there, on
purpose (maintenance) or not (protection trip), and a clock that knows
what country it is in. Two subjects share the phase because both are
prerequisites the scenario engine triggers but must not implement:
scenarios say "block 7 enters maintenance at 06:00", the kernel owns what
that means.

## Maintenance and isolation

- **Racks** already carry `in_service`; isolation becomes reachable (a
  kernel command, later a scenario action) instead of a config-time fact.
  An isolated rack leaves the capability sums, its alarm word shows the
  isolated bit, and the container thermal model keeps stepping it: an
  isolated rack still has temperature.
- **Blocks** gain an operating mode: in service or maintenance. A block in
  maintenance reports zero capability, its PCS goes to standby (a
  maintained block is de-energized on the AC side but its containers keep
  their HVAC, which is realistic and keeps the auxiliary story honest:
  maintenance does not switch the house load off).
- **EMS availability reporting**: the site publishes available charge and
  discharge power against nameplate. Both the state (`EmsState` has
  carried `available_charge_w` and `available_discharge_w` since M0) and
  the map points (`site.available_discharge_kw` and
  `site.available_charge_kw`, inputs 8 and 10) already exist; what this
  phase changes is their truthfulness. The values become correct under
  maintenance, isolation, derating and trips, and the
  partial-availability site alarm bit from phase 2's table reads off
  them. No new availability point is added.

## Protection trip and the staggered return

A protection trip is the site-level fault: HV breaker opens, every PCS
drops to fault, POI power is zero within the tick. The interesting part is
the return, which ARCHITECTURE.md has promised since M0: after reset the
site comes back in a staggered sequence, blocks reconnecting one after
another with a fixed spacing, each through its existing operating-state
transitions (the five-state startup machine with precharge and
synchronization stays in M3; what M2 owns is the sequencing, pulled
forward from M3's scope because the trip that needs it is an M2 fault).
The sequencer is EMS-layer state: which blocks are cleared to start and
when. No relay internals, no
protection logic, per the non-goals: the trip is caused by a scenario or a
command, the observable aftermath is what we model.

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
  non-isolated, in-service racks in blocks that are themselves in
  service, and is zero while the HV breaker is open. Maintenance and
  trips reduce the sum by construction, never by a separate subtraction.
- A maintained or tripped plant still pays its house load; auxiliary
  meters keep advancing.
- The revenue meter is monotonic, its period boundaries align with
  Europe/Berlin quarter-hours, and DST days close 92 or 100 periods.
- POI power is exactly zero while the breaker is open.
