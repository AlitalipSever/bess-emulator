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

## M1 (v0.3.0): the annual measurement

This is the M1 gate. Everything else in this file is a parameter, a fit, or a
single day. This section is the plant run against the whole replayed weather
year, 31.5 million ticks in one process, measured at the point of
interconnection, with every loss read off a meter the kernel kept while it
ran rather than re-derived afterwards from a summary.

<!-- bess-bench:begin m1-annual -->

Measured by `bess-bench` on kernel 0.7.0: GW-01 on the internal dispatch
plan, seed 7, 365 simulated days (31 536 000 ticks) of the replayed weather
year. Regenerate with `cargo run --release -p bess-bench -- --write`; CI
fails if this block is stale.

| Gate | Band | Measured | Verdict | Band drawn from |
|---|---|---|---|---|
| Annual round-trip efficiency at the POI | 80.00% to 85.00% (sourced) | 83.23% | inside | EIA-923 fleet average 82% (2019); NREL ATB 2024 design assumption 85% |
| Auxiliary share of energy imported | 1.00% to 8.00% (sanity bound) | 2.85% | inside | no public dataset publishes this at plant scale; see the note below |
| Reported SoC, mean absolute error (points) | 5.00% to 200.00% (sanity bound) | 25.30% | inside | no public rack-level estimator error at plant scale; see CALIBRATION.md |
| Reported SoC, largest error on any rack (points) | 20.00% to 1500.00% (sanity bound) | 113.00% | inside | ACCURE 2025 ESS report field range, errors up to 15 points |

Energy at the point of interconnection over the run:

| Quantity | Value |
|---|---|
| Imported | 66 655.7 MWh |
| Exported | 55 478.3 MWh |
| Round-trip efficiency | 0.8323 |
| Round-trip efficiency, house load removed from import | 0.8568 |
| Equivalent full cycles, exported energy over nameplate energy | 276.5 |
| Stored energy, end minus start | 93.8 MWh |
| Auxiliary share of import / of export | 2.85% / 3.43% |
| Unexplained residual | 0.00063% of throughput, gate below 0.2% |

Three definitions the figures above depend on. The round-trip ratio is
uncorrected for the stored-energy endpoints, matching how the fleet figure
it is gated against is computed; the row below it takes the house load back
out of import arithmetically rather than by re-running the plant without it.
Cycles count exported energy against nameplate energy, not against the
smaller window the BMS keeps the plant inside. Container air is read every
tick; the rack-level temperatures below are sampled once a minute, which is
far inside their thermal time constant.

Where the energy went, each category on its own meter:

| Category | MWh | Share of import |
|---|---|---|
| Battery | 4 862.9 | 7.30% |
| PCS conversion | 2 742.3 | 4.11% |
| Transformer | 1 577.1 | 2.37% |
| Auxiliary: HVAC | 1 335.0 | 2.00% |
| Auxiliary: rack electronics | 301.9 | 0.45% |
| Auxiliary: PCS standby | 46.2 | 0.07% |
| Auxiliary: controls and protection | 131.4 | 0.20% |
| Auxiliary: lighting and safety | 87.6 | 0.13% |
| **Total** | **11 084.4** | **16.63%** |

Temperatures and HVAC over the run:

| Reading | Value |
|---|---|
| Ambient, coldest to warmest | -12.1 to 33.3 C |
| Container air, coldest to warmest | 10.0 to 29.0 C |
| Cells, coldest to warmest | 8.7 to 39.0 C |
| Duty, one unit / both units / heating | 16.10% / 0.95% / 0.25% |
| Cooling starts per container | 4 760.0 |

Alarms raised over the run: 0 trips (gate: none), 38 932 warnings.

| Alarm | Raised |
|---|---|
| `block.setpoint_not_met` | 15 300 |
| `rack.derate_active` | 22 917 |
| `site.power_limited` | 715 |

Fiscal quarter-hours closed: 35 040 (gate: exactly 35 040), 92 on the 23-hour day and 100 on the 25-hour day (gates: 92 and 100).

Reported SoC against the truth, over every rack: mean absolute error 0.253 points (gate: 0.05 to 2.00 points), largest 1.13 points (gate: 0.20 to 15.00 points); 104 891 corrections, 0.599 per rack and day.

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
efficiency curve fitted to a real database entry; and heavily used, at the
cycle count in the table. Utilization is the one that matters most.
Every fixed load the plant carries, the controls, the lighting, the standby
tare, the transformer's no-load loss, is divided by throughput when it reaches
this ratio, and this plant has a great deal of throughput to divide by.

