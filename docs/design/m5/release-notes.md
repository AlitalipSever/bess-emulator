# v0.10.0: M5, time becomes a simulated quantity

Draft of the release note, edited from the inventory in [README.md](README.md).
The maintainer creates the tag with it:
`gh release create v0.10.0 --title "v0.10.0: M5, time becomes a simulated quantity" --notes-file docs/design/m5/release-notes.md`
(drop this preamble first).

---

Until v0.9.0 every rack was as new on the last day of a year as on the
first, and `site.soh_pct` had read 100 since M0. v0.10.0 lets the racks
age from the stress they actually saw, lets a run skip years through
its own stress record, lets a plant start five years old, and puts
health and age on every surface, with the projected decade held to a
public field band before any number was claimed.

## Highlights

- **The racks age.** Every rack carries two capacity-loss states and
  two resistance-growth states, advanced once per simulated day from a
  stress ledger the tick fills: the day's cycles, its mean state of
  charge and cell temperature, its depth and its rate. The capacity
  model is NREL BLAST-Lite's for a large-format prismatic LFP cell above
  250 Ah (Gasper 2023, BSD 3-Clause), the closest public analogue to a
  314 Ah rack; the resistance laws are Naumann's as SimSES carries them
  (BSD 3-Clause), labeled as a different cell's. Both are integrated at
  the trajectory's current slope, so a warm week adds what a warm week
  adds to a cell this old. The cell model delivers less capacity and
  more resistance as the states move: the clean year ends at a mean
  state of health of 0.967 (1.0 % calendar, 2.3 % cycling, resistance up
  2 %), exports 1.8 % less than the plant that never aged, and its
  round trip falls a tenth of a point to 0.8313, still inside the M1
  band. Each rack ages at its own rate (±10 %, labeled); the stored
  energy a capacity loss writes off is a row of the loss waterfall, so
  the balance keeps closing. No data series is bundled: the constants
  of two published models, with attribution.
- **A run can skip days.** A ticked year leaves a stress year behind,
  one closed day per rack per day, and the kernel can fast-forward
  through it as many times as asked, running only the aging model: the
  racks age, the clock moves, nothing is metered, one `fast_forward`
  event records it. The bench's `--years 10` projects the measured year
  over a decade in a second and publishes `calibration/m5-aging.json`
  (0.967 after one year, 0.891 after five, 0.815 after ten, the worst
  rack crossing 80 % in year ten); `--resimulate` ticks the physics
  instead. A plant aged N years at commissioning: the shell's
  `--age-years`, a scenario file's `age_years` (0 to 20), built from a
  typical day (the clean year's ledger means, labeled) with the racks'
  own rates, so the spread is there from the first tick.
- **Health on every surface.** `site.soh_pct` (7) reads the racks;
  `site.age_days` (100), `site.soh_min_pct` (101), `site.efc` (102, 103),
  `site.resistance_growth_pct` (104) and `blockNN.soh_pct` (2000 + 10b +
  9) are new, all exact; map 0.7.0, no address moves. Rack word bit 7
  `soh_low` raises below 80 %, the level laboratory cycle-life figures
  and warranties are stated at, and clears above 81 %. The REST summary
  carries `age_days`, `soh_mean`, `soh_min` and per block `soh`;
  `/api/v1/state` every rack's `age`; Prometheus `bess_rack_soh`,
  `bess_site_age_days`, `bess_rack_efc_mean`; the viewer's rack table a
  SoH column and the substation row the plant's age and health. The
  library gains `aged-plant`: three hot days on a five-year-old plant,
  nothing failing, exporting well under the fresh plant on the same
  days; twenty-two cases.
- **The BMS is told its racks' health.** On the aged plant an estimator
  kept counting on the nameplate ran away by the lost tenth every cycle
  and resnapped 11,312 times in three days where the fresh plant
  resnapped 667. No real BMS does that; the estimator now counts
  against the capacity the rack has, and an estimator that learns it
  with the error that carries is named as the next deepening.
- **The trajectory is gated.** Two bands were written before the number
  and are held by CI on every pull request: the year-five mean state of
  health between 0.82 and 0.93, derived from Figgener et al. 2024 (21
  German home storage systems over up to eight years, the LFP ones
  losing 2.2 points a year, scaled to the plant's cycles and named a
  field band because a home system is not a container plant), and
  2,500 to 9,000 cycles to 80 % for a rack at two cycles a day at 25 °C,
  Preger et al. 2020's range across LFP cells. Measured 0.891 and 3,214.
  The EVE MB31 datasheet's 8,000 cycles to 70 % is reported beside the
  model's 0.587 at 8,000 cycles, not gated; the model's cycle term
  falls with temperature, its authors' stated artifact, and
  CALIBRATION.md says so rather than correcting it.

## Compatibility

- **Signal map 0.7.0.** Additions at unused addresses and one changed
  value on a point that has read a constant since M0 (`site.soh_pct`).
  Rack word bit 7 `soh_low`. A new event `fast_forward`. The shell gains
  `--age-years`; scenario files gain `age_years`; headless records of a
  pre-aged plant carry `age_years`. COMPATIBILITY.md has the row.
- **Checkpoint format 20.** Every rack's aging states, its rate, its
  open stress ledger, its last closed day and the days it was skipped;
  the energy books' written-off row. v0.9.0 files are rejected with
  their version named.
- **Code that builds `RunParams` by hand** gains `age_years`; code that
  builds `Models` by hand gains `aging` (`LfpAging::gw01()` or
  `NoAging`); code that builds `RackState` by hand gains `age`.
- **Records regenerated:** the golden digest, the twenty-two library
  records, both annual records and the trajectory, for the physics and
  for the version bump.

## Known gaps, carried into M5.x

- The projection repeats the measured stress year; the feedback of an
  aged rack on its own later years (deeper cycles for the same energy)
  is not in the projected figures. `--resimulate` measures it; the gap
  is not yet recorded.
- The NREL model's cycle term falls with temperature inside its fitted
  range, so a warmer container ages slower under heavy cycling in this
  model; the calendar term rises with temperature as it should.
- The resistance law is a 3 Ah cylinder's, labeled.
- The BMS knows the true state of health; an estimator with error is
  the next deepening of the reported SoC.
- No public multi-year capacity record of a utility-scale LFP plant
  with a licence and a number was found; the field band is derived from
  home storage and says so.
