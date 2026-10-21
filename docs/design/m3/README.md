# M3: PCS and electrical

Status: in progress. Designed 2026-10-21 after M2.5 shipped as v0.7.0;
phase 1 PR1 the same day. ROADMAP.md defines the goal and the
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
