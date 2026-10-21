# M5 phase 3: health on the surfaces, architecture

What exists after this phase: the plant's age and health on every
surface a consumer reads. Modbus carries the site's age, mean and
minimum health and cumulative cycles, and each block's health; the rack
word has a warning for a rack past its warranty level; MQTT and REST
carry the rack rows; Prometheus the gauges; the scene's detail table the
rack's health and age; and the library plays the hot week on a
five-year-old plant.

## The model

- **Modbus.** `map/site.rs`: `site.age_days` (100, u16, whole days of
  the oldest rack), `site.soh_min_pct` (101, tenths), `site.efc` (102 and
  103, u32, the mean rack's equivalent full cycles since commissioning,
  tenths), `site.resistance_growth_pct` (104, tenths, the mean rack's
  growth). `map/block.rs`: `blockNN.soh_pct` at the extended range's
  `+ 9` (tenths, the block's in-service racks' mean, Slow class). Map
  0.7.0.
- **The rack word.** `alarms/layout.rs`: rack bit 7 `soh_low`, a warning
  with hysteresis, raised below 80 % and cleared above 81 % (labeled;
  D1), evaluated by `BmsLogic::rack_alarms` from `rack.soh`.
- **MQTT and REST.** The rack rows under `racks/<path>` gain `soh`,
  `age_days` and `efc`; `/api/v1/state` already serializes `RackState`,
  so `age` appears with it; `/api/v1/site` gains `age_days`, `soh_mean`,
  `soh_min`.
- **Prometheus.** `bess_rack_soh` (min and mean over racks),
  `bess_site_age_days`, `bess_rack_efc_mean`.
- **The scene.** `panels/detail.rs`: the container's rack table gains a
  SoH column; the transformer's site row gains age and minimum health.
- **The library.** `aged-plant`: the hot week on a plant aged five years
  (`run.age_years: 5`), taxonomy `cell`, with the derate arriving
  earlier and the day's exported energy lower than the fresh plant's on
  the same week; the lesson names both figures against the fresh case.

## Invariants

- No address moves; the map digest at 0.7.0 is recorded in the contract
  test and the previous digest is kept as the one it grew from.
- A fresh plant reads 100.0 on every health point, zero age, zero
  cycles, no `soh_low` anywhere: the surfaces of v0.9.0 are a special
  case of these.
