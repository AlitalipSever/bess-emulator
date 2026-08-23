# M1.5 design: solar position, weather-driven scenery, time controls

Status: written after PR1 landed, which is the wrong order and is recorded as
such. M1 established the norm that a milestone gets its design document before
its first commit; this iteration was small enough that the norm felt optional,
and then PR1's review produced five findings that a design pass would have
caught two of. The document is here now, PR1's outcomes are folded into it,
and the remaining PRs get the treatment they should have had from the start.

ROADMAP.md defines M1.5's scope and states that it carries no calibration
gate. This document fixes the architecture, the decisions, and the work
breakdown.

## 1. What M1.5 changes

M1 gave the plant a real weather year, thermal memory, staged cooling and an
itemized house load, and then measured the whole thing. None of it reached the
scene. M1.5 is the iteration where the view layer catches up with the kernel
it has been drawing.

Two things change, and only one of them is visual.

1. **The site acquires a position on Earth.** Until now the coordinates lived
   in prose in ARCHITECTURE.md and in a lookup table of approximate German
   sunrise hours. They become part of the site descriptor, which is what they
   always were.
2. **The scenery-only half of the weather dataset acquires a consumer.** Four
   of the eight compiled series (precipitation amount and form, wind speed and
   direction, cloud cover, humidity) are shipped, licensed, gap-filled under a
   documented slack policy, and read by nobody. `bess-data`'s own module docs
   call them scenery-only. This iteration makes that label true rather than
   aspirational.

No kernel model changes. No new physics. The plant computes exactly what it
computed at v0.3.0.

## 2. The boundary

The scene has always been a projection of the state tree: it reads
`&SiteState`, emits `ViewerCommand`s, and never steps the kernel. M1.5 adds a
second input to the projection, the scenery series, and the boundary has to
survive that addition.

The rule, unchanged from `sun.rs`'s original header and now applying to more
of the module: **every visual effect is driven either by a measured series or
by pure mathematics on position and time.** No invented decoration.

Two consequences worth stating as invariants rather than intentions:

- **The kernel never reads the scene.** `Inputs` stays at ambient temperature
  and irradiance. Cloud cover, precipitation and wind reach the view layer
  without passing through the kernel at all, which is why they cannot
  accidentally become physics.
- **The only authority for radiation in the energy accounting is the measured
  irradiance.** The scene computes a clear-sky expectation in order to decide
  how bright to draw the sun; that number never leaves the view layer. If it
  ever fed a thermal model, the site would be heated by its own art direction.

## 3. Design decisions

**D1, the site's coordinates go in `PlantConfig`.** They are site identity,
not view state, and ARCHITECTURE.md already treated them that way: "Solar
position, ambient conditions, and the market zone all follow from that point."
The alternative, a constant in `bess-scene`, keeps the iteration strictly
inside the view layer but puts a property of the plant in whichever layer
happened to need it first, and M4 would then either copy it or move it.

Cost, checked before taking it: no checkpoint impact, because the envelope in
`checkpoint.rs` carries `SiteState` and not the config; no protocol impact,
because no REST route exposes the config; a source-level break for anyone
building a `PlantConfig` from a struct literal, which `PlantConfig::gw01()`
covers for every shipped path.

**D2, scenery reaches the scene as plain numbers.** `bess-scene` depends on
`bess-models` only behind the `sim` feature and must keep building without it,
so the scene defines its own `Scenery` struct of primitives and its own small
precipitation enum. The viewer, which owns the `HistoricalWeather` and can
already reach `year()`, builds it and does the mapping. This is also what
keeps D2 from quietly undoing the boundary in section 2: the scenery type
cannot reach the kernel because the kernel does not know it exists.

**D3, particles reuse the instanced-cube pipeline.** `renderer.rs` is the only
module with GL calls and the only one allowed `unsafe`; a second pipeline for
rain would widen that surface for decoration. Rain and snow become instances
like everything else, positioned by a hash of particle index and animation
time, which also keeps the scene a pure function of `(state, scenery, time)`
with no particle state to own.

