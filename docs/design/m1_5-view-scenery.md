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

### Open against PR1, from its review

Five findings, two of them visible to a viewer. Recorded here rather than only
in the pull request, because the second one is a gap in the method and not
just in the code:

1. **Refraction is discontinuous.** The cutoff at a true elevation of -1
   degree drops 0.6466 degrees of refraction in one step, which moves the sky
   blend by 8% instantaneously at every dusk and dawn. The cutoff is necessary,
   since the formula has a pole at -5.11 degrees; the taper is what is missing.
2. **Nothing asks for continuity,** which is why the first finding survived a
   suite that samples solstices, equinoxes, daily peaks and 8784 hours. Every
   test in it evaluates points; none walks a transition. This is a standing
   lesson for PR2 and PR3, where dimming and the fast-forward both have
   transitions of their own.
3. **The light direction snaps** from sun to moon while the day term is still
   at 9% intensity, so shadows pop at sunrise and sunset. The colour ramp was
   made continuous in PR1 and the direction was not.
4. **The daylight cross-check never asserts the sun sets,** so a sun stuck
   above the horizon would satisfy both of its directional checks. The
   correlation test catches it only by producing NaN.
5. **An unverifiable sentence in the module doc** appeals to what the overview
   camera shows, and that camera auto-orbits.

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

## 9. Open questions

- **The release version.** ROADMAP.md says v0.3.x, but `PlantConfig` gaining a
  field is a source-level break and the M0.5 precedent gave a mini-iteration a
  minor bump. v0.4.0 with the roadmap line corrected is the consistent choice.
  Decide when PR3 lands.
- **Where the presets live.** PR3 computes "warmest day" and friends from the
  `WeatherYear` slices in the viewer. If a second shell ever wants them they
  belong in `bess-data` beside the series they summarize. Left in the viewer
  until there is a second caller.
- **Whether humidity gets a consumer.** It is compiled and licensed as
  scenery-only along with the other three, but nothing in the PR2 scope reads
  it. Either it drives something visible (haze) or the honest move is to say
  in DATA-LICENSES.md that it is carried without a consumer.
