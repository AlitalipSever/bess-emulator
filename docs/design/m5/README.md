# M5: Degradation

Status: designed 2026-10-22 after M4 shipped as v0.9.0; phases to land
in a stacked chain against `develop`, each phase's documents marked with
what was actually built. ROADMAP.md defines the goal and the calibration
gate; this folder fixes the architecture, the decisions and the work
breakdown, one phase per subfolder. Layout per
[docs/design/README.md](../README.md).

## Goal

Time becomes a simulated quantity: five years in five minutes. After M5
every rack carries a state of health that moves, driven by what the rack
actually went through (its state of charge, its temperature, its
throughput, the depth and rate of its cycles), the cell model delivers
less capacity and more resistance as it does, and the plant's telemetry
shows it: `site.soh_pct` stops reading a constant, every block publishes
its health, the BMS raises a warning when a rack nears the end of its
warranted life. A consumer can start from a plant that is five years old
(a pre-aged preset), or run a year at full fidelity and project it over a
decade in seconds from the stress the year recorded, and the trajectory
that comes out is held against published LFP field and laboratory bands
before it is claimed.

## What M5 changes

Following the pattern since M2: the module that deepens is the cell
model, behind `CellModel`, with one neighbour widened because aging has
no clock to run on without it: the kernel's notion of time (a run can
now skip days through a stress ledger instead of ticking them).