**D4, fast-forward runs against a wall-time budget, not in a blocking loop.**
The browser build is single-threaded, so a blocking catch-up would freeze the
canvas and show no progress. A per-frame budget keeps the scene drawing while
the plant jumps, which is also the better demo. No rewind: going back means
restarting at that date, because the kernel has no inverse.

**D5, the sun is computed, not fitted.** PSA (Blanco-Muriel et al. 2001) with
the 2020 coefficient refit, in about fifty lines with no dependency. A crate
would have been shorter to write and longer to justify: this workspace keeps
its dependency list short because all of it compiles to WASM, and solar
position is arithmetic.

**D6, the lighting is derived from the elevation, not decorated onto it.**
Daylight is the sine of the solar elevation, the same cosine-of-zenith that
decides how much of a beam a horizontal surface catches. At 52 N that means a
January noon reads about 0.25 against July's 0.88 rather than both saturating,
which is the honest picture: the site never sees a tropical noon. The colour
and twilight ramps on top of it are art direction and say so in their own doc
comments, so a reader can tell which lines are claims about the world.

## 4. Scope boundaries

Deliberately out:

- **No physics.** Cloud cover does not dim the thermal model. The measured
  irradiance already carries whatever the clouds did.
- **No shadow mapping, no volumetric anything.** The renderer keeps one
  pipeline.
- **No terrain, no vegetation, no vehicles.** Nothing that is not driven by a
  measured series or by position and time.
- **No scene-side persistence.** The date jump restarts or fast-forwards the
  kernel; it does not snapshot or restore. Checkpoints are a kernel surface
  and stay one.
- **Grid frequency stays synthetic** until M4, so nothing in the scene should
  imply otherwise.

## 5. Work breakdown

Three PRs, each landing green.

1. **Real solar position.** `PlantConfig::location`, PSA in `sun.rs`,
   refraction, lighting derived from elevation. Accept: solar geometry inside
   published bounds, and agreement with the measured irradiance year about
   when it was light.
2. **Weather-driven scenery.** `Scenery`, cloud and dimming, precipitation and
   wind particles. Accept: the dimming ratio stays bounded and behaves against
   the observed cloud series; particle density responds to the measured rate.
3. **Time controls and weather panels.** Date jump, fast-forward with
   progress, dataset presets, weather and thermal rows in the panels. Accept:
   a jump reaches the same state the same run would have reached, and the
   canvas keeps drawing while it does.

### Done in PR1

The measured sun, against geometry: June solstice noon 61.24 degrees against
the 61.23 latitude and obliquity predict, December 14.41 against 14.35 plus
refraction, equinox 37.75 against 37.79, solar noon between 10:56 and 11:11
UTC across the year against the 11:03 the longitude sets, and a peak
declination of 23.43 degrees, which reads the obliquity coefficient directly.

The test that carries the weight is not any of those. Over all 8784 hours of
the replayed year, the computed sun and the DWD pyranometer at the same
coordinates have to agree about when it was light. That is evidence of a
different kind from the geometry checks, which could all be satisfied by a
subtly wrong algorithm, and it is not a second copy of the formula under test.
A flipped longitude sign and a six-hour clock offset were both injected and
both fail it loudly.

### PR1's review, and what it changed

Five findings, two of them visible to a viewer, all fixed in the same pull
request. Recorded here rather than only in the review thread, because the
second one is a gap in the method and not just in the code.

1. **Refraction was discontinuous.** The cutoff at a true elevation of -1
   degree dropped 0.6466 degrees of correction in one step, which moved the
   sky blend by 8% instantaneously at every dusk and dawn. The cutoff itself
   is necessary, since the formula has a pole at -5.11 degrees and stops being
   monotone below about -2; what was missing was the taper. The correction now
   fades to zero over the band from -0.5 to -2 degrees, clear of the pole and
   below the last elevation where a visible disc is being refracted.
