# M3: PCS and electrical

Status: in progress. Designed 2026-10-21 after M2.5 shipped as v0.7.0;
phase 1 PR1 and phase 2 PR1 the same day. ROADMAP.md defines the goal and the
calibration gate; this folder fixes the architecture, the decisions and
the work breakdown, one phase per subfolder, each phase's documents to be
marked with what was actually built. Layout per
[docs/design/README.md](../README.md).

## Goal

The electrical path stops being a constant and starts being a character
in the causal chain. After M3 a consumer polling the plant sees a
converter whose efficiency depends on the DC voltage the racks hand it,
whose power limit falls with the racks' state of charge, that walks
through real startup states on its way back from a trip, that follows a
setpoint with a dead time and a ramp instead of instantly, that derates
on its own temperature and spends a bounded overload budget, and a
substation whose breakers interlock, whose transformer has a hot spot
and whose tap changer steps the POI voltage.

## What M3 changes

Following the M2 pattern: the one module that deepens is the PCS, behind
the existing `PcsModel` trait, with the substation (`GridInterface`)
deepened in the last phase because the breaker state machine and the
transformer belong to the same electrical story and have waited since M0
("breaker interlocks, OLTC behavior and transformer thermal arrive in
M3", `grid.rs`).

1. **The efficiency surface gains its second dimension.** M0.5 fitted
   the partial-load curve at nominal DC voltage; the Sandia model's
   voltage terms (C1 to C3) were dropped there and come back here, driven
   by the real rack voltage, which already carries SoC, current and
   hysteresis.
2. **SoC-dependent power limits.** A converter's DC current is bounded;
   its power therefore follows the DC voltage, lowest at the bottom of
   the window. Today the rack limit is a power; it becomes a current.
3. **The operating state machine.** Standby, precharge, contactor close,
   synchronize, ramp, run, fault: the staggered return M2 built gives the
   returning blocks their real startup states, and every transition is
   an event.
4. **Setpoint dynamics.** Dead time, ramp limit, first-order settling,
   per block, so a written setpoint is followed the way a converter
   follows it and a control application under test meets latency.
5. **Converter thermal and the overload budget.** A converter thermal
   node fed by its losses, derating from its temperature, and a bounded
   short-term overload the datasheet publishes.
6. **Substation depth.** Breaker and disconnector interlocks behind the
   HV breaker, a transformer hot-spot model (IEC 60076-7) with its own
   alarm, and an OLTC whose steps show in `site.poi.voltage_kv`.

## Phases

| Phase | Theme | Depends on |
|---|---|---|
| [1: Efficiency surface](phase-1-efficiency-surface/) | f(P, V_dc), current-bounded limits, DC voltage published | nothing |
| [2: PCS state machine](phase-2-pcs-state-machine/) | seven-state machine, startup sequence, setpoint dynamics | 1 (limits the ramp obeys) |
| [3: Converter thermal](phase-3-converter-thermal/) | converter thermal node, derating, overload budget | 2 (states the budget belongs to) |
| [4: Substation](phase-4-substation/) | interlocks, transformer hot spot, OLTC, release v0.8.0 | 2 (the breaker sequence uses the states) |

Phases land in order. A phase is done when its PRs are merged, its tests
are green in CI, and its documents record what was actually built.

## Calibration gate

From ROADMAP.md: efficiency surfaces match public Sandia/CEC inverter
database curves (now at every DC voltage the database tabulates, not
only the nominal one); the M1 round-trip gate still holds with the new
electrical losses in place. Per phase, against public data, recorded in
CALIBRATION.md:

1. The two-dimensional efficiency fit reproduces the CEC record's curves
   at its three tabulated DC voltages within the M0.5 tolerance (0.3
   points) at the six CEC load points.
2. Startup sequence durations and ramp rates sit inside published grid
   code and datasheet ranges (VDE-AR-N 4110/4120 for the ramp, the
   converter datasheet for precharge and synchronization), labeled where
   a datasheet gives a range rather than a value.
3. Converter derating and overload follow the converter datasheet's
   published curve and overload table.
4. Transformer hot-spot rise and time constants follow IEC 60076-7's
   published exponents and typical values; the OLTC step and range a
   public transformer datasheet.
5. The annual record: round trip stays inside the M1 band; the new loss
   categories (converter voltage dependence, transformer thermal) are
   itemized in the waterfall.

## Versioning

- **Crate:** v0.8.0 at milestone end.
- **Signal map:** breaking in phase 2: `blockNN.pcs.state` (base+2)
  stops being a three-value enum, which COMPATIBILITY.md promised since
  M0 ("M3 replaces a three-value PCS state with a five-state machine").
  Map 0.5.0. Additions in every phase (DC voltage, converter temperature,
  overload budget, transformer hot spot, tap position), at unused
  addresses in the second per-block range and the site range.
- **Checkpoint:** bumps in phases 2 (state machine timers, setpoint
  filter state), 3 (converter thermal node, overload budget) and 4
  (transformer thermal node, tap position, breaker sequence). Phase 1
  targets no bump: the DC voltage is read from the racks.

## Release-note inventory for v0.8.0

Filled as PRs land.

- **The converter converts on a surface** (phase 1 PR1). Conversion
  loss follows the DC bus voltage the racks present, through the CEC
  record's own voltage terms; `blockNN.pcs.efficiency_pct` moves with
  SoC now, by under a point across the window. New point
  `blockNN.pcs.dc_voltage_v` (2000 + 10b + 4). Map 0.5.0 (addition).
- **Capability follows SoC** (phase 1 PR1). Rack limits are the current
  limit times the voltage the rack presents, so `site.available_*_kw`
  read a few percent lower at the bottom of the window and higher at
  the top than the flat nominal they carried before.
- **Checkpoint format 11** (phase 1 PR1): `PcsState::dc_voltage_v`.
- **`blockNN.pcs.state` is SunSpec-numbered, breaking** (phase 2 PR1).
  1 off, 3 starting, 4 running, 7 fault, 8 standby, in place of the M0
  0/1/2. The converter has a state machine behind it: a reset leaves it
  off with its contactors open, a cleared block walks precharge,
  contactor close and synchronization (16 s) before it runs, so after a
  trip the staggered return takes 30 s per block plus 16 s, and the
  partial-availability bit holds until the last converter has
  synchronized. MQTT `pcs/state` and the stream carry the finer names
  (`precharge`, `contactor_close`, `synchronize`, `ramp`, `off`).
- **Checkpoint format 12** (phase 2 PR1): `PcsState::state_elapsed_s`.
- **The converter follows its setpoint** (phase 2 PR2). Per block the
  share passes the tick's dead time, a ramp limit of 10 % of the
  converter's rating per minute (VDE-AR-N 4110) and a 0.5 s first-order
  lag, so a site step over Modbus is reached ten minutes later, climbing
  at the gradient, with neither `setpoint_not_met` nor `power_limited`
  raised on the way. New point `blockNN.pcs.setpoint_followed_kw`
  (2000 + 10b + 5), exact. A baseline recorded against v0.7.0's
  instantaneous steps will differ on every hour boundary of the plan.
- **Checkpoint format 13** (phase 2 PR2): `PcsState::setpoint_ramped_w`,
  `setpoint_followed_w`.
- **The converter is warm and has an overload budget** (phase 3). One
  thermal node per converter fed by its losses (SG2500U: full power to
  50 °C ambient, derating above; the rise and the time constant labeled
  estimates), published as `blockNN.pcs.temperature_c` (2000 + 10b + 6,
  Pt100 class B) with block bits `pcs_over_temperature` (4) and
  `pcs_derated` (5). A short-term overload of 110 % (the datasheet's
  2750 kVA) for ten minutes (labeled), published as
  `blockNN.pcs.overload_budget_pct` (2000 + 10b + 7): a lost block's
  share lands on the others at 105 % for twenty minutes, two lost blocks'
  at 110 % for ten, before the site reports its limit; a staggered
  return runs the early blocks at 110 %. New library case
  `pcs-overload-budget`; `pcs-trip-and-reset` and the staggered return
  tell the new story in their headers. Advertised capability stays
  bounded by the rating.
- **Checkpoint format 14** (phase 3): `PcsState::temperature_c`,
  `overload_budget`, `overload_armed`, `ac_limit_w`.
- **The substation has switchgear** (phase 4 PR1). Line disconnectors
  and an earthing switch beside the HV breaker, with the interlocks a
  substation enforces: nothing moves under a closed breaker, the
  earthing switch closes only on an isolated bus, the breaker closes
  only onto a connected, unearthed bus with no trip latched. The close
  is a sequence that lands on the tick after the command; a site reset
  on a tripped site starts it. Points `site.substation.sequence` (52),
  `disconnectors_closed` (53), `earthing_closed` (54); events
  `breaker_sequence` and `switch`; scenario kinds on `site.substation`.
  New maintenance case `substation-earthing-work`.
- **Checkpoint format 15** (phase 4 PR1): `SubstationState::disconnectors`,
  `earthing_switch`, `sequence`, `sequence_elapsed_s`.
- **The transformer is warm and the bus is regulated** (phase 4 PR2).
  IEC 60076-7's two-temperature model on the main transformer (rated
  rises from IEC 60076-2, 60 K top oil and 78 K hot spot; the ONAF
  constants labeled), published as `site.transformer.top_oil_c` (55)
  and `hot_spot_c` (56) through thermometers, with site bits
  `transformer_over_temperature` (3, 120 °C) and `transformer_trip` (9,
  140 °C, the kernel trips the site). An on-load tap changer (±8 × 1.25
  %, labeled) holds the MV bus at nominal against a ±2 % daily swing of
  the grid voltage, a new kernel input: `site.transformer.tap_position`
  (57) and `site.mv_bus.voltage_kv` (58); `site.poi.voltage_kv` swings
  with the grid instead of reading a constant. A baseline recorded
  against v0.7.0's constant POI voltage will differ.
- **Checkpoint format 16** (phase 4 PR2): `SubstationState::top_oil_c`,
  `hot_spot_c`, `hot_spot_rise_1_k`, `hot_spot_rise_2_k`,
  `tap_position`, `tap_timer_s`, `mv_bus_voltage_kv`.

## Open questions

- Whether the converter datasheet's overload table is public enough to
  pin (Sungrow publishes 110 % for 10 min on some models; the CEC record
  carries no overload data). Fallback: a labeled estimate with the
  datasheet family named.
- Whether the OLTC lives on the HV or the MV side of the main
  transformer for GW-01; the choice sets which published voltage steps.
  Leans HV-side regulation of the MV bus, the common arrangement for a
  generation-connected step-up transformer, with `site.poi.voltage_kv`
  carrying the grid side and a new MV bus voltage point the stepped
  side.
- Whether setpoint dynamics belong in `PcsModel` (per block) or in the
  EMS (site). Leans per block: the dead time and the ramp are the
  converter's, the EMS only distributes.
