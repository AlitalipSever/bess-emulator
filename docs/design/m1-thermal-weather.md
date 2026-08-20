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
| HVAC electrical | `hvac.electrical_w` per container, summed into `aux.hvac_w` | exists, deepened |
| Rack electronics | `aux.bms_w` | done in PR5 |
| Station standby inventory | `aux.controls_w` and `aux.lighting_and_safety_w` | done in PR5: splits the 150 kW constant |
| PCS standby tare | `aux.pcs_standby_w`, idle blocks only | done in PR5 |

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
  Resolved in PR4 against the STULZ WXUC5, a BESS-dedicated unit installed
  two per 40 ft battery container: heating is **resistive**, multi-stage
  electric, and it exists for the battery's standby phase. Staging follows
  the installation's own topology, one unit then both, rather than being an
  abstraction: capacity per unit is the reference's 35 kW scaled by container
  energy to 56 kW. Full sourcing in CALIBRATION.md.
  Measured in PR3, once the cells became their own thermal node: the M0
  placeholder capacity (40 kW thermal per container) cannot hold a container
  at setpoint under summer peak dispatch. A replayed July day peaks at about
  36 C air and 42 C cells against a 27 C cooling setpoint. Capacity is
  therefore part of PR4's calibration, not just staging. Second finding from
  the same change: with the cells' mass off the air node, a thermostat cycle
  shortened from hours to minutes. That direction is right, but the model
  still has no minimum run time, so PR4 owns the staging, a minimum run time,
  and a compressor cycles per day figure someone has actually looked at.
- **D6, aux inventory:** the 150 kW station constant splits into an itemized
  inventory: controls/SCADA, per-rack BMS electronics, per-block PCS standby
  tare, lighting/misc. Item values come from public sources where available;
  the total stays calibratable against the aux-share gate.
  Resolved in PR5 with the four items as proposed, behind a new
  `AuxiliaryModel` trait rather than inside the substation model: the house
  load is a measured category with a gate of its own, so it gets a layer of
  its own. Two items are sourced (rack electronics from Schimpe et al. 2018
  Table 3, converter standby from the CEC night-tare field of the same
  database entry the efficiency curve uses), two are engineering estimates
  labeled as such. The items were sized independently and the total is what
  they sum to: 66.3 kW against the 150 kW placeholder, which moved the
  auxiliary share of a January day from 4.4% to 2.6% and of a July day from
  6.9% to 5.1%. Nothing was tuned to preserve the old number, because the old
  number was never sourced. Full record and known gaps in CALIBRATION.md.
  Rule fixed here to prevent double counting: a converting PCS does not pay
  the standby tare, since its self-supply is already inside the
  load-independent term of the efficiency curve.
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
  - Done in PR4: format version 1 to 2. `HvacState` replaced its cooling
    flag with a staged mode and gained the anti short-cycle timer, which is
    state because a resumed run has to continue mid-cycle rather than restart
    the timer. A v1 file is rejected with its version named, not migrated.
  - Done in PR5: format version 2 to 3. The site gained `aux` (the itemized
    draw of the last tick) and the energy meters gained `aux_items`. An older
    file would restore into a plant whose auxiliary meters read zero and
    whose waterfall silently misses four rows, so it is rejected by version.
- Signal map: additions only, minor version per COMPATIBILITY.md.

## 8. Calibration and test plan

- **Property tests:** thermal energy conservation per container (heat in =
  stored + leaked + removed, within tolerance); temperature sanity bounds
  over the reference year; aux accounting identity.
- **Golden snapshots:** planned for exactly twice (PR2 dataset switch, PR3
  physics change). It became four: PR4 added the HVAC mode and its
  anti short-cycle timer to the state tree, PR5 the auxiliary inventory and
  its meters, and any field added to the tree moves the digest by definition.
  The plan was wrong about the count, not about the rule: every regeneration
  is called out in its PR and in the constant's own comment, and no
  regeneration has yet been needed for a reason other than a deliberate
  schema or model change.