2. **Nothing asked for continuity,** which is why the first finding survived a
   suite that samples solstices, equinoxes, daily peaks and 8784 hours. Every
   test in it evaluated points; none walked a transition. Continuity is now a
   test category rather than an assumption: `nothing_the_eye_integrates_moves_in_steps`
   walks four days at ten-second resolution and bounds how far elevation,
   daylight and the sky ramp may move in one step. It fails on the old code by
   two orders of magnitude. This is the standing lesson for PR2 and PR3, where
   dimming and the fast-forward both have transitions of their own.
3. **The light direction snapped** from sun to moon at half a degree of
   elevation, while the day term was still at about a tenth of its intensity,
   so shadows swung at every sunrise and sunset. PR1 had made the colour ramp
   continuous and left the direction where it was. The handover now happens at
   the horizon, where the day term is zero by construction.
4. **The daylight cross-check never asserted the sun sets,** so a sun stuck
   above the horizon would have satisfied both of its directional checks, the
   first because its premise never fires and the second vacuously. The
   correlation test caught it only by producing NaN, which is not protection.
   The hour count is now asserted directly against the roughly even split this
   latitude gives.
5. **An unverifiable sentence in the module doc** appealed to what the
   overview camera shows; that camera auto-orbits, so there was no fixed
   viewing direction to appeal to. It now points at the test that holds the
   claim instead.

One thing worth recording from the fixing rather than the review. The first
version of the test for finding 3 asserted that the light was below 0.15 in
intensity when the direction swung. The actual value at the old threshold was
0.146, so the test passed against the very defect it was written for. It was
rewritten to assert on daylight rather than on colour, with a bound derived
from the walk resolution instead of chosen round: ten seconds is at most 0.042
degrees of elevation, so a correct handover leaves at most 7.3e-4 of daylight
behind, while the old one left 8.7e-3. A guard is only worth what it has been
shown to catch, and this one had to be shown twice.

### Code organization, adopted mid-iteration

PR1 produced a 494-line `sun.rs` and that prompted a contract the repository
did not have: AGENTS.md now states one file, one concern, with a soft ceiling
at 300 lines and a hard one at 500, counted over the whole file.

`sun.rs` was the first thing held to it, and length turned out to be the
symptom rather than the problem. The file carried three subjects: astronomy,
art direction, and calendar arithmetic that had nothing to do with the sun at
all and lived there only because a deleted lookup table once needed the month.
It is now `sun.rs` (module doc, the shared ramp, test fixtures),
`sun/position.rs`, `sun/light.rs` and a separate `clock.rs`, at 77, 229, 208
and 56 lines.

The split is worth two files for `sun` specifically because one half makes
claims about the world and the other makes choices about a picture, and a
reader should be able to tell which lines they are allowed to argue with
without reading the doc comments.

Six pre-existing files exceed the hard limit. They are named in AGENTS.md
rather than hidden in a tool exemption list, and they are cleared as work
reaches them rather than in a campaign: a refactor whose only purpose is a line
count moves risk into files nobody was otherwise changing. Within this
iteration that means one of them, `instances.rs`, which PR2 splits because
precipitation particles are a new concern in it.

### Done in PR2

`instances.rs` split first, because precipitation is a new concern in a file
that was already over the limit: `instances/site.rs` for what the plant is
made of, `instances/live.rs` for what it is doing this frame,
`instances/weather.rs` for what the sky is doing, and a root holding the
instance format and the one test that belongs to all three. The root went
from 680 lines to 81. `site.rs` sits at 342, above the soft ceiling, and stays
one file: ground, containers, skids, substation and fence are one concern, and
five seventy-line files would be fragmentation bought with a number.

