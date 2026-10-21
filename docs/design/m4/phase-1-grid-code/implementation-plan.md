# M4 phase 1: grid code at the POI, implementation plan

Two PRs, each landing green.

## PR1: the frequency and the active-power responses

- `bess-models/src/grid_signal.rs` with `SyntheticFrequency` and the
  hour-boundary pattern; `bess-data/src/frequency.rs` with the replay
  artifact, `fetch-frequency.sh`, `compile-frequency`; `--frequency` on
  the shell and the bench; `kernel/gridcode.rs` with `GridCode` and
  `FcrDroop`; the fast term through the block step; points 60 and 61,
  bits 4 and 10; checkpoint 17 (partial: droop state); the
  `frequency-event` case; CALIBRATION.md section with the paper and the
  prequalification annex named; DATA-LICENSES.md rows for the
  frequency sources (none bundled).

Accept: the FCR shape test passes; LFSM cuts the target at the standard's
gradient; the annual record moves only through the frequency responses
and stays inside the M1 band; the switch-off proof holds.

*As built (PR #48):* as the design's as-built notes say. The switch-off
proof is `inside_the_band_the_grid_code_block_is_inert`: characteristics
pushed out of reach leave the same tree as the defaults over six hours
on a flat grid, and the synthetic series never reaches a threshold. The
kernel's tick grew past clippy's line bound and the auxiliary step moved
to `step_aux`.

## PR2: reactive power

- `PcsModel::reactive`, `ReactiveMode`, `set_reactive_mode`, the Q(U)
  filter, the transformer's reactive consumption, `site.poi.power_factor`,
  `blockNN.pcs.q_kvar`, `site.grid_code.state`; checkpoint 17 complete;
  CALIBRATION.md section with the circle, the 4110 range and the
  labeled impedance.

Accept: the circle holds in every mode; Q at zero active power reaches
the rating; the annual energy record is unchanged by reactive power
(no energy row moves).

*As built (PR #49):* as the design's as-built notes say. The mode is set
through `Simulation::set_reactive_mode` until phase 2 puts it on the
holding bank; the shell publishes the vars and the power factor and
nothing else changed on its surfaces. Records regenerated a second time.

## Open questions

- Whether the droop's fast term should also bypass the M3 dead time
  (one tick): leans no, the tick is the kernel's resolution.
- Whether the Q(U) slope should be the DSO's usual ±10 % band instead of
  ±5 %: leans ±5 % with the number labeled; a config value either way.