- **CI gates:** annual RTE band, aux share band, waterfall identity.
- **CALIBRATION.md:** the M1 entry is generated by `bess-bench`, ending the
  hand-maintained era.

## 9. Release-note inventory for v0.3.0

Kept here as the PRs land, so the release note in the last step is an edit
rather than an archaeology exercise. Every entry is a change someone
integrating against v0.2.0 can trip over.

- **`Inputs` reshaped** (PR3): ambient and irradiance moved into a nested
  `Inputs::weather` of type `Weather`. `SiteState::weather` is that same
  type; `WeatherState` is gone. Field names, and therefore the checkpoint
  JSON and the signal map, are unchanged.
- **`ThermalModel::step_container` signature changed** (PR3): it now takes
  the per-rack heat slice and a `Weather` instead of a single container heat
  total and an ambient temperature, and returns `ThermalFlows` instead of the
  HVAC electrical draw alone.
- **`LumpedThermal` fields changed** (PR3): `heat_capacity_j_per_k` became
  `air_heat_capacity_j_per_k` and no longer carries the cells' mass; new
  `rack_heat_capacity_j_per_k`, `rack_to_air_w_per_k` and
  `sol_air_coefficient_m2_k_per_w`.
- **Cell temperature dynamics changed** (PR3): the per-rack cell temperature
  registers report an integrated quantity that lags container air, not air
  plus a constant offset. Same addresses, same units, different behavior; any
  regression baseline recorded against v0.2.0 telemetry will differ.
- **Weather is replayed, not synthesized** (PR2): `site.weather.*` carries
  DWD observations, and the default driver is `gw01_weather()`.
- **Checkpoint format version 1 to 2** (PR4): v1 files are rejected, not
  migrated. Pre-1.0, and the loader says so by version.
- **`HvacState` reshaped** (PR4): `cooling_on: bool` became
  `mode: HvacMode` (off, one unit, two units, heating) plus a `mode_hold_s`
  timer. `thermal_w` is now signed: positive removes heat, negative adds it.
- **`LumpedThermal` HVAC fields replaced** (PR4): `cooling_thermal_w`,
  `cool_on_c`, `cool_off_c` and `fan_w` gave way to the staged set
  (`unit_cooling_thermal_w`, per-stage bands, `heating_thermal_w`, heating
  bands, `unit_fan_w`, `min_run_s`, `min_off_s`).
- **Auxiliary load grew** (PR4): the HVAC is sized against a container
  datasheet now, so it draws what a real one draws. Anyone comparing
  auxiliary energy against a v0.2.0 baseline will see a step change.
- **New layer `AuxiliaryModel`** (PR5): the site's house load is itemized
  behind its own trait, and `Models` carries an `aux` implementation. Anyone
  assembling a `Models` bundle by hand has one more field to fill.
- **`SimpleGrid` lost `station_aux_w`** (PR5): the substation no longer adds
  a 150 kW constant of its own. `GridInterface::step` takes the site
  auxiliary total, itemized upstream, instead of the HVAC draw alone.
- **New state and meters** (PR5): `SiteState::aux` carries the itemized draw
  of the last tick and `EnergyAccounting::aux_items` its per-item energy.
  `aux_wh` keeps its meaning, the total as the substation meters it.
- **Checkpoint format version 2 to 3** (PR5).
- **Station load fell to 66.3 kW** (PR5): itemizing replaced an unsourced
  150 kW placeholder, which moves auxiliary energy and lifts the single-cycle
  round-trip figure from 0.9186 to 0.9207. Values, sources and the known gaps
  are in CALIBRATION.md.

## 10. Open questions

- DWD station and year completeness (resolved in PR1).
- Aux share band source (resolved in PR7; blocks freezing the gate).
- Heating technology in the reference container datasheet (resolved in PR4).
