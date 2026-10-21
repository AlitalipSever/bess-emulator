# v0.8.0: M3, the electrical path becomes a character

Draft of the release note, edited from the inventory in [README.md](README.md).
The maintainer creates the tag with it:
`gh release create v0.8.0 --title "v0.8.0: M3, the electrical path becomes a character" --notes-file docs/design/m3/release-notes.md`
(drop this preamble first).

---

Until v0.7.0 the converter was a curve and the substation a bit: power
converted at one efficiency whatever the racks' voltage, a setpoint was
reached the tick it was written, a reset dropped twenty converters
straight into the connected set, and the HV breaker flipped. v0.8.0
gives the electrical path its own physics and its own order of events.
Anyone testing a dispatcher, a ramp-rate check, an availability
calculation or a switching procedure against the emulator now faces a
plant that takes time, gets warm and refuses out-of-order commands.

## Highlights

- **The converter converts on a surface and follows a setpoint.**
  Conversion loss follows the DC bus voltage the racks present, through
  the CEC record's own voltage terms, and rack limits are current-bounded
  at that voltage, so capability falls toward the bottom of the SoC
  window. Each block's share passes a dead time, a ramp of 10 % of the
  converter's rating per minute (VDE-AR-N 4110) and a 0.5 s lag: a site
  step over Modbus is reached about ten minutes later, climbing at the
  gradient, with neither `setpoint_not_met` nor `power_limited` raised on
  the way. New points `blockNN.pcs.dc_voltage_v` and
  `blockNN.pcs.setpoint_followed_kw`.
- **The converter has states, a temperature and an overload budget.**
  `blockNN.pcs.state` carries SunSpec's operating-state numbering
  (**breaking**, map 0.5.0: 1 off, 3 starting, 4 running, 7 fault, 8
  standby). A reset leaves a converter off with its contactors open; a
  cleared block walks precharge, contactor close and synchronization
  (16 s) before it ramps, so a staggered return takes 30 s per block plus
  16 s. One thermal node per converter (SG2500U: full power to 50 °C
  ambient) with `pcs_over_temperature` and `pcs_derated` bits, and a
  short-term overload of 110 % (the datasheet's 2750 kVA) for ten
  minutes: a lost block's share lands on the others at 105 % for twenty
  minutes, two lost blocks' at 110 % for ten, before the site reports its
  limit. New points `blockNN.pcs.temperature_c` and
  `blockNN.pcs.overload_budget_pct`.
- **The substation has switchgear, a warm transformer and a tap
  changer.** Line disconnectors and an earthing switch beside the HV
  breaker, with the interlocks a substation enforces; closing the breaker
  is a sequence, and a site reset on a tripped site starts it. The main
  transformer carries IEC 60076-7's top-oil and hot-spot temperatures
  (rated rises from IEC 60076-2), with a warning at 120 °C and a kernel
  trip at 140 °C. An on-load tap changer holds the MV bus at nominal
  against a ±2 % daily swing of the grid voltage, a new input, and
  `site.poi.voltage_kv` swings with it instead of reading a constant.
  New points `site.substation.sequence`, `disconnectors_closed`,
  `earthing_closed`, `site.transformer.top_oil_c`, `hot_spot_c`,
  `tap_position`, `site.mv_bus.voltage_kv`.

## What an integrator can trip over

- Signal map 0.5.0 (breaking): `blockNN.pcs.state` (base+2) is
  SunSpec-numbered; everything else is an addition at an unused address
  or a value change on a point that kept its meaning. COMPATIBILITY.md
  has the row.
- Checkpoint format 16: converters carry their startup timer, the
  follower's two values, their temperature, budget and permitted power;
  the substation its positions, sequence, two temperatures, tap and MV
  bus. v0.7.0's format 10 files are refused by version.
- Timing changed on every surface: a setpoint step is a ten-minute ramp,
  a reset closes the breaker on the next tick, a returned block runs
  16 s after its release. Baselines recorded against v0.7.0 differ on
  every hour boundary of the plan and after every trip.
- A converter asked for more than its rating delivers its rating and
  reports no setpoint miss; the site reports the limit.
- Two new events, `breaker_sequence` and `switch`, on MQTT under
  `events/site/substation/…`, in the stream and the viewer's journal.
- The grid voltage is a new kernel input (`Inputs::grid_voltage_pu`);
  code that builds `Inputs` by hand gains a field.
- Scenario files gain six switchgear kinds on `site.substation`; two
  library cases are new (`pcs-overload-budget`, `substation-earthing-work`)
  and two tell new stories (`pcs-trip-and-reset`,
  `protection-trip-staggered-return`). Seventeen cases, all records
  regenerated.
- The annual round trip moved 0.8296 to 0.8323: the surface converts a
  little better below its nominal voltage, and every hourly step of the
  plan is a ramp now, so the plant spends more of the year at partial
  load, which is cheaper on every loss row. Inside its band.

## Calibration

Five M3 gates, recorded in CALIBRATION.md: the efficiency surface against
the CEC database record; the setpoint gradient against the grid code; the
converter's knee and overload against the SG2500U datasheet, with the
rise, time constant, slope, duration and refill labeled estimates; the
transformer's rated rises against IEC 60076-2 and its limits against IEC
60076-7, with the standard's constants labeled and its worked example
named as an open gate; the M1 round-trip gate still holding with every
new loss in place. The known gaps are written beside each, among them
the switchgear's travel times, the one-gradient-for-everything ramp, and
the absence of a shell command surface for the switchgear.
