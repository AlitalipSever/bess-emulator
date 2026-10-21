# M3 phase 4: substation depth, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, IEC 60076-7 exponents and typical constants.** The standard's
  thermal model is public in its structure and its typical values are
  widely reproduced (top-oil exponent, winding exponent, time constants
  for ONAN/ONAF); the plant's main transformer takes the ONAF column.
  Rated losses from the M0 constants (`no_load_loss_w`,
  `load_loss_at_rated_w`), so the waterfall's transformer row is
  unchanged in energy and gains a temperature.
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
  sheet decides.
- **D4, checkpoint bump.** Positions, sequence state and timer, two
  transformer temperatures, tap position and regulator timer. *As built
  (PR1):* format 15 with the positions, the sequence and its timer; the
  transformer and the tap are PR2's.
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
  `earthing_open` and `earthing_close`.

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
  Annex) reproduced within a stated tolerance.
- **Unit: the tap** stays in range and steps once per delay.
- **Integration:** `protection-trip-staggered-return` regenerated with
  the sequence; a new `transformer-overload-day` case. *As built (PR1):*
  the trip case's reset now closes the breaker on the tick after the
  reset (the return's timing is unchanged, since the release runs on that
  same tick); a new maintenance case `substation-earthing-work` shows
  the switchgear in order. The transformer case is PR2's.
- **Release:** v0.8.0 (version bump, records regenerated, ROADMAP M3
  done, release notes from the inventory, COMPATIBILITY map 0.5.0).
