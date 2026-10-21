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
  ranges are public.
- **D3, the OLTC** with a public transformer datasheet's range and
  step (the common ±9 steps of 1.25 % or ±8 of 1.78 %), a regulator
  dead band of one step and a delay of tens of seconds; the pinned
  sheet decides.
- **D4, checkpoint bump.** Positions, sequence state and timer, two
  transformer temperatures, tap position and regulator timer.
- **D5, points and bits.** Site range: `site.substation.sequence`
  (enum), `site.substation.disconnectors_closed`, `earthing_closed`,
  `site.transformer.top_oil_c`, `site.transformer.hot_spot_c`
  (thermometer instrument), `site.transformer.tap_position`,
  `site.mv_bus.voltage_kv` (analog); site alarm bits
  `transformer_over_temperature` (warning) and `transformer_trip`.

## Signal map impact

Seven additions at unused site addresses, two site alarm bits (map
0.5.0 absorbs them, unreleased).

## Checkpoint impact

One format bump (D4).

## Test plan

- **Unit: interlocks** (the forbidden closes), the sequence clock.
- **Property: transformer thermal balance**; the hot spot above top oil.
- **Gate: IEC 60076-7 reference**: the standard's worked example (its
  Annex) reproduced within a stated tolerance.
- **Unit: the tap** stays in range and steps once per delay.
- **Integration:** `protection-trip-staggered-return` regenerated with
  the sequence; a new `transformer-overload-day` case.
- **Release:** v0.8.0 (version bump, records regenerated, ROADMAP M3
  done, release notes from the inventory, COMPATIBILITY map 0.5.0).
