# Calibration record

Realism in this project is never claimed, always measured: every milestone
states a target against public data, and this file records what was measured,
with what sources, at which release. The gates listed here run in CI; a
regression fails the build.

Two kinds of content live here and they are kept current in two different
ways. Blocks marked as generated are written by `bess-bench` from a committed
record of what it measured, and CI fails when they go stale. Everything else,
parameter provenance and sources and known gaps, is written by hand: a source
is knowledge rather than a measurement, and no harness can produce it.

## M0 (v0.1.0): energy balance and initial round-trip band

- **Gate:** energy conservation, SoC bounds, and meter monotonicity as CI
  property tests; point-of-interconnection round-trip efficiency of a
  full-depth 0.5C cycle inside [0.87, 0.90] with the flat 97.5% PCS
  placeholder.
- **Measured:** RTE 0.897 with the flat PCS (import 173.9 MWh, export
  156.0 MWh; re-measured on the v0.2.0 kernel with `FlatPcs` swapped back
  in, since this file did not exist at v0.1.0).
- **Sources:** none external yet; the band was an engineering estimate for a
  system with conversion and transformer losses but no thermal/auxiliary
  modeling.

## M0.5 (v0.2.0): PCS partial-load efficiency curve

- **Gate 1, curve fit:** `CurvePcs::cec_utility_reference` coefficients
  (k0 = 2.464096e-3, k1 = -1.402627e-3, k2 = 2.144562e-2, per-unit of AC
  rating) must reproduce the reference efficiency curve within 0.3
  percentage points at the 5/10/20/30/50/75/100% load points. Enforced by
  `curve_matches_the_cec_reference_within_the_gate` in
  `crates/bess-models/src/pcs.rs`.
  - **Reference:** Sungrow SC2500UD-US {900V}, Sandia performance-model
    coefficients from the CEC inverter database as distributed with NREL SAM
    (see DATA-LICENSES.md), evaluated at nominal DC voltage.
  - **Measured:** worst error 0.037 pp (at the 5% point). CEC-weighted
    efficiency of the fit: 98.33%; Euro-weighted: 98.27%.
- **Gate 2, round-trip re-measurement:** the 0.5C full-depth cycle gate
  moved from [0.87, 0.90] to [0.90, 0.93].
  - **Measured:** RTE 0.917 (import 172.0 MWh, export 157.8 MWh).
  - **Why the band moved:** the flat 97.5% placeholder underestimated
    conversion efficiency at the 50% load this cycle runs at (the real curve
    sits near 98.6% one-way there), so replacing it raised the measured
    value from 0.897 to 0.917. The new value still sits inside the 88-94% nameplate
    band for modern LFP systems. The 80-85% field band remains the M1 gate:
    thermal and auxiliary losses are the missing components, and they are
    M1 scope, not tunable knobs to force this gate to stay put.
- **Known simplifications (tracked for M3):** no V_dc dependence (the
  Sandia C1-C3 terms are dropped at nominal voltage), charge direction
  reuses the discharge curve, no operating state machine or setpoint
  dynamics. The three-term polynomial is a fit family, not a strict physical
  loss decomposition; k1 fitting slightly negative is expected and
  documented in `crates/bess-models/src/pcs.rs`.

## M1: the annual measurement

This is the M1 gate. Everything else in this file is a parameter, a fit, or a
single day. This section is the plant run against the whole replayed weather
year, 31.5 million ticks in one process, measured at the point of
interconnection, with every loss read off a meter the kernel kept while it
ran rather than re-derived afterwards from a summary.

<!-- bess-bench:begin m1-annual -->

Measured by `bess-bench` on kernel 0.2.0: GW-01 on the internal dispatch
plan, seed 7, 365 simulated days (31 536 000 ticks) of the replayed weather
year. Regenerate with `cargo run --release -p bess-bench -- --write`; CI
fails if this block is stale.

