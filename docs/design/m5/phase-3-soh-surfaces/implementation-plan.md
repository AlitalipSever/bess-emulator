# M5 phase 3: health on the surfaces, implementation plan

One PR.

## PR1: the points, the bit, the rows, the scene and the case

- `bess-core`: rack bit 7 in `alarms/layout.rs`; `BmsLogic::rack_alarms`
  (in `bess-models/src/bms/alarms.rs`) evaluates it with the thresholds
  in `RackAlarmThresholds`.
- `bess-emulator`: `map/site.rs` points 100 to 104, `map/block.rs`
  `+ 9`, `MAP_VERSION` 0.7.0, the map tests; the MQTT rack rows, the
  REST site summary, the Prometheus gauges; the every-register test.
- `bess-scene`: the detail table column, the site row.
- `bess-scenario`: the `aged-plant` case in `library/`, its record,
  the taxonomy and short-case counts.
- COMPATIBILITY.md 0.7.0 row; README.md's status sentence.

Accept: the tests above pass; the map digest is updated once and called
out; the annual record does not move; CI green.

## Open questions

- Whether the scene should colour an old rack (a tint on the container
  by its minimum health); leans no in M5, the table is enough and the
  scene's palette is for alarms.