`Scenery` carries six numbers and no `bess-data` type, so the scene still
builds without the `sim` feature and the kernel still cannot see a series it
does not consume. The viewer does the translation, which needed one addition
upstream: `HistoricalWeather::hour_at` returns the whole observed bucket,
uninterpolated, because cloud cover is an integer count of eighths and
precipitation form is a category, and averaging either across an hour boundary
would invent a reading nobody took. `bess-models` re-exports `HourSample` and
`PrecipForm` rather than making callers depend on `bess-data` to name what it
hands them.

Two independent witnesses, kept independent. Cloud cover greys the sky;
dimming, the pyranometer reading over a Haurwitz clear-sky expectation,
darkens the sun. They are never averaged, so a disagreement shows in the
picture instead of vanishing into it. What holds them honest is a test over
the real year: hours at 0 to 2 okta average 0.87 of their clear-sky
expectation, hours at 7 to 8 okta average well below that, and the
eighth-by-eighth progression trends down throughout. Feeding the clear-sky
model degrees instead of radians collapses the gap to 0.12 and fails it.

The dimming ratio reports 1 below three degrees of elevation, where its
denominator collapses. That is safe rather than arbitrary: the only thing it
multiplies is the sun's own brightness, which the lighting has already taken
to zero by then. Continuity across that floor is held by a test, which is the
category PR1's review added.

Particles carry no state. A position is a pure function of index, frame clock
and observed weather, so a jump to another date and back produces the
identical frame, and nothing has to be seeded or reset. Density follows the
square root of the measured rate, because a linear map spends the whole
budget on the first millimetre.

### PR2's review, and what it changed

Five findings. The first is the one worth remembering, because it is the same
defect the register map's review found in PR6 and it arrived by the same road.

1. **The mapping from dataset to scene had no test.** Six field-to-field
   assignments, buried in a method that needs a GL context to reach, which is
   exactly the shape that silently swaps two of them. It is a free function
   now, `observed_from`, tested by giving every source field a distinct value
   and asserting each lands in its own slot. Swapping wind speed and direction
   fails it; so does letting humidity, which has no consumer, leak into the
   irradiance field.
2. **`Scenery::default()` was a black sun.** The derive zeroed `dimming`,
   which means fully dimmed, so a correct-looking call produced a sun with
   nothing coming out of it. `Default` is `clear()` now. A default that has to
   be avoided is a trap left in a public type.
3. **An unnamed factor in a file claiming not to have any.** The module doc
   says every effect is driven by a measured series or by pure mathematics,
   and then `wind_ms * 0.35` sat in the middle of it. It is `WIND_CARRY` now,
   with the arithmetic that justifies it: honouring the real drift would sweep
   the field further than the site is wide.
4. **Wind direction zero is the dataset's word for calm, and the code read it
   as north.** Latent rather than live: Lindenberg 2024 never uses the
   sentinel, which was checked before the finding was written. A calm hour
   carries nothing now, and 360 still means north because the trigonometry
   already agreed.
5. **The instance buffer's capacity hint predated particles** and a heavy hour
   could push past it.

### Done in PR3

Two files were going to cross the ceiling, so the structure came first again.
`viewer.rs` grew `viewer/jump.rs` and `viewer/presets.rs`; `panels.rs` grew
`panels/site.rs`, `panels/controls.rs` and `panels/detail.rs`. Nothing over
272 lines afterwards.

The split paid for itself immediately in `jump.rs`. Fast-forward inside the
eframe app would have needed a GL context to test, which in practice means it
would not have been tested. On its own it takes a `Simulation` and a tick
budget, so the claim that matters can be asserted directly: a jump reaches the
same state a plain run would have reached, byte for byte, and it does so
across deliberately awkward budget sizes so that the frame split cannot be
what makes it come out right.

