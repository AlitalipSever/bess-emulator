# M1 design: thermal and weather

Status: draft for review, pre-implementation. ROADMAP.md defines M1's goal and
calibration gate; this document fixes the architecture, the design decisions,
and the work breakdown. It absorbs what would otherwise be three documents
(architecture note, design spec, implementation plan) into one, matching the
weight of the milestone.

## 1. What M1 changes

M1 is described in the roadmap as "thermal + weather", but architecturally it
adds two capabilities that outlive the milestone:

1. **An exogenous input layer.** Until now every input has been synthetic, a
   pure function of the timestamp. M1 introduces replayed historical data as
   a first-class, licensed, versioned input source.
2. **Loss accounting with an address for every watt.** The M1 gate does not
   just ask for an annual round-trip efficiency in the field band; it asks
   for the nameplate-to-field gap to be explainable component by component.
   That is an architectural property: every loss category must be a named,
   metered quantity, not an implicit subtraction.

On top of those two capabilities, exactly one module deepens (the thermal
layer), per the iteration discipline. Interfaces widen where stated below;
no other module is reworked.

## 2. Architecture

### 2.1 The exogenous input layer

- **Determinism contract.** Replay preserves determinism because the dataset
  is a fixed artifact shipped with the release: same seed + scenario +
  dataset = byte-identical output still holds. The dataset's content hash is
  pinned and asserted at load.
- **New crate `bess-data`.** Holds compiled series and the fetch/compile
  scripts. This crate is the licensing boundary: per DATA-LICENSES.md, a
  source's license and redistribution status are recorded before it enters
  the repo. DWD observation data is redistributable with attribution; the
  exact license text is recorded in PR1.
- **Time alignment.** Hourly source data feeds a 1 s tick via linear
  interpolation. For temperature this is accurate (thermal time constants
  are hours). For irradiance it flattens sub-hour cloud transients; accepted
  at M1 depth and noted as a limitation, since container thermal mass
  filters those transients anyway.
- **Replay versus synthesis boundary.** Weather is replayed from M1 on. Grid
  frequency stays synthetic until M4 (frequency replay is an M4 deliverable
  tied to grid-code behaviors). `SyntheticWeather` remains available for
  tests and offline use.
- **Timeline mapping.** A scenario selects its start date inside the
  reference year; the simulation clock maps directly onto the dataset
  timeline.

### 2.2 The thermal energy graph

Nodes and flows after M1:

- **Nodes:** ambient (boundary condition from replay), container air
  (existing state), and a new per-rack cell mass node.
- **Flows:** cell I2R heat into container air (existing path), solar gain
  through the envelope (new: irradiance times an effective aperture area),
  envelope leakage via UA (existing), HVAC heat removal or addition
  (deepened: staged cooling plus a heating mode).

Consequences:

- **Trait widening.** `ThermalModel::step_container` currently receives only
  `ambient_c`; irradiance is carried in `Inputs` but never reaches the
  model. The signature widens to take the weather slice of the inputs
  (ambient and irradiance). Pre-1.0 this is allowed; it is called out in the
  release notes.
- **Cell temperature becomes state.** Today rack cell temperature is
  instantaneous (container air plus a fixed offset). M1 gives each rack a
  first-order thermal mass so cell temperature lags air. This is the
  groundwork M2's derating needs to be meaningful, and it changes the
  checkpoint format (section 7).
- **PCS heat stays outside.** Utility-scale PCS skids sit outside the
  battery container; PCS losses do not enter the container air node. Stated
  as a modeling assumption in ARCHITECTURE.md.

### 2.3 Loss accounting

Loss categories after M1, each a named quantity in the state tree with a
path to the meters:

| Category | Where | Status |
|---|---|---|
| PCS conversion loss | `pcs.loss_w` per block | exists (M0.5) |
| Transformer copper/iron | substation | exists |
| HVAC electrical | `hvac.electrical_w` per container | exists, deepened |
| Station standby inventory | substation aux, itemized | new: splits the 150 kW constant |
| PCS standby tare | per block, at zero power | new |

The energy meters gain per-category accumulators so `bess-bench` can emit
the loss waterfall by reading state, not by re-deriving physics.

## 3. Design decisions

Each decision is proposed here and confirmed or revised in its PR.

- **D1, weather source:** DWD Climate Data Center hourly station
  observations, 2 m air temperature plus global irradiance, one full
  reference year (2024). Candidate station: Lindenberg (Mark) observatory,
  Brandenburg, chosen for its strong radiation record. Series completeness
  is verified in PR1; a fallback station is acceptable. Rationale: German
  site, attribution-only license, real observations rather than reanalysis.
- **D2, site location:** the station choice pins GW-01's nominal location to
  eastern Germany. Recorded in ARCHITECTURE.md's GW-01 section.
- **D3, trait signature:** pass a small weather struct (ambient, irradiance)
  to `ThermalModel`, not the whole `Inputs` (grid frequency is not thermal
  business).
- **D4, thermal nodes:** two nodes on the container path: container air
  (existing) plus one first-order cell mass per rack. Finer granularity
  (cell groups) stays in M6+.
