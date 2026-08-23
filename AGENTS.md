# Code organization

The operating contract for how this repository is cut into files. What the
code has to *do* lives in ARCHITECTURE.md, ROADMAP.md and CALIBRATION.md; this
file is only about where it goes.

## The rule that leads

**One file, one concern.** A file you cannot describe without saying "and" is
two files. This is the rule; the line counts below are the symptom it usually
shows up as, not the rule itself.

A concern is not the same as a type or a function. Two things belong apart
when a reader has to switch what kind of question they are asking. The clearest
recent case: solar position and scene lighting used to share a file. One is
astronomy and makes claims about the world that tests hold against published
geometry; the other is art direction and makes choices about a picture. They
are now `sun/position.rs` and `sun/light.rs`, and the split is worth two files
because it tells a reader which lines they are allowed to argue with.

The same applies to a whole domain arriving. When work introduces a genuinely
new subject rather than more of an existing one, it starts in its own module
directory from the first line, before any length question comes up.

## The line counts

Rust has no standard file-length lint. Clippy's `too_many_lines` bounds
functions at 100 and stops there, and the compiler's own source has files in
the thousands, so nothing here is quoted from an authority. These are our
numbers, chosen so that a file can be read in one pass by a person and held in
one piece by an agent.

Counted over the whole file, doc comments and tests included:

| | Lines | What it means |
|---|---|---|
| Soft | 300 | Stop and look for the second concern. Usually there is one. |
| Hard | 500 | Split before the change lands. |

**When tests are what push a file over**, the answer is never fewer tests. Move
them to the crate's `tests/` directory if they only need the public API, as
`bess-scene/tests/solar_daylight.rs` does, or split the module so each half
carries the tests that belong to it.

## How to split

Rust 2018 module style, no `mod.rs`:

```
src/sun.rs          module doc, submodule declarations, re-exports,
                    and anything genuinely shared by the children
src/sun/position.rs one concern
src/sun/light.rs    the other
```

The parent keeps the shared vocabulary so the children cannot drift: the
smoothstep both halves of `sun` need lives in `sun.rs`, because two copies of a
curve is how two copies stop matching. Re-export from the parent so callers
outside the module are not disturbed by an internal reorganization.

Test fixtures shared by sibling modules go in the parent behind `#[cfg(test)]`,
for the same reason.

## Current state

Six files predate this contract and exceed the hard limit. They are named here
rather than allowlisted in a tool, because an allowlist in a tool is a place
for debt to become invisible:

| File | Lines | Code | Tests |
|---|---|---|---|
| `crates/bess-emulator/src/map.rs` | 946 | 703 | 243 |
| `crates/bess-models/src/thermal.rs` | 683 | 342 | 341 |
| `crates/bess-scene/src/instances.rs` | 680 | 635 | 45 |
| `crates/bess-data/src/lib.rs` | 568 | 545 | 23 |
| `crates/bess-emulator/src/http.rs` | 544 | 394 | 150 |
| `crates/bess-data/src/bin/compile-weather.rs` | 501 | 501 | 0 |

The CI check that enforces the hard limit lands in the same pull request that
clears this table. Turning it on first would mean either failing every build or
shipping an exemption list, and this repository has a rule about gates: a gate
that does not fail is not a gate.

New and rewritten files comply from now on, and the contract applies to a file
the moment a change touches it.