1. **The racks age.** A semi-empirical calendar and cycle aging model for
   large-format LFP cells, evaluated once per simulated day per rack from
   a stress ledger the tick fills (mean state of charge, mean cell
   temperature, throughput, the day's depth of discharge, its mean rate).
   Capacity fade and resistance growth are two states with power-law
   trajectories; the cell model reads them (less capacity per Ah, more
   ohms per rack), so the whole plant feels them: fewer MWh a day, more
   heat per MWh, earlier BMS tapers.
2. **Time compression.** A simulated year leaves a stress year behind (one
   ledger row per rack per day). The kernel can fast-forward through that
   year again, as many times as asked, running only the aging model: ten
   projected years in seconds. The bench runs the measured year and
   projects it; the shell can start from a plant aged N years at
   commissioning (a bundled typical stress day, labeled); a scenario can
   ask for the same.
3. **Health on every surface.** Modbus points for the site's and each
   block's health and age, rack rows on MQTT and REST, Prometheus gauges,
   a rack warning bit, the scene's detail table, a library case that plays
   the hot week on a five-year-old plant.
4. **A gate on the trajectory.** The projected trajectory of the clean
   year's plant is held against a band derived from a public multi-year
   field study of LFP home storage and a public laboratory study of LFP
   cycle life, both written down before the number is.

## Phases

| Phase | Theme | Depends on |
|---|---|---|
| [1: The aging model](phase-1-aging-model/) | `AgingModel` trait, the stress ledger, calendar and cycle states, the cell model reading them, checkpoint 20, the `aging` section of the annual record | nothing |
| [2: Time compression](phase-2-time-compression/) | the stress year, `fast_forward`, the bench's `--years`, pre-aged presets on the shell and in scenario files, `calibration/m5-aging.json` | 1 |
| [3: Health on the surfaces](phase-3-soh-surfaces/) | Modbus points and the rack bit, MQTT, REST, Prometheus, the scene, the `aged-plant` case | 1 (the state), 2 (the preset the case starts from) |
| [4: Calibration and release](phase-4-calibration-release/) | the trajectory gate in CI, CALIBRATION.md, release v0.10.0 | 2, 3 |

Phases land in order. A phase is done when its PRs are merged, its tests
are green in CI, and its documents record what was actually built.

## Calibration gate

From ROADMAP.md: capacity fade trajectories inside published LFP field
study bands. Made concrete, against public data, recorded in
CALIBRATION.md before the number:

1. **The field band.** Figgener et al., "Multi-year field measurements of
   home storage systems and their use in capacity estimation", Nature
   Energy 9 (2024) 1438 to 1447, measured 21 German home storage systems
   over up to eight years. The LFP systems lost 2.2 percentage points of
   usable capacity a year by the field capacity tests (2.0 by the
   estimator), 0.7 percentage points per 100 equivalent full cycles, at
   around 200 cycles a year; the spread between similar systems was about
   2 percentage points a year. GW-01's clean year runs about 280 cycles,
   so the cycle share scales by 1.4: the band is 1.5 to 3.5 percentage
   points a year, and the gate is the mean rack state of health after
   five projected years of the clean year's stress, between 0.82 and 0.93.
   A home storage system is not a 5 MW container plant (its cells are
   smaller, its temperatures a living room's), which is why the band is
   this wide and why it is named a field band and not a fleet record.
2. **The laboratory band.** Preger et al., "Degradation of Commercial
   Lithium-Ion Cells as a Function of Chemistry and Cycling Conditions",
   J. Electrochem. Soc. 167 (2020) 120532 (open access): LFP cells cycled
   at 15 to 35 °C, 0.5C to 3C, three depth-of-discharge windows reached
   80 % capacity between 2,500 and 9,000 equivalent full cycles. The gate:
   a rack cycled at two full cycles a day in a 25 °C container reaches
   80 % inside that range.
3. **The datasheet claim**, cited and not gated: the EVE MB31 314 Ah cell
   is specified at 8,000 cycles to 70 % at 25 °C and 0.5P. The model is
   expected to sit below it (a datasheet counts cycles in a two-year
   test, the model adds the calendar of the years those cycles take).
4. **The M1 round-trip gate still holds** for the aging year: the clean
   year's efficiency moves only through the resistance growth the model
   books, and stays inside the band.

## Versioning

- **Crate:** v0.10.0 at milestone end.
- **Signal map:** additions (block health at the extended range, site
   age and minimum health at unused addresses) and one changed value on a
   point that has read a constant since M0 (`site.soh_pct`). No address
   moves. Map 0.7.0.
- **Checkpoint:** format 20 in phase 1 (the rack's aging states and its
   ledger), widened in phase 2 (the stress year's capture state).

## Data and licences

Recorded in DATA-LICENSES.md before each lands:

| Source | Terms | M5 handling |
|---|---|---|
| NREL BLAST-Lite, `Lfp_Gr_250AhPrismatic` (Gasper, NREL; data in Gasper et al., J. Energy Storage 2023, doi 10.1016/j.est.2023.109042): a capacity model for a large-format prismatic LFP cell above 250 Ah, fitted on calendar aging over temperature and state of charge and cycle aging over depth, mean state of charge and rate | BSD 3-Clause (Alliance for Energy Innovation, 2023) | the ten parameters and the two power-law trajectories are carried as constants in `bess-models` with attribution; the code is not vendored |
| SimSES (TUM), `SonyLFPCalendarDegradationModel` and `SonyLFPCyclicDegradationModel`, carrying Naumann et al.'s resistance laws (J. Energy Storage 17 (2018) 153; J. Power Sources 451 (2020) 227666) | BSD 3-Clause (TUM, archived) | the resistance-growth constants (a calendar Arrhenius term in SoC, a cycle term in depth and rate) as constants with attribution, labeled as a different cell's law |
| Naumann et al. calendar and cycle aging datasets (Mendeley Data) | CC BY 4.0 | not needed; named as the resistance laws' provenance |
| Figgener et al. 2024 field study and its dataset (Zenodo 12091223, 146 GB) | paper CC BY-NC-ND 4.0; dataset CC BY 4.0 | figures cited for the band; nothing bundled |
| Preger et al. 2020 and the Sandia cycling files (batteryarchive.org) | paper CC BY-NC-ND 4.0; archive terms not stated | figures cited for the laboratory band; nothing bundled |
| EVE MB31 datasheet | manufacturer's document | the cycle-life claim cited |

## Release-note inventory for v0.10.0

Filled as PRs land.

- **The racks age** (phase 1 PR1). Every rack carries two capacity-loss
  states and two resistance-growth states, advanced once per simulated
  day from a stress ledger the tick fills (cycles, mean state of charge
  and cell temperature, the day's depth and rate), by NREL BLAST-Lite's
  large-format prismatic LFP capacity model and Naumann's resistance
  laws (both BSD 3-Clause, the constants carried with attribution, the
  resistance law labeled). The cell model delivers less capacity and
  more resistance as they move; `site.soh_pct` (7) reads the racks. Each
  rack's rate is its own (±10 %, labeled); the stored energy the fade
  writes off is an `aging` row of the loss waterfall so the balance
  closes. The annual record gains an `aging` section. Checkpoint 20.
- **A run can skip days** (phase 2 PR1). A ticked year leaves a stress
  year behind (one closed day per rack per day), and
  `Simulation::fast_forward` replays it through the aging model alone:
  the racks age, the clock moves, nothing is metered, one `fast_forward`
  event records it. `Simulation::new_aged` builds a plant aged N years
  at commissioning from a typical day (labeled); the shell's
  `--age-years`, a scenario file's `age_years` (0 to 20), the headless
  record's `age_years`. The bench's `--years N` projects the measured
  year over a decade in a second and publishes
  `calibration/m5-aging.json` (per year the mean, worst and best health,
  the cycles, the resistance growth; two laboratory trajectories to the
  80 % crossing and 8,000 cycles); `--resimulate` ticks the physics
  instead. Checkpoint 20 widened (`skipped_days`).
- **Health on every surface** (phase 3 PR1). `site.age_days` (100),
  `site.soh_min_pct` (101), `site.efc` (102, 103),
  `site.resistance_growth_pct` (104) and `blockNN.soh_pct` (ext + 9), all
  exact, map 0.7.0; rack bit 7 `soh_low` below 80 % (labeled
  hysteresis); the REST summary's `age_days`, `soh_mean`, `soh_min` and
  per block `soh`; Prometheus `bess_rack_soh`, `bess_site_age_days`,
  `bess_rack_efc_mean`; the scene's rack table and substation row; the
  case `aged-plant` (three hot days on a five-year-old plant) with its
  lesson held against the fresh plant.

## Open questions

- Whether the daily aging step should fall on the run's day boundary
  (every 86,400 s from tick 0) or on local midnight; leans the run
  boundary, which the fiscal meter and the KPI report do not use, so a
  day's stress is one ledger row whatever the start time.
- Whether the per-rack spread of aging rate (a scalar drawn at
  commissioning) is ±10 % or taken from the field study's spread between
  identical systems (about 2 percentage points a year on a 2 mean, so
  far wider); leans ±10 % labeled, since the field spread includes
  operation, which the ledger already carries.
- Whether the projection re-simulates the physics year at intervals (the
  aged racks change the year that ages them) or repeats the measured
  stress year unchanged; leans unchanged for the gate (the way the NREL
  model is used) with a re-simulation flag for anyone who wants the
  feedback, outside CI.
- Whether the BMS reports a state of health of its own (an estimator with
  error, as the reported SoC is) or the truth; leans the truth in M5 with
  the estimator named as a later deepening.