The budget is in ticks rather than wall time. The browser has no monotonic
clock without a shim, and a tick count means the same thing on both targets.
3000 a frame is fifteen milliseconds natively and several times that in the
browser, which still leaves the canvas moving, and moving is the point: a
blocking catch-up would show a frozen screen for the length of the jump.

Presets are derived rather than chosen. "Warmest day" is a fact about the
compiled series, so it is read off the series, by daily total rather than by
peak hour: the hottest single hour of the year can belong to an otherwise
ordinary day, and someone clicking that button wants the day that was warm.
The tests pin the season rather than the date, so refreshing the reference
year does not rewrite them.

`Stop` lives in `panels.rs` rather than beside the code that derives it,
because a label and a date are panel vocabulary and the panel has to build
without the `sim` feature. Same boundary as `Scenery`, same reason.

The panel readouts finally show what M1 built. Ambient, irradiance against
its clear-sky expectation, cloud in okta, container air across the site, cell
extremes, and how many of the forty containers are cooling or heating. The two
that walk the tree are pure functions with tests, because a count that drifts
from the fleet size is exactly what nobody notices in a screenshot.

### What a screenshot found that no test did

Ali opened the plant and asked why the rain, the clouds and the sun were not
there. Four answers, three of them defects, and none of them caught by a
suite that had grown to fifty-five tests.

**Shadows never moved.** The scene has projected a fake planar shadow since
M0, and the vertex shader flattened each object along a hardcoded offset,
`world.x + h * 0.42`. It never read the light direction. So PR1 made the sun
real, wired it into the light's direction and colour, and missed the one
thing that makes solar position visible from the ground. Worse, PR1's own
notes told the reader to go and confirm that winter shadows are long, which
could not have been true. The shadow now lands where the ray leaving a point
meets the floor, clamped so a sun at the horizon does not stretch it to
infinity.

**Nothing drew the sun.** The scene drew ground and cubes over a flat clear
colour; there was no sky, so there was nowhere for a sun to be. There is a
sky pass now, a fullscreen triangle before the scene, sharing the present
pass's vertex shader rather than adding a second pipeline. It holds a
gradient, the sun at its computed position, and a cloud deck thresholded out
of fractal noise by the observed okta series and drifting on the observed
wind. Where the sun sits is astronomy; how large it is drawn is art
direction, and the file says which is which. Drawn at its true half degree it
would be two pixels and nobody would find it.

**The rain was real and the moment was not.** At the hour in the screenshot
the dataset reads 0 okta and 0 mm, so the correct picture was an empty sky.
Only 748 of the year's 8784 hours carry any precipitation at all.

**The stop meant to solve that landed in the dry.** "Wettest day" picked the
day with the largest total and opened at nine in the morning. The wettest day
of the reference year holds 34.7 mm, of which 32.9 falls in the single hour
at 20:00. Anything this spiky has to be found by the hour, not by the day, so
the precipitation stops now open an hour before the hour they found, and a
test asserts that rain actually falls within two hours of where the stop
lands. A snow stop joined it, since the year has 49 hours of it.

The lesson is the one the continuity finding already taught, one level up.
Every test here asks whether a number is right. None of them could ask
whether anything was visible, and three defects lived comfortably in that
gap. A view layer's last gate is a person looking at it.

## 6. Compatibility impact

- **Checkpoint format: unchanged.** No state field is added. The digest does
  not move.
- **Signal map: unchanged.** The scene is not a published surface, and CI
  diffs the map on every pull request to keep that true.
- **Calibration record: unchanged.** No physics changes, so `bess-bench` must
  return the annual figures to the digit. A moved number would mean something
  leaked from the scene into the kernel, which makes the existing gate a
  boundary test for this iteration at no extra cost.
- **Source-level breaks**, pre-1.0 and called out: `PlantConfig` gains
  `location`; `sun::sun_at` takes the site location; `SunLight` gains `sky`;
  `SceneView::show` will take a `&Scenery` in PR2; `ViewerCommand` gains two
  variants in PR3, so a downstream `match` on it stops being exhaustive.