**Reading the auxiliary share.** What the house load costs in efficiency is
the gap between the two round-trip rows in the table above, and it is a small
number here. The closest peer-reviewed decomposition of the same kind,
[Schimpe et al. 2018](https://www.osti.gov/pages/biblio/1409737), reports
conversion round-trip efficiency of 70 to 80% and overall system efficiency 8
to 13 points below it for primary control reserve and PV-battery duty on a
192 kWh prototype, and names the reason directly: auxiliary consumption
dominates at low utilization. A 200 MWh plant running the cycle count in the
table is the opposite case, so landing well under that range is the expected
direction and not a contradiction. It is also why the share is gated only by a
wide sanity bound. The same hardware would read several times this share on a
plant that mostly sits still, so a narrow band on it would be measuring the
dispatch plan, not the plant.

**The transformer is the clearest thing this run shows.** Its no-load loss is
100 kW whether or not the plant is doing anything, so over 8760 hours it is
876 MWh: more than half of the transformer's annual total in the table above.
That value is pinned by `the_transformer_no_load_loss_is_what_the_record_says`
in `crates/bess-models/src/grid.rs`, so this paragraph cannot quietly outlive
it. On the single 0.5C cycle that M0.5 was gated on, the same 100 kW spread
over under seven hours and moved the result by about four parts in a thousand.
Nothing about the component changed between those two measurements. The window
did.

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

## M1 (v0.3.0): auxiliary inventory

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

## M1 (v0.3.0): thermal parameter provenance

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

## M2 (v0.5.0): BMS temperature derating

The BMS now scales each rack's charge and discharge limit by a factor read
from cell temperature, multiplied onto the SoC taper. The factor is the
cell maker's own table, taken point for point.

| Cell temperature (C) | -30 | -20 | 0 | 5 | 10 | 15 | 45 | 55 | 60 |
|---|---|---|---|---|---|---|---|---|---|
| Charging, P-rate (Table 5) | | | 0.05 | 0.12 | 0.3 | 0.5 | 0.5 | 0.5 | 0 |
| Discharging, P-rate (Table 7) | 0 | 0.5 | 0.5 | 0.5 | | | 0.5 | 0.5 | 0 |

Charging below 0 C is prohibited by the same document, so the charging curve
starts there. The discharging table skips 5 to 45 C at 0.5P on both sides,
and the curve carries 0.5P across the gap. Between listed points the curve is
linear, because the specification gives points and not a step rule; that
choice is ours, not the document's. `listed_points_are_reproduced_exactly`
and `defaults_are_the_datasheet_tables` in
`crates/bess-models/src/bms.rs` and `bms/derate.rs` hold the numbers above
to the code.

**Only the shape is carried over, and that is a known gap.** The table peaks
at 0.5P, the cell's continuous rating. GW-01's racks are rated at 1C
(`max_current_a` = 314 A, an M0 parameter), so the factor is the table
divided by its own peak and applied to that rating. Read literally, the
cell would cap a rack at half its site rating in the comfort band, and the
plant, which draws 0.498 of rack rating at full PCS power, would sit right
on that cap. Aligning the rack rating with a named cell changes full-power
capability and every annual figure below, so it is a decision of its own
and not a side effect of this one. The Hithium 314 Ah specification (1P
continuous, the same 0 to 60 C charging and -30 to 60 C discharging window)
fits the 1C rating but publishes no table.

Measured on replayed days (seed 7, GW-01 on the internal dispatch plan),
held by `crates/bess-models/tests/bms_derating.rs`:

| Reading | 1 January | 14 July |
|---|---|---|
| Cell temperature range | 13.6 to 33.7 C | up to 38.0 C |
| Lowest temperature factor on any rack | 0.89 (charging) | 1.00 |
| Binds against dispatch | no | no |

**Derating is capability, not yet a change to the record.** A winter morning
moves the charging factor, because cells dip below the 15 C knee, but 0.89
of rack rating is still well above the 0.498 the plant draws. July cells
stay 17 K under the 55 C shoulder. The annual measurement was re-run with
derating in place and every published figure held (`bess-bench --check`),
so the M1 record stands unchanged. Derating first shows in the plant when a
scenario takes the HVAC away or starts the site cold; phase 4 of M2 is where
that happens.

**Sources:** EVE Energy, Product Specification, Prismatic LFP Cell MB31
(314 Ah), PBRI-MB31-D06-01, version A, first issued 2023-11-20, Tables 3, 5,
7 and 10. No manufacturer-hosted copy was found; the copy used is
[a distributor mirror](https://www.gobelpower.com/download/EVE-314Ah-CB31-LiFePO4-Cell-Datasheet.pdf)
(filename says CB31, every page reads MB31), retrieved 2026-09-27.
Cross-check: Xiamen Hithium, Cell Technical Specification LFP71173207 /
314 Ah, HC-A314-SPEC-0001 V3.0, effective 2024-01-10, also through a
[distributor mirror](https://www.xihopower.com/uploadfile/2026/02/26/20260226135453ow8Ryn.pdf),
retrieved 2026-09-27.

## M2 (v0.5.0): cell spread and balancing

Each rack carries one number with dynamics, the SoC spread between its
highest and lowest cell, and publishes it the way a BMS does: as the
voltage spread that SoC difference produces on the OCV curve. The spread
widens every tick and passive bleed resistors close it again near the top
of charge.

| Parameter | Value | Basis | Status |
|---|---|---|---|
| Spread growth with time | 1.8 % SoC per month | self-discharge of 3.0 %/month (EVE MB31, after the first month), with a cell-to-cell spread of sigma = 10 % of the mean (Nuvation's simulation assumption); the highest and lowest of 416 cells sit about 6 sigma apart | referenced rate, referenced spread assumption, derived range |
| Spread growth with throughput | 100 ppm of SoC throughput | coulombic efficiency differences are measurable at the 10 ppm level (Yang 2015); Nuvation assumes sigma = 830 ppm, chosen high so imbalance shows | estimate, bracketed by the two |
| Commissioning spread | 0.2 to 0.6 % SoC, uniform per rack | what a factory top-balance leaves | estimate |
| Bleed current | 240 mA per cell | TI BQ79616 battery monitor, internal balancing at 240 mA; Nuvation's ESS bleed resistor (13 ohm at 90 % duty) works out to about 250 mA | referenced |
| Share of cells bleeding | half the string | cells above the lowest by more than the tolerance | estimate |
| Balancing window | rack SoC 0.90 and up, idle or charging, interrupted above 1 A discharge | Orion BMS starts LFP balancing with a cell within 5 to 10 % of full and balances in charge mode; Nuvation allows charge, low discharge or rest | referenced policy, estimated interrupt current |
| On / off thresholds | 30 mV / 20 mV | a published LFP BMS parameter sheet: start at 30 mV, stop at 20 mV | referenced |

Measured over 90 replayed days from 11 April (seed 7):

| Reading | Value |
|---|---|
| Days until a fresh plant first balances | about 27 |
| Spread once balancing runs | saws between 2.2 and 2.75 % SoC |
| Voltage spread at SoC 0.95 at the thresholds | 30 mV on, 20 mV off (by construction) |
| Racks bleeding at once, busiest 10 minutes of a day | 17 to 94 of 480 |
| Balancing hours on one rack | 61 in 90 days |

Over the replayed year (the generated block above), balancing added 26.5 MWh
to the battery loss row, which is where bleed heat is booked, and the HVAC
drew 7.6 MWh more to remove that heat. Round-trip efficiency fell from
84.22 % to 84.17 %; both gates still hold. `tests/balancing.rs` holds one
tooth of the sawtooth on a replayed day, with the energy balance closing
over it.

**Known gaps.**

- **The plateau reads too tight.** At mid SoC a 2.5 % spread shows as 2 to 3 mV
  on this OCV curve. Field data from a 300 Ah LFP container (396 cells per
  string, eight months) averages 12 to 21 mV and reaches 63 to 113 mV at the
  end of charge.
  - The missing part is resistance and temperature spread between cells. That spread shows up under current, while this model reads only the open-circuit curve.
  - The top-of-charge figure is also out of reach: the window stops at SoC 0.95, below the steep knee where those readings come from.
- **Rack mean SoC does not self-discharge.** Only the spread drifts. The
  cell model has no self-discharge, so 3 %/month of mean loss is missing
  from the stored-energy account.
- **Drift is the same for every rack.** Racks differ only in their commissioning draw.
  - They therefore reach the threshold within days of each other.
  - This is the uniformity the phase 1 plan flagged for the imbalance alarm.
- **No measured growth rate exists to gate against.** No open source found
  states an SoC imbalance growth rate for LFP ESS strings, so the growth
  law is built from its causes, not fitted to an observed rate.

**Sources:** EVE MB31 specification (as in the derating section; Table 3
self-discharge). [Nuvation Energy, cell balancing white paper](https://nuvationenergy.com/wp-content/uploads/2023/04/nuvation-energy-whitepaper-cell-balancing-bms.pdf)
(Table 1 simulation assumptions, bleed resistor and duty). [TI BQ79616
datasheet](https://www.ti.com/lit/ds/symlink/bq79616.pdf) (240 mA cell
balancing). [Orion BMS balancing parameters](https://www.orionbms.com/manuals/utility_o2/param_balancing_description.html).
[LFP BMS parameter sheet](https://akkudoktor.net/uploads/short-url/1oLsGvxsgF1eFVtFNk2cYVH9P9d.pdf)
(vendor not named; 30 mV start, 20 mV stop). Yang et al. 2015, J. Power
Sources, coulombic efficiency measurement (abstract only). [Field
voltage spreads, 300 Ah LFP container](https://arxiv.org/pdf/2601.03007).
Retrieved 2026-09-27.

## M2 (v0.5.0): alarm thresholds

The three alarm words are laid out in `bess_core::alarms::layout`. Each
threshold rests on the physics it watches, and the bench gates one claim
about all of them: a clean replayed year trips nothing.

| Bit | Raise / clear (or trip) | Basis | Status |
|---|---|---|---|
| `rack.over_temp_warning` / `_trip` | 50 / 47 C; trip 60 C | EVE MB31: power falls from 55 C and reaches zero at 60 C, where the datasheet says to stop charging | referenced limits, estimated warning margin |
| `rack.under_temp_warning` / `_trip` | 0 / 2 C; trip -30 C | EVE MB31: charging prohibited below 0 C; -30 C minimum operating temperature | referenced |
| `rack.imbalance_warning` / `_trip` | 120 / 100 mV; trip 500 mV | 0.12 V is the imbalance threshold in the published analysis of eight months of LFP container data; 500 mV is the voltage-difference fault of a published LFP BMS parameter sheet | referenced |
| `rack.derate_active` | factor below 0.98, clear at 0.995 | the lower of the two temperature factors | estimate |
| `rack.soc_high` / `soc_low` | 1 % past the window, clear at the edge | the plant sits at its window edges daily by design, so only a violation counts | estimate |
| `block.setpoint_not_met` | miss above 2 % of PCS rating for 10 s, clear below 1 % | the M0 PCS has no ramp, so the deadband is not sized against ramp lag | estimate |
| `block.container_over_temp` | 40 / 37 C air | 11 K above the warmest replayed July air, under the 45 C top of the EVE MB31 recommended range | estimate |
| `site.power_limited` | delivery short by 2 % of site rating (2 MW), clear below 1 % | a shortfall a dispatcher would act on | estimate |

Measured over the replayed year (the generated block above):

- **Zero trips.** This is the gate.
- **Warning raises:**
  - `block.setpoint_not_met`: 14 600, which is two per block per day;
  - `site.power_limited`: 730, two per day;
  - `rack.derate_active`: 17 707, about 37 per rack, all on cold mornings when cells dip below the 15 C knee of the charging table.
- **What the daily warnings mean.** The reference dispatch plan asks for 100 MW for longer than the SoC window holds, so the plant runs short at both ends of every cycle and says so. That is the plan's behavior, not a fault. A dispatcher under test that stays inside the window sees none of them.
- **Temperature never approaches a warning.** Cells stay between 9 and 38 C.

The causal chain test (`tests/alarm_chain.rs`) needs a whole block's HVAC
down to reach the site bit. With one container failed, the physics limits
itself. Its racks derate, the block delivers less and makes less heat, and
it settles near 59 C, about 1.6 MW short. That is under the 2 MW site
threshold, so the site bit stays down.

## M2 (v0.5.0): trip, return and station service

Phase 3 lets the plant be partly there: a rack isolated, a block in
maintenance, one PCS tripped, or the whole site tripped by protection and
brought back. Two figures stand behind it; neither moves the annual
record, because the replayed year never trips.

| Figure | Value | Basis | Status |
|---|---|---|---|
| Return spacing (`ReturnPolicy::spacing_s`) | 30 s per block, block 19 at 570 s | One GW-01 block is 5 % of the site, so a block every 30 s averages 10 % of rated power per minute, the reconnection gradient the German connection codes ask of a plant coming back after a protective disconnection. VDE-AR-N 4105 states it for the low-voltage case in published summaries; the 110 kV text (VDE-AR-N 4120) is not freely available | estimate anchored to that gradient; the per-block spacing itself has no public source |
| Station service while the breaker is open | the full house load, on its own meter | No current crosses an open breaker, so the POI reads zero and its meters stand still. HV-connected plants keep cooling, protection, controls and fire detection supplied through a separate station-service feed with its own meter, backed by UPS | modeled as a third substation meter, `site.meter.station_service_kwh`; losing that supply too is an M3 fault |

The release is a 5 MW step per block, not a ramp: the M0 PCS has no
ramp, and the default matches the average gradient, not its shape. A
maintained converter stops paying its 339.8 W standby tare (it is locked
out with its disconnects open); a tripped one keeps paying it.

`tests/trip_return.rs` holds the observable story: a trip at full
discharge on the replayed July afternoon takes the POI from about
100 MW to zero in one tick while the station-service meter takes over
the house load, and after the site reset block k first runs k x 30 s
later, the POI climbing in twenty steps.

## M2 (v0.5.0): real prices and the revenue meter

Phase 3 PR2 replaces the M0 placeholder curve with real prices. The
planner keeps its rule (each day's three cheapest hours charge at full
power, its two dearest discharge) and now ranks the hours of each
Europe/Berlin day of the replayed price year.

| Dataset | Value | Source | Status |
|---|---|---|---|
| Day-ahead price, DE-LU, 2024 | 8784 hourly prices, the local year 2023-12-31 23:00 UTC to 2024-12-31 22:00 UTC; mean 78.51 EUR/MWh, minimum -135.45, maximum 936.28, 457 negative hours | Bundesnetzagentur \| SMARD.de, CC BY 4.0 (see DATA-LICENSES.md) | bundled, hash pinned; no hour filled |
| Local days | 366, of which 2024-03-31 has 23 hours and 2024-10-27 has 25 | derived from SMARD's weekly block stamps by `compile-prices` | the emulator's DST days are the reference year's, as its weather is |

The annual record before and after the switch, same plant, same seed,
same weather; reported, not tuned:

| Figure | Placeholder curve | Replayed 2024 prices |
|---|---|---|
| Round-trip efficiency at the POI | 0.8417 | 0.8366 |
| Auxiliary share of import | 2.60 % | 2.71 % |
| Import | 74,406.4 MWh | 66,484.8 MWh |
| Export | 62,629.7 MWh | 55,622.2 MWh |
| Equivalent full cycles | 312.2 | 277.2 |
| Compressor starts per container | 6,819.5 | 4,847.9 |
| Warnings raised | 33,037 | 40,449 |

Both gates still hold. The fall in throughput is a finding about the
rule, not about the prices. The rule ranks hours without ordering them,
and the placeholder curve always put its trough (03:00 to 05:00) before
its peak (18:00 and 19:00). Real days do not: on 65 of the 366 local
days the first discharge hour comes before the first charge hour,
typically a 07:00 or 08:00 morning peak ahead of a 12:00 to 13:00 solar
trough. Discharging a plant that the day before already emptied moves
nothing, so 28 local days export nothing at all and 58 export less than
half of the best day's 197.6 MWh; `block.setpoint_not_met` and
`site.power_limited` raise more often for the same reason. Charging moves
from the night into the warm midday, which is why derating raises grow
(17,707 to 24,384) and the cooling pattern changes. A planner that orders
charge before discharge, or looks at state of charge, is market
behaviour, and that is M4's; this phase gives the plan real days, not a
better strategy.

The same unordered plan breaks the monthly figures of the study series.
Month-end state of charge now swings by up to a full plant (about
185 MWh in September, November and December), and a month's plain export
over import reads that as loss in the month that ends full and as gain
in the one that empties it: September 0.802, November 0.875. The season
was still there, hidden. The series therefore also publishes each month's
change in stored energy and an endpoint-corrected round trip, export plus
that change over import. Corrected, January reads 0.848 against August's
0.830, the 1.8-point seasonal spread the series test holds; the
correction is first order (the stored change counted as if returned
without the discharge path's losses) and is good to a few tenths of a
point. The plain ratio stays in the series, as it was.

The fiscal meter closes 35,040 quarter-hours over the run (365 days; the
common simulated year skips the reference leap day), 92 of them on the
23-hour day and 100 on the 25-hour day. The bench gates the three counts
as exact values, since they are arithmetic. The reference year itself has
35,136 quarter-hours, pinned in `bess-data`. Energy is metered and the
price is known, but no EUR figure is computed or published anywhere until
M4 sources the band it would be held against.

## M2 (v0.5.0): the rack self-discharge fault

Phase 4 adds the one fault no earlier phase built: one cell in a rack's
string leaks faster than its neighbours, a soft internal short. The
fault sets a per-rack excess rate, and the spread law adds it to the
healthy drift above (1.8 % SoC per month). Nothing else changes: the
consequences are the balancing and imbalance behaviour already
calibrated in the two sections before this one.

| Parameter | Value | Basis | Status |
|---|---|---|---|
| Excess self-discharge | 0.1 to 100 % SoC per month, per scenario | a soft internal short is described in the literature as tens of times a healthy cell's leak; no public dataset pins a figure for a 314 Ah LFP cell | estimate; the schema's range, not a claim about any cell |
| Carried as | integer ppm of SoC per day (1 %/month = 333) | keeps the fault event exact under the determinism digest | by construction |
| Spread ceiling | 1.0 (the whole window) | a cell cannot hold less than empty | by construction |

Measured from 11 April (seed 7) on one container of twelve racks, rack 5
leaking, the day its `rack.imbalance_warning` first raises:

| Excess rate | Warning | Balancing hours on the leaking rack, first 90 days |
|---|---|---|
| none (healthy) | never | 62 |
| 3 %/month | never | 176 |
| 10 %/month | never | 450 |
| 13.5 %/month | day 36 | 587 |
| 18 %/month (ten times the healthy drift) | day 20 | 629 |
| 30 %/month | day 5 | 656 |
| 100 %/month | day 2 | 662 |

At 100 %/month the spread meets the ceiling after about a month and
`rack.imbalance_trip` (500 mV) latches on day 35; no slower rate trips
inside 90 days.

The bleed is the reason for the step between 10 and 13.5 %/month. It
removes at most 0.076 % SoC an hour and runs only at the top of the
window, about 7.5 hours a day in this spring, so it absorbs a leak up to
roughly 17 %/month on a good day and less on a day that does not reach
the top. Below that the leak shows only as more balancing; above it the
spread climbs. The healthy neighbours peak at 78 mV and never warn.
`tests/self_discharge.rs` holds the 18 %/month row (a warning inside 30
days, no neighbour warning) and the arithmetic behind it.

**Known gaps.** The leak drains the spread, not the rack's mean SoC, as
for the healthy drift: what one cell of 416 loses is below anything the
energy account shows. There is no cell undervoltage: the model has no
per-cell voltage limit, so an emptied weak cell is visible only through
the imbalance bits.

## M2 (v0.5.0): the scenario library against the incident record

The roadmap's M2 gate reads "injected failure types and frequencies
follow the public EPRI failure incident taxonomy (controls and
balance-of-system dominant, cells rare)". What is calibrated is the mix
of stories the library ships, not a simulation measurement: the gate is
a distribution check over the taxonomy tags of `scenarios/`, held by
`bess-scenario`'s `library` test on every build.

**The snapshot.** EPRI, *Insights from EPRI's Battery Energy Storage
Systems (BESS) Failure Incident Database: Analysis of Failure Root
Cause*, white paper 3002030360, May 2024, written with PNNL and TWAICE.
At the time of writing the database held 81 incidents, of which 26 had
enough public information to assign a root cause and a failed element;
two carried dual classifications, so the shares are over 28. Retrieved
2026-10-15 from EPRI's public download of the paper. Figure 6, the
failed element:

| Failed element | Incidents | Share |
|---|---|---|
| Controls (BMS, EMS, plant controllers, sensing, communication) | 13 | 46% |
| Balance of system (PCS, cabling, enclosures, HVAC, fire suppression) | 12 | 43% |
| Cells and modules | 3 | 11% |

Figure 4, the root cause, for the record: integration, assembly and
construction 10 (36%), operation 8 (29%), design 6 (21%), manufacturing
4 (14%). The library tags by failed element, which is what a story
about a plant can show; the root cause of a failure is history a
scenario file does not carry. The database has grown since the paper;
its live page is an application without a printable snapshot, so the
paper's analysis is the pin, and the next pin is the next published
analysis.

**The library.** Fifteen cases on 2026-10-15, eleven of them failures;
seventeen since M3 phase 4 (`pcs-overload-budget`, a `bos` case, in
phase 3; `substation-earthing-work`, a maintenance case, in phase 4),
twelve of them failures:

| Class | Cases | Share | Snapshot | Distance |
|---|---|---|---|---|
| `bos` | 6 | 50% | 43% | 7 points |
| `controls` | 5 | 42% | 46% | 4 points |
| `cell` | 1 | 8% | 11% | 3 points |

Two calendar cases (the DST days) and three maintenance cases sit
outside the gate, since they are not failures. The gate holds each class within
15 percentage points of its snapshot share, the cell class smallest and
never empty: with a dozen failure cases one case is about nine points,
so the band admits one case of drift and refuses two. Deleting a `bos`
case leaves 4 of 10 (40%), inside; adding three `cell` cases makes
4 of 14 (29%), eighteen points out, refused with the composition in the
message. The library's mapping of the emulator's faults onto the
classes: HVAC failures, PCS and protection trips are balance of system;
every data fault (dropout, freeze, unit error, chatter, clock drift,
NaN burst, backfill) is controls, the sensing and communication class;
the self-discharge leak is the one cell story the model can tell.

**The runtime budget.** Every case plays headless against its committed
record in CI (`scripts/assert-library.sh`, the calibration job). On the
development machine (Apple M-series, release profile) the fifteen cases
take 37 s together, the ten-day hot week 14 s and the six-day leak 9 s
of it; the kernel steps about 60,000 ticks per second. Under a minute
is inside the budget, so nothing moves to release-tag frequency; the
decision is revisited when the library doubles.

## M2 (v0.5.0): the annual record at release

The record was regenerated once more for the version bump, as every
release regenerates it, and the figures did not move: the M2 plant
measured at v0.5.0 is the plant phase 3 PR2 measured. Over the
milestone, two phases moved the year, in order:

| Record | Round trip at the POI | Import | Export | Equivalent full cycles | What moved it |
|---|---|---|---|---|---|
| v0.4.0 | 0.8422 | 74,400.9 MWh | 62,657.9 MWh | 312.3 | the M1 plant |
| after phase 1 | 0.8417 | 74,406.4 MWh | 62,629.7 MWh | 312.2 | passive balancing: 26.5 MWh of bleed heat booked to the battery row, 7.6 MWh more cooling |
| after phase 3, v0.5.0 | 0.8366 | 66,484.8 MWh | 55,622.2 MWh | 277.2 | real day-ahead prices: an unordered plan on days whose peak comes before their trough |

Temperature derating (phase 1) never binds on the replayed year, the
alarm words (phase 2) are observation, the trip and the staggered return
(phase 3) never fire in a clean year, the scenario engine and the taps
(phases 4 and 5) touch nothing unless a file asks, and no file is loaded
in the annual run; so none of them moved a figure, and CI holds that for
each. The 0.8366 sits inside the M1 band (the measured EIA fleet floor to
the NREL ATB ceiling), as the generated block above records, with the
market rule's cost itemized in the prices section: a planner that orders
its hours is M4's, and the number will move again then. The fifteen
library records were regenerated for the same bump and moved in their
version string and nothing else, which is the evidence that the release
is the physics the phases measured.

## M2.5 (v0.7.0): OCV hysteresis

The cell's open-circuit voltage stopped being one curve. The 13-point LFP
table is now the mean curve, and a per-rack hysteresis state (Plett's
one-state model) shifts the rested voltage above it after charging and
below it after discharging, held at rest and moved only by throughput.
The envelope is half the measured charge-minus-discharge rest-voltage gap
(`LFP_HALF_GAP_V` in `crates/bess-models/src/cell/hysteresis.rs`).

**Gate, curve-pair fidelity,** held by
`rested_voltage_lies_on_two_branches_with_the_pinned_gap` in
`crates/bess-models/tests/ocv_hysteresis.rs`. A rack is brought to the
test SoC from ten points below at about 0.2C and rested fifteen minutes,
then from ten points above, and the two rested voltages are compared per
cell. Bands drawn before the measurement:

| SoC | Band (full gap, mV per cell) | Bound | Measured |
|---|---|---|---|
| 5 % | 32 to 44 | sourced: 38 mV peak near 5 % SoC (Barai 2015, text) | 35.9 |
| 50 % | 10 to 25 | sanity bound: the text gives no plateau value | 19.9 |

The 5 % reading sits 2 mV under the source's peak because the rack rests
at the SoC it crossed, a fraction above 5 %, on a knee where the envelope
falls 2 mV per percent. **The plateau level is an estimate**, chosen
inside the range the LFP literature reports and labeled as such: the
pinned paper plots the whole curve (its Figure 6b) and states only the
peak and where the maximum lies (5 to 25 % SoC), and this record quotes
nothing it did not read in text.

**Transition rate.** `gamma` = 75 covers 95 % of a transition within one
4 % SoC step, the step size of the source protocol, after which its
measured rest voltage lay on the new branch. The source publishes no
intermediate rest points (its own simulation sets the rate "arbitrarily"
at 50), so this is an interpretation of the protocol, not a fit, and is
held as one by `ninety_five_percent_of_a_transition_fits_one_source_step`.

**One envelope, one temperature.** The same group's 2016 measurement on
the same cell has the 25 C gap within 3 mV of the 45 C gap, 13 mV wider
at 0 C, and nearly rate-independent between 0.2C and 3C. GW-01's cells
spend the replayed year between 8.7 and 38.1 C, so one 25 C envelope is
carried and the cold widening is a known gap below.

**Energy.** The stored energy stays the integral of the mean curve, and
the hysteresis work `-h * i` is booked to the battery loss row with the
RC and ohmic terms, so the site balance closes to the same residual as
before (`the_energy_books_close_through_a_hysteresis_cycle`). The cost
is real: the 0.5C full-depth cycle gate measured **0.9129** (import
172.3 MWh, export 157.3 MWh) against 0.917 before the phase, 0.4 points
of round trip paid to the gap, inside the unchanged [0.90, 0.93] band.
The annual record moved as the table at the end of this section says:
0.7 points of round trip, still inside the M1 field band.

**Switch-off proof.** With `gamma` = 0 (`Ecm1Rc::lfp_314ah_rack_without_hysteresis`)
the four-hour determinism run, with the new state field cut out of its
serialization, hashes to the pre-phase golden digest
(`switched_off_the_plant_is_the_pre_phase_plant_byte_for_byte`).

**Known gaps.**

- **The plateau level is not a measurement.** A source that prints the
  gap at mid SoC in text, or a curve pair this repository may bundle,
  would turn the 50 % row from a sanity bound into a sourced one.
- **No temperature dependence.** The envelope is the 25 C one; a cold
  site rests on branches 13 mV further apart than the model shows.
- **The spread still reads through the mean curve.** `cell_dv_v` turns
  the cell SoC spread into a voltage on the mean curve; cells of one
  string sit on the same branch, so the branch cancels in the difference,
  but a string mid-transition would show a spread this does not.
- **Rested voltage is ambiguous now, and nothing yet exploits it.** The
  published SoC is still the truth; phase 2 is where an estimator meets
  this ambiguity.

**Sources:** Barai, Widanage, Marco, McGordon, Jennings, "A study of the
open circuit voltage characterization technique and hysteresis
assessment of lithium-ion cells", J. Power Sources 295 (2015) 99-107,
author manuscript in the
[Warwick Research Archive Portal](https://wrap.warwick.ac.uk/70160/)
(CC BY-NC-ND 4.0), retrieved 2026-10-21: the 38 mV peak near 5 % SoC,
the 5 to 25 % placement, the protocol (20 Ah LFP pouch, 25 C, 4 % steps
at 1C, 4 h rests, CV to C/20). Barai, Widanage, McGordon, Jennings, "The
influence of temperature and charge-discharge rate on open circuit
voltage hysteresis of an LFP Li-ion battery", IEEE ITEC 2016, author
manuscript in the same archive
([WRAP 81701](https://wrap.warwick.ac.uk/81701/)), retrieved 2026-10-21:
the temperature and rate findings. Model: Plett, "Extended Kalman
filtering for battery management systems of LiPB-based HEV battery
packs, part 2", J. Power Sources 134 (2004), the one-state hysteresis
form as restated in Barai 2015 section 2.3. Numbers cited, nothing
reproduced.

### The annual record after the phase

| Record | Round trip at the POI | Import | Export | Equivalent full cycles | What moved it |
|---|---|---|---|---|---|
| v0.6.1 | 0.8366 | 66,484.8 MWh | 55,622.2 MWh | 277.2 | the M2 plant |
| after phase 1 | 0.8296 | 66,777.9 MWh | 55,396.9 MWh | 276.1 | OCV hysteresis: 389.1 MWh more heat in the battery row (4,562.7 to 4,951.8), 129.8 MWh more HVAC to remove it, warmest cell 38.1 to 39.0 C |

## M2.5 (v0.7.0): the reported SoC

Each rack's BMS now carries an estimate of its own state of charge beside
the truth, and from phase 2 PR2 on the estimate is what the plant
publishes. The estimator is behavioral (`crates/bess-models/src/bms/estimator.rs`):
coulomb counting through a current sensor with an offset and a gain
error, a feeble pull toward the truth on the plateau standing in for the
little a flat OCV curve lets voltage say, and a hard correction when the
rack reaches a knee of the curve, where voltage finally can. Corrections
are events (`SocResnapped`), so the sawtooth is a sequence of facts.

**The sensor bounds the drift.** Each rack draws its sensor's offset and
gain error once at commissioning, uniformly inside the published class
of a shunt-based pack current sensor: the Isabellenhütte IVT-S at the
±500 A range, offset ≤ 75 mA, total accuracy ±0.4 % of reading. Over a
314 Ah rack, 75 mA is 0.57 points of SoC a day; the gain error cancels
over a balanced cycle and shows only as the error between knees. The
bound is a specification limit, not a distribution, and a BMS that
auto-zeroes its sensor at rest would sit well inside it; both are stated
here rather than modeled.

**Where it corrects.** The knees are where the mean OCV table leaves its
plateau, 10 % and 90 % SoC. A visit with more than 0.5 points of error
snaps the estimate to the truth plus a fifth of the error. The threshold,
the residual and the plateau pull (a ten-day time constant) are estimates
under sanity bounds: no public source gives them at plant scale.

**Gate,** sanity bounds in `bess-bench` (`ESTIMATOR_MEAN_ABS_ERROR_PCT`,
`ESTIMATOR_MAX_ABS_ERROR_PCT`), measured over every rack of the replayed
year and printed in the generated block above: the mean absolute error
has to sit between 0.05 and 2 points, the worst rack's error between 0.2
and 15 points. The ceiling is the field range ACCURE reports across more
than a hundred operating sites (errors up to ±15 points, with outliers
beyond); the floor says the estimator is not an oracle in disguise.
Measured: mean 0.253 points, worst 1.13 points, 104,941 corrections
(0.599 per rack and day). The energy figures did not move by a digit:
the generated block's round trip, import and export are phase 1's.

**The truth never moves.** `the_estimator_never_touches_the_truth` in
`crates/bess-models/tests/reported_soc.rs` holds the physics with the
imperfect estimator to the physics with a perfect one, byte for byte once
the estimator's three fields and the event log are cut out; dispatch,
limits, derating and alarms keep reading `soc`. That is a known gap, by
design: a real BMS acts on its estimate, and routing the estimate into
the limit chain couples estimator error into every calibration number at
once. It is deferred to a later iteration, so this phase changes what the
plant says before what it does.

**The chart material.** The study series (`calibration/m1-study-series.json`,
hot-week trace) carries the site-mean and one rack's true and reported
SoC side by side, which is the sawtooth over the smooth truth the phase
set as its acceptance bar; the published surfaces carry the reported
value alone from map 0.4.0.

**Known gaps.**

- **No plant-scale source for the correction behavior.** Threshold,
  residual and pull are labeled estimates; a published BMS log with
  correction sizes would turn the sanity bounds into a sourced band.
- **The offset is a bound, not a draw from the field.** Real offsets after
  auto-zero are smaller; the drift here is the class limit's.
- **The BMS does not know the state of health.** It counts against the
  nameplate capacity, which is right while `soh` is 1.0 and a second
  drift source once M5 ages the racks.
- **The limit chain reads the truth** (above).

**Sources:** Isabellenhütte Heusler, IVT-S datasheet, current channel
table (measurement ranges ±100 to ±2,500 A; offset ≤ 8 to ≤ 500 mA;
initial accuracy ±0.1 %, total ±0.4 % of reading; linearity 0.01 %),
[manufacturer PDF](https://www.isabellenhuette.com/hubfs/Files/Data-sheets/IVT-S.pdf),
retrieved 2026-10-21. Field range: D. Rüwald, "SOC errors in LFP
batteries: the $1 million per GWh challenge for BESS", Modern Power
Systems, 2026-08-06, citing the ACCURE 2025 Energy Storage System Health
& Performance Report (over 100 sites, more than 20 GWh): SoC errors up to
±15 %, outliers beyond 40 %, a 12-point daily difference in one field
example. Knee placement: the OCV table in `crates/bess-models/src/cell.rs`.
Numbers cited, nothing reproduced.

## M2.5 (v0.7.0): the instrument inventory

Everything the plant publishes now passes through an instrument, and the
instruments have classes. The inventory is explicit and small
(`crates/bess-emulator/src/instruments/inventory.rs`): the fiscal meter
behind the two `site.revenue.period_*_kwh` registers (IEC 62053-22 class
0.2S), the house-load meters behind the five `site.aux.*` items, their
total and the station-service register (class 0.5S), the substation's
CT/VT chain and each PCS's own measurement behind the POI power, reactive
power and voltage, the two SCADA energy registers and the per-block AC
and DC power (class 0.5), and a Pt100 class B thermometer behind every
published temperature. Everything else is exact and listed as such:
counters, states, enumerations, the control surface, the BMS's own
estimates (`soc_pct`, `cell_dv_mv`), the PCS efficiency ratio, the sun's
irradiance and the grid frequency, the last because a plant frequency
meter resolves the point's own 1 mHz LSB.

**The error model** (`instruments/model.rs`), per instrumented point, derived
from (seed, instrument, point, tick) and never stored, so a resumed run
publishes the same bytes (design D5, achieved):

- a slow bounded wander of the offset, three sinusoids of 3.1 days,
  16.8 hours and 5.3 hours with seeded phases, consuming 60 % of the
  class limit at the current load;
- white noise, bounded, with a sigma of 10 % of the class limit at the
  current load, plus a zero-reading floor of one part in ten thousand of
  full scale;
- the S-curve: the class's rated limit from 5 % of full scale up, its
  low-load limit at 1 %, log-linear between, flat below 1 % (the meter's
  starting range, where the class promises nothing and the model stops
  getting worse);
- cumulative energy registers carry one constant ratio error per
  register, drawn once inside 60 % of the rated limit, and no wander,
  so a published register stays monotonic as a real one does. This is
  a stated simplification of design D4 (a register that integrates its
  own measured power): the integral would be shell state and would need
  its own checkpoint story, which D5 set out to avoid.

**Gate, class compliance,** held by
`every_reading_stays_inside_its_class_at_every_load` in
`crates/bess-emulator/src/instruments/tests.rs`: every instrumented
point, at eleven loads from zero to 120 % of full scale in both
directions, at ticks sampled across a year, reads within the class limit
at that load plus three sigma of the zero floor. By construction the
wander and the noise sum to at most 90 % of the limit, so the gate is a
check on the arithmetic staying inside its own budget rather than a
measurement, and it is labeled that way. The design asked for this gate
in `bess-bench`; it lives in the shell crate instead, because the
instruments do.

**Two meters disagree.** `two_meters_on_one_quantity_disagree_by_a_bounded_wander`
runs the hot-plant fixture six hours and reads the POI power through its
analog and through the sum of the twenty block readings. The analog's
error over those six hours: worst 287 kW on a 100 MW scale, sigma 78 kW,
lag-one autocorrelation 0.87 between minute samples (a wander, not
noise), every sample inside its class limit. The two readings of the
same power differ by a slowly moving amount on top of the losses that
separate them in truth, which is the reconciliation gap a plant data
pipeline meets in its first week.

**Switch-off.** `--exact-instruments` publishes the truth on every surface
(the pre-phase bytes); the research record never saw an instrument, since
`bess-bench` reads the kernel.

**Known gaps.**

- **Energy registers do not integrate measured power** (above).
- **No temperature coefficient, no aging of the instruments.** The class
  is consumed at a fixed share; a meter near the end of its calibration
  interval would sit nearer the limit.
- **Irradiance and efficiency are exact.** A pyranometer has a class
  (ISO 9060) and the efficiency ratio is computed from two measured
  powers; both are left for a later inventory revision.
- **The one-sensor rule reaches no telemetry point yet.** The rack
  current sensor that drives the SoC estimator (`sensor_offset_a`,
  `sensor_gain`) is defined once on the rack, but no published point
  carries rack current, so D3's second consumer does not exist on the
  map; `cell_dv_mv` is the BMS's own reading and is left exact.
- **The viewer reads the kernel.** The browser module shows the truth,
  not an instrumented value; it is a research surface by that choice.

**Sources:** IEC 62053-22:2020, Electricity metering equipment, Part 22:
Static meters for AC active energy (classes 0.1S, 0.2S and 0.5S), the
percentage error limits at unity power factor: class 0.2S ±0.2 % from
0.05 In to Imax and ±0.4 % from 0.01 In, class 0.5S ±0.5 % and ±1.0 %,
as summarized publicly by
[Compere Power](https://comperepower.com/understanding-meter-accuracy-classes-0-2s-0-5s-and-1-0/)
(the standard itself is paywalled; only the limit values are used).
IEC 61869-2:2012, Instrument transformers, Part 2: Current transformers,
class 0.5 ratio error ±0.5 % at 20 to 120 %, ±0.75 % at 5 %, ±1.5 % at
1 % of rated current, as published by
[CTL Systems](https://ctlsys.com/support/ct-accuracy-standards/),
retrieved 2026-10-21. IEC 60751, industrial platinum resistance
thermometers, class B tolerance ±(0.30 + 0.005 |t|) K. Numbers cited,
nothing reproduced.

## M2.5 (v0.7.0): the annual record at release

The record was regenerated for the version bump, as every release
regenerates it, and the figures are phase 1's: round trip 0.8296, import
66,777.9 MWh, export 55,396.9 MWh, 276.1 equivalent full cycles. Of the
three phases only the first touched the physics; the estimator (phase 2)
is held to leave the truth untouched, byte for byte, and the instruments
(phase 3) live in the shell, which the bench never reads. The fifteen
library records moved in their version string and nothing else at the
bump. The 0.8296 sits inside the M1 field band, 0.7 points under v0.6.1,
paid to the hysteresis gap.

## M3 (v0.8.0): the efficiency surface and the DC bus

The converter stopped converting on one curve. `SurfacePcs`
(`crates/bess-models/src/pcs/sandia.rs`) carries the CEC inverter
database record of the Sungrow SC2500UD-US in the Sandia model's own
form, voltage terms included, per unit of its rating; the kernel hands
it the DC bus voltage every tick, the mean terminal voltage of the
block's in-service racks, which already carries state of charge,
current and the hysteresis of M2.5. In the same step the rack limit
became a current limit at the voltage the rack presents, so a block's
capability falls toward the bottom of its window and rises toward the
top.

**Gate, the surface at the reference voltage,** held by
`surface_matches_the_cec_reference_at_vdco_within_the_gate` in
`crates/bess-models/src/pcs.rs`: at Vdco (1,350 V) the surface
reproduces the seven M0.5 reference points within the M0.5 tolerance of
0.3 points; it reproduces them within 0.01 points, since they were
computed from the same record. The voltage axis is the record's own
(C1 = 3.22933e-6, C2 = 4.85948e-4, C3 = 4.84988e-5 per volt), so there
is nothing to fit and nothing to gate against beyond the record; a
sanity test holds the curve within one point of its Vdco value across
the rack window (1,206 to 1,435 V), and the inverse round-trips to 1e-9
at every voltage. The record's MPPT window, 1,300 to 1,500 V, sits
inside GW-01's rack window, so no reference shift was needed.

**What it moved.** The 0.5C cycle gate measured 0.9131 against 0.9129
before the phase: the cycle runs near Vdco and the surface and the M0.5
polynomial agree there to a hundredth of a point. Over the replayed year
the surface converts 48 MWh better than the single-voltage polynomial
did, 0.6 points of round trip, because the record's voltage terms make
the converter slightly more efficient below Vdco and GW-01's racks sit
below 1,350 V for most of the year; the annual record is in the table at
the end of this section, inside the M1 band.

**Known gaps.**

- **The charge direction mirrors the discharge surface,** the M0.5
  simplification carried; the database publishes one direction.
- **One record stands for every block;** a plant would carry a spread of
  converter efficiencies as it does of rack resistances.
- **The 1C rack rating stands** (M2's known gap about the cell's 0.5P
  table).

**Sources:** the Sungrow SC2500UD-US record of the CEC inverter database
as distributed with [NREL SAM](https://github.com/NREL/SAM)
(`deploy/libraries/CEC Inverters.csv`, BSD-3-Clause; underlying data
published by the California Energy Commission), retrieved 2026-10-21:
Paco 2,507,230 W, Pdco 2,563,700 W, Pso 6,066.41 W, Vdco 1,350 V,
C0 -8.03897e-9 1/W, C1 to C3 as above, Pnt 169.9 W. Sandia model form:
King, Gonzalez, Galbraith, Boyson, "Performance Model for Grid-Connected
Photovoltaic Inverters", SAND2007-5036. Numbers cited, nothing
reproduced.

### The annual record through M3

| Record | Round trip at the POI | Import | Export | Equivalent full cycles | What moved it |
|---|---|---|---|---|---|
| v0.7.0 | 0.8296 | 66,777.9 MWh | 55,396.9 MWh | 276.1 | the M2.5 plant |
| after phase 1 | 0.8302 | 66,739.3 MWh | 55,405.9 MWh | 276.1 | the surface at the racks' voltage: the PCS row fell 2,812.0 to 2,763.7 MWh, since the record converts a little better below Vdco and the racks spend most of the year there; current-bounded limits never bound against the plan |
| after phase 2 | 0.8323 | 66,655.7 MWh | 55,478.3 MWh | 276.5 | the setpoint follower: every hourly step of the plan is a ten-minute ramp now, so the plant spends the first minutes of each charge and discharge hour at partial load, where the battery and the converter both lose less per MWh (battery row 4,952.4 to 4,862.9 MWh, PCS row 2,763.7 to 2,742.3 MWh), and the HVAC row falls with the heat (1,365.1 to 1,335.0 MWh) |

## M3 (v0.8.0): the converter's state machine

A reset no longer drops a converter straight into the connected set.
`PcsOpState` grew from three values to eight (`Off`, `Precharge`,
`ContactorClose`, `Synchronize`, `Ramp`, `Standby`, `Run`, `Fault`), and a
cleared converter walks the three startup states for their durations
before it ramps and runs (`crates/bess-core/src/kernel/startup.rs`). The
plant controller's return still spaces the blocks 30 s apart; each block
now runs 16 s after its release, so a full return takes 9.5 minutes of
spacing plus the startup, and the site reports partial availability until
the last converter has synchronized.

**Durations, labeled estimates.** Precharge 10 s, contactor close 1 s,
synchronization 5 s. Utility converter manuals describe the sequence in
this order and in seconds, but no public manual found gives the three
figures for one unit, so these are the order of magnitude the manuals
describe, not a cited column, and the gate is the sequence's own
arithmetic: `a_cleared_converter_reaches_ramp_after_the_published_durations`
holds the walk to 0, 10, 11 and 16 s. The ramp itself takes its gradient
from VDE-AR-N 4110's 10 % of agreed power per minute, the public figure
the return's spacing already rests on; the next section has it.

**On the wire.** `blockNN.pcs.state` carries SunSpec's operating-state
numbering (models 701/702 `InvSt`: 1 off, 3 starting, 4 running, 7 fault,
8 standby); the three startup states share `starting` there and carry
their own names on MQTT and REST. Map 0.5.0, the major step promised
since M0.

**What it moved.** Nothing in a clean year: no block trips or is
maintained in the annual run, so the record did not move. The two trip
cases of the library and the maintenance case regenerated their records
for the new states and the later returns; their headers say so.

**Known gaps.**

- **The durations are estimates** (above); a public manual with the
  three figures would pin them.
- **`Ramp` lasts one tick** as a state: the ramp itself runs on under
  `Run` (next section), since the converter is converting while it
  climbs, which is what `Run` says.
- **No shutdown sequence.** A converter that is asked for nothing drops
  to `Standby` in one tick, as since M0; a graceful stop is not modeled.

**Sources:** SunSpec Alliance, SunSpec Device Information Model
Specification, models 701 and 702 (DER AC measurement and capacity),
operating-state enumeration, public specification; VDE-AR-N 4110:2018,
section on active power gradients after reconnection (public summary
cited at the M2 return policy). Numbers cited, nothing reproduced.

## M3 (v0.8.0): the setpoint follower

A converter does not jump to the share the plant controller hands it.
Each block's share passes a dead time, a ramp limit and a first-order
lag (`crates/bess-core/src/kernel/setpoint.rs`), and the converter
converts what the follower has reached. The followed target is a
published point, `blockNN.pcs.setpoint_followed_kw`, and both the block's
setpoint-miss bit and the site's power-limited bit judge against it, so
a plant climbing at the grid code's gradient raises nothing, and a plant
that cannot climb still does.

**Gradient, sourced.** 10 % of the converter's rating per minute, the
active-power gradient VDE-AR-N 4110:2018 asks of a generating plant
after reconnection, the same public figure the M2 return policy's
spacing rests on. A full step from idle to rating therefore takes ten
minutes; `a_step_is_reached_after_the_ramp_without_an_alarm` holds the
site to 98 % between 560 and 620 s and to the gradient tick by tick. The
code states the gradient for reconnection; applying it to every setpoint
change is a choice, documented here, and the conservative one for anyone
testing a dispatcher against the emulator. A plant contracted for faster
services (FCR asks full activation inside 30 s) would carry a different
figure; it is one parameter.

**Dead time and lag, labeled estimates.** The dead time is the kernel's
tick: a setpoint written between ticks acts on the next one, and the
filter starts from what the previous tick left, one second at the M0 tick.
The lag is a first-order 0.5 s, chosen so the lag is visible at the tick
without competing with the ramp; a converter's current controller settles
in tens of milliseconds, so the figure stands in for the controller plus
the plant controller's own cycle rather than for either. No public
datasheet gives a utility converter's closed-loop settling time; the
estimate is labeled and the switch-off (`SetpointDynamics::off`) removes
it along with the ramp.

**What it moved.** Every hourly step of the day-ahead plan became a
ramp, so the plant spends the first minutes of each charge and discharge
hour at partial load. Partial load is cheaper per MWh on every loss row:
the battery row fell 4,952.4 to 4,862.9 MWh, the PCS row 2,763.7 to
2,742.3 MWh, and the HVAC row with the heat, 1,365.1 to 1,335.0 MWh.
Import fell 66,739.3 to 66,655.7 MWh, export rose 55,405.9 to 55,478.3
MWh, and the annual round trip at the POI 0.8302 to 0.8323, inside the
sourced band. The estimator did not move (0.25 points mean, 1.13 worst):
it counts coulombs whatever the shape of the current.

**Known gaps.**

- **One gradient for every change.** Up and down, charge and discharge,
  reconnection and routine dispatch all ramp at the reconnection figure;
  real plants carry a service-dependent gradient.
- **The lag figure** is an estimate, above.
- **No rate limit on the site setpoint itself.** The EMS still steps;
  only the converters ramp. A plant controller that pre-shapes the site
  command would ramp at the site level and share the ramped value.

**Sources:** VDE-AR-N 4110:2018, section on active power gradients after
reconnection (public summary cited at the M2 return policy). Numbers
cited, nothing reproduced.

## M3 (v0.8.0): the warm converter and its overload budget

The converter has a temperature and an allowance. One thermal node per
converter is fed by its conversion loss and cooled toward the ambient
(`crates/bess-models/src/pcs/thermal.rs`); from the node comes a derating
factor, and beside it a bounded short-term overload budget. Both are
published (`blockNN.pcs.temperature_c` through a Pt100 class B
thermometer, `blockNN.pcs.overload_budget_pct` exact) and the block word
gained `pcs_over_temperature` and `pcs_derated`.

**The knee, sourced; the slope, labeled.** The Sungrow SG2500U, the unit
behind the CEC record the surface uses, publishes its operating range as
-30 to 60 °C with derating above 50 °C ambient, and temperature-controlled
forced-air cooling. The curve is shifted onto the node by the rise at
rated loss, so full power holds to 70 °C on the node; the slope beyond,
to zero at 90 °C, is a labeled estimate, since the datasheet gives the
knee and not the curve. The replayed year never reaches it: on the
warmest evening at full power the node sits about 15 K under the knee
(`the_node_settles_under_the_knee_on_the_warmest_evening`), so no
public-weather case can show a derate, and the derate is exercised by a
node set hot by hand (`a_hot_converter_derates_and_says_so`).

**The rise and the time constant, labeled.** 20 K over the ambient at
rated loss and a 15-minute time constant. Forced-air central inverters
hold their heat sinks a few tens of kelvin over the inlet and settle in
the order of ten minutes to half an hour; no public datasheet gives
either figure for this unit. The node steps by implicit Euler, so the
kernel stays rational.

**The overload, sourced; its duration, labeled.** 110 % is the
datasheet's: 2750 kVA at 45 °C against 2500 kVA at 50 °C. Ten minutes at
110 % is the figure utility inverter specifications commonly ask for and
no datasheet of this unit publishes; the thirty-minute refill is an
estimate of the same kind. The budget spends in proportion to the excess
(105 % lasts twenty minutes) and carries an arming latch: a spent budget
disarms the overload and only a full one re-arms it, so the limit does
not flicker at the edge of an empty budget.
`the_budget_spends_and_refills_at_the_published_rates` holds the three
durations to the second.

**Who spends it.** The plant controller, without knowing: its share is
the site target over the dispatchable blocks, so a lost block's share
lands on the others at 105 %, two lost blocks' at 110 %, and a staggered
return runs the early blocks at 110 % while the late ones are still on
their way. `pcs-overload-budget` is the library's case: two converters
trip on the warmest evening, the eighteen left hold the site within a
percent of its target for ten minutes to the second, then every
converter falls to its rating in one tick and the site reports its
limit. `pcs-trip-and-reset` tells the same story at 105 % for twenty
minutes. Advertised capability (`site.available_*_kw`) stays bounded by
the ratings: the overload is a reserve, not capability.

**What it moved.** Nothing in a clean year: the day-ahead plan asks each
converter for its rating and no more, so no budget spends and no node
nears the knee, and the annual record is phase 2's figure for figure
(round trip 0.8323, import 66,655.7 MWh, export 55,478.3 MWh). The two
trip cases of the library changed their stories and their records.

**Known gaps.**

- **The slope, the rise, the time constant, the duration and the
  refill** are labeled estimates, above; the unit's manual would pin
  them.
- **One node.** No junction, capacitor or filter temperatures; the
  interface is the one temperature a converter publishes.
- **No overload policy in the controller.** The share is divided evenly
  and the converters spend their budgets as it falls; a plant controller
  that rations overload across blocks, or holds it back for frequency
  services, is M4's.
- **A block at its rating asked for more reports no setpoint miss.** It
  delivers what it follows and what it follows is bounded by what it
  permits; the site reports the limit. A per-block "asked above rating"
  flag is a candidate for the block word.

**Sources:** Sungrow Power Supply Co., SG3150U/SG2500U datasheet, version
1.22, public (operating temperature range and derating threshold, output
power against ambient, cooling method), retrieved 2026-10-21. Numbers
cited, nothing reproduced.

## M3 (v0.8.0): the substation's switchgear

The HV breaker stopped being a bit that flips. Beside it sit the line
disconnectors and an earthing switch (`SubstationState`), moved by
operator commands that the interlocks a substation enforces
mechanically can refuse (`crates/bess-core/src/kernel/switchgear.rs`):
nothing moves under a closed or closing breaker, the earthing switch
closes only on an isolated bus, and the breaker closes only with the
disconnectors closed, the earthing switch open and no protection trip
latched. Closing the breaker is a sequence, `idle -> closing -> idle`,
that ends with the breaker in and the staggered return started; a site
reset on a tripped site clears the trip and starts it.

**The close, one tick; the travel times, a gap.** IEC 62271-100 breakers
close in tens of milliseconds, under the kernel's one-second tick, so
the close lands on the tick after the command and the sequence shows
for one tick: the finest the plant can say, not a measured figure. The
disconnectors and the earthing switch take seconds to travel on a real
145 kV bay (motor drives); no public datasheet found states the figure,
so they move between ticks and the travel is a known gap rather than a
guessed constant.

**What it moved.** Nothing in a clean year: no breaker opens in the
annual run. The trip case's reset closes the breaker one tick later than
before, with the return's timing unchanged; the new maintenance case
`substation-earthing-work` shows the switchgear in the only order the
interlocks allow. The library is seventeen cases, five of them outside
the failure gate (two calendar, three maintenance).

**Known gaps.**

- **Travel times** of the disconnectors and the earthing switch, above.
- **No shell commands.** The switchgear is driven by scenario files and
  the kernel API; REST and Modbus carry reset, maintenance, HVAC and
  trip as before.
- **No synchronism check** on the close: the grid side is always there
  and in phase until M4 brings a replayed grid.

**Sources:** IEC 62271-100, high-voltage alternating-current circuit
breakers (closing times, public summaries); IEC 62271-102,
alternating-current disconnectors and earthing switches (the interlock
requirements, public summaries). Numbers cited, nothing reproduced.

## M3 (v0.8.0): the transformer's temperatures and the tap changer

The main transformer has two temperatures and a tap. The temperatures
are IEC 60076-7's: a top-oil rise over the ambient driven by the load
factor through the oil exponent, and a hot-spot rise over the top oil as
the difference of two first-order responses driven by the load factor
through the winding exponent, in the standard's difference-equation form
(`crates/bess-models/src/grid/thermal.rs`). The fractional powers come
from a `pow` built of arithmetic and square roots
(`crates/bess-models/src/math.rs`, held to `f64::powf` within parts in
1e12), so the kernel keeps its byte-identity across platforms. The tap
changer holds the MV bus at nominal against the grid's swing and the
transformer's own drop.

**Rated rises, sourced.** 60 K top-oil rise and 78 K hot-spot rise over
the ambient at rated load are IEC 60076-2's temperature-rise limits for
mineral-oil units, public and widely cited (the IEEE/IEC loading-guide
comparison quotes the IEC column as 65 K average winding, 78 K hot
spot, 98 °C absolute at the 20 °C life ambient). The gate
`the_rated_rises_are_the_steady_state` holds the model to both after a
day at rated load. **The constants, labeled.** The ONAF column of the
standard's table of recommended thermal characteristics, as the open
literature reproduces it: oil exponent 0.8, winding exponent 1.3, k11
0.5, k21 2.0, k22 2.0, oil time constant 150 min, winding 7 min. The
table is the standard's text, which is sold; the figures are stated
here as reproduced, not as read from it, and the standard's worked
example (Annex) is not reproduced for the same reason: that gate stays
open. The loss ratio (6) is the M0 constants' (600 kW over 100 kW).

**The limits, sourced.** 120 °C hot spot for normal cyclic loading and
140 °C for long-term emergency loading, IEC 60076-7's limits for power
transformers, public in every loading-guide summary; the warning raises
at the first and clears 10 K under it, the kernel trips the site at the
second (`transformer_trip`, latched, cleared by the site reset), and the
IEEE/IEC comparison's note that operation above 140 °C may cause
gassing is the reason the second is a trip and not a warning.

**The tap changer, labeled.** ±8 steps of 1.25 % (17 positions, ±10 %),
the common range of an HV/MV power transformer's on-load tap changer; a
1.5 % dead band (a little over one step, so the regulator cannot hunt)
and a 60 s delay. No datasheet pinned. The grid voltage it works against
is a ±2 % daily triangle (highest at 04:00, lowest at 16:00), a synthetic
input until M4 replays a measured series, and the transformer's
resistive drop is 0.6 % at rating, the copper losses over the rating.

**What it moved.** The energy did not: the transformer's losses are the
M0 constants' still, so the annual record is phase 3's figure for
figure. The temperatures and the bus are new readings: on the plan's
summer days the hot spot swings by tens of kelvin between the night
charge and the evening discharge and stays under 120 °C; on the
hot-week case it peaks at 98.5 °C with the top oil at 76.6 °C, 21 K
under the normal-cyclic limit, and the tap moves between -1 and +1
holding the bus between 32.5 and 33.5 kV. `site.poi.voltage_kv` swings
with the grid now instead of reading the nominal.

**Known gaps.**

- **The worked example** is not reproduced, above.
- **The constants and the tap changer** are labeled, above.
- **One hot spot.** No bottom oil, no per-winding gradients, no ageing
  rate; the two temperatures are what the transformer's monitor
  publishes.
- **Unity power factor.** The MV bus sees the resistive drop only;
  reactive power and the reactance's drop are M4's.

**Sources:** IEC 60076-2, power transformers, temperature rise for
liquid-immersed transformers (the rise limits, public summaries); IEC
60076-7:2018, loading guide for mineral-oil-immersed power transformers
(the model's structure and the temperature limits, public summaries;
the recommended-characteristics table and the Annex example not
reproduced); IEEE Transformers Committee, "Comparison of Loading Guide
Standards" (IEEE C57.91 against IEC 60076-7), public. Numbers cited,
nothing reproduced.

## Planned gates (from ROADMAP.md)

- **M1:** annual RTE in the 80-85% field band (CAISO/EPRI fleet reports);
  realistic auxiliary share of throughput.
- **M2:** failure type/frequency distribution follows the EPRI incident
  taxonomy.
- **M3:** efficiency surfaces f(P, V_dc) match the Sandia/CEC database (phase
  1 done above); the M1 RTE gate still holds; phases 2 to 4 per
  docs/design/m3/.
- **M4:** simulated annual revenue inside public German fleet index bands.
- **M5:** capacity fade inside published LFP field bands.
