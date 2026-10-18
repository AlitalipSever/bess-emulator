# v0.6.1: the library shows its lessons

Draft of the release note. The maintainer creates the tag with it:
`gh release create v0.6.1 --title "v0.6.1: the library shows its lessons" --notes-file docs/design/m2_6/release-notes-v0.6.1.md`
(drop this preamble first).

---

v0.6.0 noted two library cases that did not show what their headers
promised. Fixing them found a third. All three had their faults in
hours the day-ahead plan leaves idle, so their records were faithful to
the files but the files no longer told their story. v0.6.1 re-times
them. It changes no physics.

## What changed

- **`hvac-failure-both-units`** now shows the full causal chain from the
  plan alone. Both HVAC units of block 2 fail on the evening of 13 July
  and stay down until after the next evening's discharge, 30 hours. On
  the warmest afternoon of the replayed year the block's cells cross
  the 55 C shoulder of the EVE MB31 limit, and within one discharge come
  the over-temperature warning, the derate, the block's setpoint miss
  and the site's limit, while the other blocks still have charge.
- **`pcs-trip-and-reset`** trips the converter a quarter of an hour into
  the evening discharge, not in standby. The site falls one block short,
  95 MW available, because the other nineteen already run at their
  rating; the reset half an hour later returns it to Run through
  Standby.
- **`hvac-failure-hot-week`** keeps its unit down 30 hours, and the racks
  derate on the evening of the 14th. Its header now says what one
  container does: the block misses by about 1.4 MW, under the 2 MW the
  site's limit waits for.

Why moving the fault time was not enough for the cooling cases: a case
starts from a fresh plant, racks near 20 C, and one charge and one
discharge with the units down end at 54 C. The failure has to last two
cycles.

## A test for what a case is for

A committed record holds a case to its own past, which is how these
three lost their meaning with CI green. `tests/library_lessons.rs` now
plays the two short cases and checks the order of events their headers
describe; both tests fail against the v0.6.0 files.

## Compatibility

The signal map stays 0.3.0, the checkpoint stays format 8, nothing on
the wire changes, and no Rust API changes. The three case records move
(new content hashes, starts, lengths and event counts);
`hvac-failure-both-units` is now 30 hours long instead of 6. Every other
record changes its `engine_version` only, as does the annual
calibration record. The browser module embeds the library, so the npm
package carries the new files.

## Known limits

`hvac-failure-winter-night` does not show its premise either: over the
replayed January the plant cycles twice a day and its own losses keep
the containers near 20 C, so even on the coldest night (-12 C) the
failed container stays above the heater's 10 C threshold, and runs
warmer than its neighbours. Fixing it changes what the case teaches and
is left for its own change. `rack-self-discharge-drift` raises its
imbalance warning on day 5 as its header says, but the warning clears
three hours later.
