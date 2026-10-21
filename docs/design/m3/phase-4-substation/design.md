# M3 phase 4: substation depth, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, IEC 60076-7 exponents and typical constants.** The standard's
  thermal model is public in its structure and its typical values are
  widely reproduced (top-oil exponent, winding exponent, time constants
  for ONAN/ONAF); the plant's main transformer takes the ONAF column.
  Rated losses from the M0 constants (`no_load_loss_w`,
  `load_loss_at_rated_w`), so the waterfall's transformer row is
  unchanged in energy and gains a temperature. *As built (PR2):*
  `TransformerThermal` in `bess-models/src/grid/thermal.rs`, owned by
  `SimpleGrid`, the standard's difference-equation form (top-oil rise
  through the oil exponent, hot-spot gradient as the difference of two
  first-order responses through the winding exponent), implicit Euler.
  The fractional powers come from `bess-models/src/math.rs`, a `pow`
  built from arithmetic and square roots so the kernel stays
  byte-identical across platforms (the hysteresis lesson). Rated rises
  are IEC 60076-2's limits (60 K top oil, 78 K hot spot); the ONAF
  constants (x 0.8, y 1.3, k11 0.5, k21 2.0, k22 2.0, 150 and 7 min)
  are the open literature's reproduction of the standard's table,
  labeled as such, the table itself not reproduced. The waterfall's
  transformer row did not move.
- **D2, the sequence's durations** from public switchgear data (breaker
  close and disconnector travel times), labeled estimates where only
  ranges are public. *As built (PR1):* the breaker's close is one tick:
  IEC 62271-100 breakers close in tens of milliseconds, and the
  kernel's one-second tick is the finest it can say, so the close lands
  on the tick after the command. The disconnectors and the earthing
  switch move between ticks; their seconds of travel (no public figure
  found for the 145 kV class) are a known gap. The sequence is
  `Idle -> Closing -> Idle` (`kernel/switchgear.rs`), the commands are
  `open_hv_breaker`, `close_hv_breaker`, `set_disconnectors` and
  `set_earthing_switch`, and the interlocks are
  `CommandError::Interlocked` with the rule named: nothing moves under a
  closed or closing breaker, the earthing switch closes only on an
  isolated bus, the breaker closes only with the disconnectors closed,
  the earthing switch open and no trip latched. The site reset is still
  "close": it clears the trip and starts the sequence, or leaves the
  breaker open when an interlock holds.
- **D3, the OLTC** with a public transformer datasheet's range and
  step (the common ±9 steps of 1.25 % or ±8 of 1.78 %), a regulator
  dead band of one step and a delay of tens of seconds; the pinned
  sheet decides. *As built (PR2):* `TapChanger`, ±8 steps of 1.25 %, a
  1.5 % dead band and a 60 s delay, labeled estimates (no sheet pinned;
  the range and step are the common HV/MV figures). The regulator holds
  the MV bus at nominal against the grid's swing and the transformer's
  resistive drop (0.6 % at rating, the copper losses over the rating).
  The grid voltage is a new input, `Inputs::grid_voltage_pu`, a ±2 %
  daily triangle (highest 04:00, lowest 16:00) in both drivers,
  synthetic until M4; `site.poi.voltage_kv` carries it now instead of a
  constant.
- **D4, checkpoint bump.** Positions, sequence state and timer, two
  transformer temperatures, tap position and regulator timer. *As built
  (PR1):* format 15 with the positions, the sequence and its timer; the
  transformer and the tap are PR2's: format 16 adds the top oil, the
  hot spot's two responses, the tap position, its timer and the MV bus
  voltage.
- **D5, points and bits.** Site range: `site.substation.sequence`
  (enum), `site.substation.disconnectors_closed`, `earthing_closed`,
  `site.transformer.top_oil_c`, `site.transformer.hot_spot_c`
  (thermometer instrument), `site.transformer.tap_position`,
  `site.mv_bus.voltage_kv` (analog); site alarm bits
  `transformer_over_temperature` (warning) and `transformer_trip`. *As
  built (PR1):* `site.substation.sequence` (52), `disconnectors_closed`
  (53) and `earthing_closed` (54), exact. Two events: `breaker_sequence`
  (from, to) and `switch` (which, closed), on MQTT under
  `events/site/substation/…`, in the stream and the viewer's journal.
  Scenario files address `site.substation` with `breaker_open`,
  `breaker_close`, `disconnectors_open`, `disconnectors_close`,
  `earthing_open` and `earthing_close`. *As built (PR2):*
  `site.transformer.top_oil_c` (55) and `hot_spot_c` (56) through Pt100
  class B thermometers, `site.transformer.tap_position` (57, signed,
  exact), `site.mv_bus.voltage_kv` (58, class 0.5 against the 33 kV
  nominal); site bits `transformer_over_temperature` (3, warning, 120 °C
  raise and 110 clear, the normal-cyclic limit) and `transformer_trip`
  (9, trip byte, latched: the kernel trips the site from inside the tick
  at 140 °C, the long-term emergency limit, and the site reset clears it
  like a protection trip).

## Signal map impact

Seven additions at unused site addresses, two site alarm bits (map
0.5.0 absorbs them, unreleased).

## Checkpoint impact

One format bump (D4).

## Test plan

- **Unit: interlocks** (the forbidden closes), the sequence clock. *As
  built (PR1):* `bess-models/tests/switchgear.rs`: every forbidden move
  refused, an open and a close as a sequence the log shows, the reset
  through the sequence or interlocked, a closing breaker through a
  checkpoint; the library lessons test for `substation-earthing-work`.
- **Property: transformer thermal balance**; the hot spot above top oil.
- **Gate: IEC 60076-7 reference**: the standard's worked example (its
  Annex) reproduced within a stated tolerance. *As built (PR2):* not
  reproduced: the Annex's load steps and temperatures are the
  standard's text, which is sold, and no public copy of the example was
  found; the gate is the rated rises instead (`the_rated_rises_are_the_
  steady_state` holds the top oil to 60 K and the hot spot to 78 K over
  the ambient at rated load), the overshoot of the hot spot on a load
  step, and the hot spot never under the top oil. Named as a gap in
  CALIBRATION.md.
- **Unit: the tap** stays in range and steps once per delay. *As built
  (PR2):* `the_tap_steps_once_per_delay_and_stays_in_range`, and in the
  plant `a_summer_day_swings_the_hot_spot_under_the_limit` (the tap
  follows the grid's swing, the MV bus inside the band most of the day).
- **Integration:** `protection-trip-staggered-return` regenerated with
  the sequence; a new `transformer-overload-day` case. *As built (PR1):*
  the trip case's reset now closes the breaker on the tick after the
  reset (the return's timing is unchanged, since the release runs on that
  same tick); a new maintenance case `substation-earthing-work` shows
  the switchgear in order. *PR2:* no `transformer-overload-day` case: a
  transformer sized at the site rating cannot be overloaded by the plan,
  and the converters' overload (110 % for ten minutes) lifts the hot
  spot by a few kelvin, not to a limit; the hot-week case's header
  carries the hot spot's swing instead, and the emergency trip is
  covered by `transformer_thermal.rs` with a hot state set by hand.
- **Release:** v0.8.0 (version bump, records regenerated, ROADMAP M3
  done, release notes from the inventory, COMPATIBILITY map 0.5.0).
