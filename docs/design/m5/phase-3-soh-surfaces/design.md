# M5 phase 3: health on the surfaces, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the warning is at 80 %, with 1 % of hysteresis.** 80 % is the
  level laboratory cycle-life figures are stated at (Preger 2020) and
  the level home storage warranties are commonly written at (Figgener
  2024 discusses the aging reserve datasheets carry to meet it); the
  datasheet's own end of life (70 % for the EVE MB31) is a status the
  points show, not a bit, since a plant at 70 % is still a plant. One
  rack below the level raises its own bit; the site's partial
  availability bit is not touched (an old rack is in service).
- **D2, tenths of a percent for health, days for age.** A state of
  health moves by hundredths of a percent a day; a u16 in tenths resolves
  a week of aging, which is the cadence a SCADA historian would read it
  at, and a whole percent (the M0 `site.soh_pct` scale, kept) is what an
  operator sees. Age in days fits a u16 for 179 years.
- **D3, the block's health is its in-service racks' mean**, the same
  rule as the block's reported state of charge; an isolated rack's
  health is in its own row and in the site's minimum.
- **D4, the case is a hot week on an old plant.** It is the one library
  case that starts from a preset, which is why it lands in this phase
  and not in the scenario crate's own set: the case exists to show the
  surfaces a consumer compares (the fresh hot-week case against this
  one), and its lesson is the two figures side by side.

## Signal map delta

| Point | Address | Encoding | Class |
|---|---|---|---|
| `site.age_days` | 100 | u16, days | Slow, exact |
| `site.soh_min_pct` | 101 | u16, tenths | Slow, exact |
| `site.efc` | 102, 103 | u32, tenths | Slow, exact |
| `site.resistance_growth_pct` | 104 | u16, tenths | Slow, exact |
| `blockNN.soh_pct` | 2000 + 10 b + 9 | u16, tenths | Slow, exact |

Rack word bit 7 `soh_low` (warning). Map 0.7.0; COMPATIBILITY.md row.

## Checkpoint impact

None (the state landed in phases 1 and 2).

## Test plan

- Map contract tests: the five points, the digest, no address moved.
- An aged preset reads its figures end to end over Modbus (the
  every-point test gains the new addresses).
- The rack bit raises on a rack pushed below 80 % by a long projection
  and clears above 81 %; the alarm names test covers `rack.soh_low`.
- MQTT and REST snapshot tests for the rack rows; the Prometheus
  exposition test for the gauges.
- The scene's detail table test for the column.
- The library: `aged-plant` has a record, a headline under 600
  characters, and its lesson's two figures hold in `library_lessons.rs`;
  the short-case count in the scene test moves.