- **D5, HVAC:** two cooling stages plus an electric heating mode, hysteresis
  bands per stage, constant COP per mode, fan power per active stage.
  Parameters calibrated to a public utility-scale container HVAC datasheet
  in PR4; whether heating is resistive or a heat pump follows the datasheet.
  Measured in PR3, once the cells became their own thermal node: the M0
  placeholder capacity (40 kW thermal per container) cannot hold a container
  at setpoint under summer peak dispatch. A replayed July day peaks at about
  36 C air and 42 C cells against a 27 C cooling setpoint. Capacity is
  therefore part of PR4's calibration, not just staging.
- **D6, aux inventory:** the 150 kW station constant splits into an itemized
  inventory: controls/SCADA, per-rack BMS electronics, per-block PCS standby
  tare, lighting/misc. Item values come from public sources where available;
  the total stays calibratable against the aux-share gate.
- **D7, gate numbers:** annual round-trip efficiency in [0.80, 0.85]
  (CAISO/EPRI field band). Auxiliary share of annual throughput: target band
  fixed from EPRI/CAISO fleet reports in PR7 before the gate freezes
  (working placeholder 2 to 5 percent, not yet sourced).
- **D8, bench:** new `bess-bench` crate, headless accelerated runs, emits
  the CALIBRATION.md M1 section plus machine-readable JSON. CI runs a
  representative multi-week slice on every PR and the full year on release
  tags (a full year is ~31.5 M ticks; runtime is measured in PR7 and the
  release job budgeted accordingly).

## 4. Scope boundaries

In M1: everything above. Not in M1:

- Temperature derating of power limits (M2; the cell temperature dynamics
  that feed it land now).
- Converter thermal model and electrical depth (M3).
- Frequency replay, market signals (M4).
- Aging (M5).
- Permanent non-goals per ARCHITECTURE.md (CFD, waveform-level power
  electronics, invented signals).

## 5. Work breakdown

Eight PRs, each landing green:

1. **`bess-data`:** fetch script, compiled weather series, DATA-LICENSES.md
   entry. Accept: series loads, hash pinned, license recorded.
2. **`HistoricalWeather` driver:** wired into the GW-01 bundle; synthetic
   stays for tests. Accept: determinism test green against the pinned
   dataset; golden snapshot regenerated once, called out in the PR.
3. **Trait widening + solar gain + cell thermal mass.** Accept: thermal
   energy balance property test; checkpoint version bumped.
4. **Staged HVAC + heating,** datasheet-calibrated. Accept: stage transition
   unit tests; datasheet cited in CALIBRATION.md sources.
5. **Aux inventory + per-category loss accumulators.** Accept: waterfall
   identity property test (category sums equal meter deltas within
   tolerance).
6. **Signal map delta + Grafana panels.** Accept: additions only, minor
   version per COMPATIBILITY.md.
7. **`bess-bench` + gates in CI + CALIBRATION.md M1 entry.** Accept: annual
   RTE and aux share inside their bands; waterfall published.
8. **Docs + release v0.3.0:** ARCHITECTURE.md thermal section, README,
   ROADMAP status flip, release notes.

## 6. Signal map delta

Constraint discovered during design: per-block registers stride by 10
(1000, 1010, ...) and base+0..7 are taken, so each block has exactly two
free slots. Proposal:

- `blockNN.container.air_temp_c` at base+8 (i16, scale 10). If blocks carry
  more than one container, this is the block max, mirroring the existing
  cell temp min/max convention.
- `blockNN.hvac.state` at base+9 (u16 enum: off / stage 1 / stage 2 /
  heating).
- Site level, appended after `site.aux_power_w`: `site.hvac_power_w` (u32).
- Existing `site.weather.ambient_c` and `site.weather.irradiance_wm2` start
  carrying replayed data instead of synthetic values.

Per-container HVAC electrical detail, if wanted later, goes into a new
address range rather than renumbering; deferred until someone asks for it.

## 7. Checkpoint and compatibility impact

- New state: per-rack cell thermal state, HVAC stage, per-category loss
  accumulators. Checkpoint format version bumps; pre-1.0, old checkpoints
  are not migrated, and the release notes say so.
  - Revised in PR3: the cell thermal node needed no new field. `cell_temp_c`
    was already in the tree; it went from a value derived every tick to one
    integrated across ticks, which changes trajectories but not the schema,
    and an older checkpoint still restores into a valid plant. The version
    bump therefore waits for the fields that do change the schema: the HVAC
    stage (PR4) and the per-category loss accumulators (PR5).
- Signal map: additions only, minor version per COMPATIBILITY.md.

## 8. Calibration and test plan

- **Property tests:** thermal energy conservation per container (heat in =
  stored + leaked + removed, within tolerance); temperature sanity bounds
  over the reference year; aux accounting identity.
- **Golden snapshots:** regenerated exactly twice (PR2 dataset switch, PR3
  physics change), each called out; the determinism contract holds
  otherwise.
- **CI gates:** annual RTE band, aux share band, waterfall identity.
- **CALIBRATION.md:** the M1 entry is generated by `bess-bench`, ending the
  hand-maintained era.

## 9. Open questions

- DWD station and year completeness (resolved in PR1).
- Aux share band source (resolved in PR7; blocks freezing the gate).
- Heating technology in the reference container datasheet (resolved in PR4).