| Gate | Band | Measured | Verdict | Band drawn from |
|---|---|---|---|---|
| Annual round-trip efficiency at the POI | 80.00% to 85.00% (sourced) | 84.22% | inside | EIA-923 fleet average 82% (2019); NREL ATB 2024 design assumption 85% |
| Auxiliary share of energy imported | 1.00% to 8.00% (sanity bound) | 2.59% | inside | no public dataset publishes this at plant scale; see the note below |

Energy at the point of interconnection over the run:

| Quantity | Value |
|---|---|
| Imported | 74 400.9 MWh |
| Exported | 62 657.9 MWh |
| Round-trip efficiency | 0.8422 |
| Equivalent full cycles | 312.3 |
| Stored energy, end minus start | -91.6 MWh |
| Auxiliary share of import / of export | 2.59% / 3.07% |
| Unexplained residual | 0.00069% of throughput |

Where the energy went, each category on its own meter:

| Category | MWh | Share of import |
|---|---|---|
| Battery | 5 103.4 | 6.86% |
| PCS conversion | 3 128.2 | 4.20% |
| Transformer | 1 680.1 | 2.26% |
| Auxiliary: HVAC | 1 355.8 | 1.82% |
| Auxiliary: rack electronics | 301.9 | 0.41% |
| Auxiliary: PCS standby | 47.1 | 0.06% |
| Auxiliary: controls and protection | 131.4 | 0.18% |
| Auxiliary: lighting and safety | 87.6 | 0.12% |
| **Total** | **11 835.5** | **15.91%** |

Temperatures and HVAC over the run:

| Reading | Value |
|---|---|
| Ambient, coldest to warmest | -12.1 to 33.3 C |
| Container air, coldest to warmest | 10.0 to 29.0 C |
| Cells, coldest to warmest | 9.1 to 38.0 C |
| Duty, one unit / both units / heating | 17.79% / 0.33% / 0.03% |
| Cooling starts per container | 6 729.6 |

<!-- bess-bench:end m1-annual -->