## 7. Test plan, and why there is no calibration gate

M1.5 adds no physics, so there is no realism claim to hold against public
data, and inventing a gate would be worse than admitting there is none. A view
iteration that needed a calibration gate would mean the scene had started
deciding something.

What replaces it:

- **Agreement with the measured series.** Where the scene derives something
  the dataset also knows, the two are checked against each other: the sun
  against measured irradiance in PR1, the dimming ratio against the observed
  cloud series in PR2.
- **Physical bounds.** Solar geometry against latitude and obliquity;
  ratios inside [0, 1]; monotone responses where a monotone response is the
  whole claim.
- **Continuity.** Added as a category in response to PR1's review: any
  quantity the eye integrates over time gets walked across its transitions,
  not sampled at points.
- **The kernel-side gates as boundary tests.** Signal map diff, golden
  determinism digest, and `bess-bench --check` all have to come back unmoved.
  They are not about the view, which is exactly what makes them useful here.
- **By eye, on a running viewer.** Screenshots come from Ali; no headless
  browser is installed for this or anything else.

## 8. Release-note inventory

Kept here as the PRs land, so the release note is an edit rather than an
archaeology exercise.

- **`PlantConfig` gains `location`** (PR1): latitude and longitude of the
  site, set for GW-01 to the DWD station its weather replays. A struct literal
  built by hand needs one more field. No checkpoint or protocol impact.
- **The scene's sun is computed rather than tabulated** (PR1): real solar
  position from the site coordinates replaces a monthly sunrise and sunset
  table. Winter days are visibly shorter and lower than summer days, and
  daylight intensity now differs between seasons instead of saturating in
  both.
- **`sun::sun_at` and `SunLight` changed shape** (PR1): the function takes a
  `SiteLocation`, and `SunLight` carries a `sky` ramp beside `daylight`.
- **`bess-data` is a dev-dependency of `bess-scene`** (PR1), so the sun can be
  tested against the measured year. It does not reach the browser build or a
  downstream consumer.
- **`SceneView::show` takes a `&Scenery`** (PR2). Callers without an
  observation series pass `Scenery::clear()`.
- **New `HistoricalWeather::hour_at`** (PR2), returning the observed hour
  bucket uninterpolated, and `bess-models` now re-exports `HourSample` and
  `PrecipForm`.
- **`instances` became a module directory** (PR2). The names callers used are
  re-exported from the root, so nothing outside the module had to change.
- **Two new `ViewerCommand` variants** (PR3), `RestartAt` and
  `FastForwardTo`, so a downstream `match` on the enum stops being exhaustive.
- **`panels::side_panel` takes a `&PanelInput`** (PR3) instead of loose
  arguments, and `PanelState` gained the date fields and a jump progress
  slot.
- **`viewer` and `panels` became module directories** (PR3), with the names
  callers used re-exported or unchanged.
- **New `clock::unix_from_civil`** (PR3), the inverse of `civil_from_unix`.

## 9. Open questions

- **The release version.** ROADMAP.md says v0.3.x, but `PlantConfig` gaining a
  field is a source-level break and the M0.5 precedent gave a mini-iteration a
  minor bump. v0.4.0 with the roadmap line corrected is the consistent choice.
  Decide when PR3 lands.
- **Where the presets live.** PR3 computes "warmest day" and friends from the
  `WeatherYear` slices in the viewer. If a second shell ever wants them they
  belong in `bess-data` beside the series they summarize. Left in the viewer
  until there is a second caller.
- **Whether humidity gets a consumer.** Resolved for now by saying so:
  DATA-LICENSES.md records that it is carried without one, and why it is kept
  anyway (it arrives in the same DWD product as the temperature the physics
  uses, so dropping it would mean re-fetching to get it back). If haze ever
  earns its place in the scene, that is where it comes from.
