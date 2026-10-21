# v0.7.0: M2.5, three kinds of truth leave the surfaces

Draft of the release note, edited from the inventory in [README.md](README.md).
The maintainer creates the tag with it:
`gh release create v0.7.0 --title "v0.7.0: M2.5, three kinds of truth leave the surfaces" --notes-file docs/design/m2_5/release-notes.md`
(drop this preamble first).

---

Until v0.6.1 the plant published three things no real plant can: cell
voltages that sat on one curve, a state of charge that was exact by
construction, and readings without an instrument behind them. v0.7.0
takes all three off the published surfaces. Anyone testing an estimator,
a reconciliation job or a data-quality rule against the emulator now
faces the signals the field would give them.

## Highlights

- **Rested voltage depends on history.** The LFP cell carries Plett's
  one-state OCV hysteresis: after charging a rested rack reads above the
  mean curve, after discharging below it, up to 19 mV per cell at the
  low knee and about 10 mV on the plateau, held at rest. The envelope's
  peak is pinned to a published characterization (Barai et al. 2015);
  the plateau level is a labeled estimate. Voltage points keep their
  addresses and units; a baseline recorded against v0.6.1 voltages will
  differ, and inferring SoC from a rested voltage now gives a
  path-dependent answer, which is the point.
- **The published SoC is an estimate.** Each rack's BMS counts coulombs
  through a current sensor with an offset and a gain error inside a
  published class (75 mA, 0.4 % of reading), walks away from the truth
  on the plateau and corrects itself by jumps at the knees of the OCV
  curve. `site.soc_pct`, `blockNN.soc_pct`, their MQTT mirrors, the REST
  summary's `soc` and `bess_site_soc_ratio` carry that estimate now:
  same addresses, units and scales, new meaning, **signal map 0.4.0**.
  On the replayed year it sits 0.25 points from the truth on average and
  up to 1.1 on a rack. Each correction is an event, `soc_resnap`, on
  every surface. The truth stays on the research surfaces: the annual
  record's estimator section and the study series.
- **Every published value passes through an instrument.** An explicit
  inventory: the fiscal meter (IEC 62053-22 class 0.2S), the house-load
  meters (0.5S), the substation's CT/VT chain and each PCS's own power
  measurement (class 0.5), Pt100 class B thermometers. Bounded noise, a
  slow wander and a limit that grows at low load, always on, inside the
  class by construction, derived from the seed and the tick so a resume
  publishes the same bytes. The signal map CSV gains `instrument` and
  `accuracy_class` columns; counters, states, the BMS's own estimates,
  frequency and irradiance are listed exact. `--exact-instruments`
  publishes the kernel's values for research.

## What an integrator can trip over

- Signal map 0.4.0 (breaking by meaning, no address moved): the SoC
  points above. COMPATIBILITY.md has the row.
- Checkpoint format 10: racks carry `hysteresis_v`, `soc_reported`,
  `sensor_offset_a` and `sensor_gain`; v8 and v9 files are refused by
  version.
- A new event, `soc_resnap`, about once per rack and day on the
  replayed year, on MQTT under `events/<rack>/soc_resnap`, in the event
  counter and in the viewer's journal.
- Two readings of one quantity no longer agree to the digit; a pipeline
  that reconciled them exactly will see a bounded, wandering difference.
- The 0.5C cycle gate moved 0.917 to 0.913 and the annual round trip
  0.8366 to 0.8296, the hysteresis work booked as battery heat; both
  inside their bands. All fifteen library records were regenerated.

## Calibration

Three gates, recorded in CALIBRATION.md: the modeled charge/discharge gap
against the pinned curve (35.9 mV at 5 % SoC, 19.9 at 50 %); the
estimator's mean and worst error against sanity bounds whose ceiling is
the field range a fleet report gives (0.253 and 1.13 points); every
instrumented reading inside its class at every load. The known gaps are
written beside each.