**Where the round-trip band comes from.** Its floor is the measured fleet:
[the EIA computes an average monthly round-trip efficiency of 82% for the
U.S. utility-scale battery fleet in
2019](https://www.eia.gov/todayinenergy/detail.php?id=46756), from plant-level
consumption and generation reported on Form EIA-923, so whatever those plants
spend on themselves is inside that number by construction. Its ceiling is the
reference design: [NREL's Annual Technology Baseline assumes 85% for
utility-scale battery
storage](https://atb.nrel.gov/electricity/2024/utility-scale_battery_storage)
in its 2024 edition, revised down from 86% in 2022. Falling below the floor
would claim this plant is worse than a fleet of mixed vintages, chemistries
and duty cycles; beating the ceiling would claim it is better than the
reference design. The model has earned neither claim, which is what makes the
interval a gate rather than a decoration.

**Where the measurement sits, and why there.** It lands between the two,
close to the design assumption. That is the expected place for what this plant
currently is: brand new, so no capacity fade and no resistance growth; never
out of service, so all 8760 hours are productive hours; modern LFP with a PCS
efficiency curve fitted to a real database entry; and heavily used, at 312
equivalent full cycles in the year. Utilization is the one that matters most.
Every fixed load the plant carries, the controls, the lighting, the standby
tare, the transformer's no-load loss, is divided by throughput when it reaches
this ratio, and this plant has a great deal of throughput to divide by.

**Reading the auxiliary share.** Auxiliary consumption costs 2.2 percentage
points of round-trip efficiency here: without it the same year reads 86.5%.
The only peer-reviewed decomposition of the same kind, [Schimpe et al.
2018](https://www.osti.gov/pages/biblio/1409737), reports overall system
efficiency 8 to 13 points below conversion efficiency for primary control
reserve and PV-battery duty on a 192 kWh prototype, and names the reason
directly: auxiliary consumption dominates at low utilization. A 200 MWh plant
at 312 cycles a year is the opposite case, so landing well under that range is
the expected direction and not a contradiction. It is also why the share is
gated only by a wide sanity bound. The same hardware would read several times
this share on a plant that mostly sits still, so a narrow band on it would be
measuring the dispatch plan, not the plant.

**The transformer is the clearest thing this run shows.** Its no-load loss is
100 kW whether or not the plant is doing anything, which is 876 MWh over 8760
hours: more than half of the transformer's annual total. On the single 0.5C
cycle that M0.5 was gated on, the same 100 kW spread over nine hours and
disappeared into the third decimal. Nothing about the component changed
between those two measurements. The window did.

**What this number does not include.** Read it with these in view, all of them
scheduled rather than forgotten:

- **No degradation and no outages.** Capacity fade arrives in M5 and faults
  and partial availability in M2, so this is a first-year plant that never
  loses a block.
- **Auxiliary transformer and LV distribution losses are not modeled.** The
  main step-up transformer is; the small transformers feeding the house load
  are not.
- **The cooling coefficient of performance is constant.** It does not fall
  with outdoor temperature, which understates auxiliary energy on exactly the
  days cooling runs hardest. Documented in the thermal table below.
- **One dispatch pattern, one weather year, one site.** A fixed daily price
  shape over DWD Lindenberg 2024. The annual figure is a property of the pair,
  plant and duty, not of the plant alone.
- **The PCS curve has no voltage dimension** and charging reuses the
  discharge curve; both arrive in M3.

## M1: auxiliary inventory

Until this step the plant's house load was one 150 kW constant living inside
the substation model, covering everything that was not HVAC. It could not be
attributed, could not be checked against anything, and did not change when
the plant did. PR5 replaced it with four itemized loads, which together with
the HVAC draw from the thermal layer make up every watt the site consumes for
itself.

| Item | Value | Basis | Status |
|---|---|---|---|
| Rack battery-management electronics | 71.8 W per rack (34.5 kW site) | Schimpe et al. 2018 measured 287 W of battery-side control and monitoring on a container system of 8 racks x 13 modules x 16 cell blocks. Scaled by monitored cell count, since sensing channels are what set the electronics count: 0.172 W per cell block, times the 416 cells a GW-01 rack monitors. The source figure is already corrected for the 91.5% efficiency of the 24 V supply it measured, so this one carries that correction too | referenced, scaled. Two assumptions ride on the scaling. It takes per-cell monitoring at both ends, standard for utility LFP racks but not stated in the source. And it scales the whole battery group per cell, including the rack-level master units, which do not multiply with cell count: a GW-01 rack monitors twice the cells of the reference rack, so its master contribution is counted twice over. Both push the figure up, by a few watts per rack |
| PCS standby tare | 339.8 W per idle block (6.8 kW site) | the CEC inverter database publishes night tare, the draw of a unit that is energized and not delivering, as 169.9 W for the Sungrow SC2500UD-US at its 2.507 MW rating. That is the same database entry this plant's efficiency curve is fitted to; a 5 MW block is two such units | referenced |
| Plant control, protection, SCADA | 15 kW, site constant | substation protection and control with its DC systems and telecom, site EMS and SCADA, per-block controllers and communications | estimate. Together with the line below, the part of this inventory that most wants a source |
| Lighting and safety systems | 10 kW, site constant | fire and gas detection per container, security and access control, site and building lighting averaged over the day. An enumerated row, not a remainder: nothing lands here for failing to fit elsewhere | estimate. Lighting is modeled as a flat average rather than a night load |

**Sources:** [Schimpe et al. 2018, Applied Energy 210, 211-229, Table
3](https://www.osti.gov/pages/biblio/1409737) (control and monitoring
consumption of a 192 kWh container system, 287 W battery / 422 W power
electronics / 81 W system), and the CEC inverter database entry for the
Sungrow SC2500UD-US as distributed with NREL SAM (see DATA-LICENSES.md), the
`Pnt` night-tare field of the same record used for the efficiency curve.

Two rules prevent the same watts from being billed twice, both held by tests
in `crates/bess-models/src/aux.rs`:

- A converting PCS does not pay the standby tare. Its self-supply is already
  inside the load-independent term of the efficiency curve.
- Rack circulation fans stay inside the HVAC item, where their power is
  computed, rather than becoming a fifth station line.

Measured on the same replayed days as the thermal record (seed 7, GW-01 on
the internal dispatch plan). Every figure below is inside a band held by CI:
the daily energy by `the_published_daily_totals_still_hold`, the item split
by `the_published_item_split_still_holds` (both in
`crates/bess-models/tests/aux_inventory.rs`), and the share of import by
`the_published_calibration_readings_still_hold`. The 66.3 kW station total is
held by `the_idle_station_load_is_what_the_record_says`.

| Reading | 1 January | 14 July |
|---|---|---|
| Auxiliary energy over the day | 2.7 MWh | 5.4 MWh |
| Share of energy imported at the POI | 2.6% | 5.1% |
| HVAC | 42% | 71% |
| Rack electronics | 31% | 15% |
| PCS standby | 5% | 2% |
| Controls and protection | 13% | 7% |
| Lighting and safety systems | 9% | 4% |

What the itemization changed, stated plainly: the station load fell from the
150 kW placeholder to 66.3 kW, so the auxiliary share of a January day moved
from 4.4% to 2.6% and of a July day from 6.9% to 5.1%. The single-cycle M0.5
round-trip gate moved with it, 0.9186 to 0.9207, still inside [0.90, 0.93].
The placeholder was never sourced, and nothing here was tuned to keep it:
four items were sized independently and the total is what they sum to.

Known gaps in this inventory, recorded rather than hidden:

- **Auxiliary transformer and LV distribution losses are not modeled.** The
  main step-up transformer is; the small transformers feeding the house load
  are not, which understates the station total by a few kW.
- **Two of the four items are engineering estimates.** They are 25 kW of the
  66.3 kW, so a sourced replacement can move the station load by a third.
- **Nothing here varies with temperature or time of day** except the HVAC
  item. Real control rooms are air conditioned and real lighting is a night
  load.

## M1: thermal parameter provenance

The gate above says what the plant did over a year. This section says what it
did it with. The two belong together and neither replaces the other: a
measurement is only worth its parameters, and half of these are estimates. An
estimate that says it is an estimate is honest; one that reads like a
measurement is not.

| Parameter | Value | Basis | Status |
|---|---|---|---|
| Container air capacitance | 2.8e6 J/K | ~6 t of enclosure steel and rack frames at the ~470 J/(kg K) of structural steel, plus a negligible 40 kJ/K of air | estimate |
| Rack cell capacitance | 2.33e6 J/K | a 418 kWh rack at ~180 Wh/kg cell level is ~2330 kg of cells, at the ~1000 J/(kg K) reported for LFP | estimate; the rack energy is fixed by the site descriptor, the energy density and the specific heat are not |
| Rack-to-air conductance | 900 W/K | sized for a ~9 K cell-to-air spread at the ~8 kW a rack dissipates at full site power | dependent estimate: that 8 kW follows from the M0 equivalent-circuit resistances, which `cell.rs` calls tuned rather than sourced. Refining them in M1 requires revisiting this number, or the 9 K spread silently becomes something else |
| Envelope conductance (UA) | 500 W/K | M0 placeholder | estimate |
| Sol-air coefficient | 0.026 m2 K/W | ASHRAE Handbook of Fundamentals, light-colored surface (0.052 for dark). The effective solar aperture is derived from it, `UA * alpha / h_o` = 13 m2, never stored separately | referenced |
| HVAC cooling capacity | 2 x 56 kW thermal | STULZ WXUC5, a BESS-dedicated unit: a 35 kW monoblock on each short side of a 40 ft battery container, in the DECCI installation (22 MWh over seven containers, about 3.14 MWh each). GW-01's container holds 5.02 MWh, so the same two-unit topology scaled by container energy gives 56 kW per unit | referenced topology and unit size, scaled. Scaling by energy assumes a comparable C-rate |
| HVAC cooling setpoint | stage 1 at 26 C, stage 2 at 29 C | the reference installation holds 25 C inside a battery container; the bands sit around it | referenced target, estimated bands |
| Cooling coefficient of performance | 3.0, constant | container datasheets publish cooling capacity but not input power | estimate, mid-range for a packaged direct-expansion unit. **Known bias:** a real unit's coefficient falls with outdoor temperature, roughly 4 at 20 C ambient to 2.5 or below at 40 C. A constant value therefore understates auxiliary energy on the hot days when cooling runs hardest and overstates it in winter. The design fixes one coefficient per mode at this depth; the annual auxiliary share has to be read with that bias in view |
| Electric heating | 13 kW, resistive | same installation: heating during the battery's standby phase, done with multi-stage electric heaters. Resistive, so its coefficient of performance is exactly 1 | referenced. Modeled as one stage rather than several |
| Anti short-cycle interval | 180 s minimum run and minimum off | the usual interval for scroll compressors | estimate. Applies to compressors only: a running unit may start its neighbour at once, a stopped one waits however hot it gets, and electric heating is not gated by it at all |

**Sources:** [STULZ, cooling containers for the DECCI battery storage
project](https://www.stulz.com/projects/sma-altenso/) (WXUC5 unit, 35 kW per
monoblock, 25 C target, 13 kW multi-stage electric heaters), [STULZ WallAir
series](https://www.stulz.com/en-de/products/detail/wallair/) (the platform
the WXUC5 is built on: -20 C winter to +50 C summer envelope).

Measured over replayed days once the unit was sized (seed 7, GW-01 on the
internal dispatch plan). Every figure in this table is held inside a band by
`the_published_calibration_readings_still_hold` in
`crates/bess-models/tests/hvac_duty.rs`, so a model change that moves one of
them fails CI instead of quietly leaving this record stale:

| Reading | 1 January | 14 July |
|---|---|---|
| Container air | 14.7 to 26.0 C | 18.6 to 29.0 C |
| Peak cell temperature | 33.7 C | 38.0 C |
| Stage 1 / stage 2 duty | 5% / 0% | 17% / 1% |
| Compressor starts per container per day | 5.0 | 35.1 |
| Auxiliary energy, share of import | 2.6% | 5.1% |

The July figures are the point of the exercise: before this step the same day
left containers at about 36 C air and 42 C cells against a 27 C setpoint,
because the plant out-produced its cooling. Compressor starts land at roughly
one every 40 minutes on the hard day, which is what the 3 minute minimum run
and minimum off times are there to guarantee.

Three observations that belong here rather than in a commit message:

- **Cell temperature is no longer a derived value.** It integrates, so the
  Modbus cell-temperature registers lag and spread instead of tracking
  container air plus a constant. Same registers, different dynamics.
- **Heating never ran on a dispatching January day.** The batteries warm
  themselves; the heater is a standby-phase load, exactly as the reference
  installation describes it. It does appear on an idle winter day, which is
  what the `an_idle_winter_day_brings_the_heater_on` test holds.
- **The auxiliary share above is not yet the gate.** It is auxiliary energy
  over import for a single day. The M1 gate is auxiliary share of annual
  throughput against a band sourced in PR7, measured by `bess-bench` over the
  full replayed year. The share fell when the station constant was itemized
  in PR5; both readings above are post-inventory.

## Planned gates (from ROADMAP.md)

- **M1:** annual RTE in the 80-85% field band (CAISO/EPRI fleet reports);
  realistic auxiliary share of throughput.
- **M2:** failure type/frequency distribution follows the EPRI incident
  taxonomy.
- **M3:** efficiency surfaces f(P, V_dc) match the Sandia/CEC database; the
  M1 RTE gate still holds.
- **M4:** simulated annual revenue inside public German fleet index bands.
- **M5:** capacity fade inside published LFP field bands.
