# Rust idioms in Ephemerust

Ephemerust's mission is twofold: teach the astrophysics, and teach **Rust** — not as a
safer C, but as a language whose type system, ownership model, and compile-time
guarantees change how you *design* software. This chapter walks through the crate's own
code and shows where each of Rust's signature strengths does real work. Every claim
about performance is backed by a reproducible benchmark (`cargo bench`, sources in
`benches/core_operations.rs`).

## 1. Ownership as API design: the `Propagator`

SGP4 propagation has an expensive part and a cheap part. **Initialization** parses the
element set and derives the secular and periodic coefficients; a **propagation step**
just evaluates them at a time offset. Before v0.6.0, every call to `propagate`,
`subpoint`, or `look_angles` silently re-did the initialization — a 90-minute ground
track at 60-second steps re-initialized the same element set 90 times.

Languages with implicit state usually solve this with an "init once, remember to call
it" convention and a runtime check. Rust lets us put the distinction **in the type
system** instead:

```rust
use ephemerust::satellite::{Propagator, Tle};

let tle = Tle::parse(iss_text)?;

// Construction does the expensive, fallible work — once.
let prop = Propagator::new(&tle)?;

// Borrowing methods (&self) are cheap and reusable.
for minutes in 0..90 {
    let state = prop.propagate(tle.epoch + chrono::Duration::minutes(minutes))?;
    // ...
}
```

If you hold a `Propagator`, it *is* initialized — there is no "did I remember to call
`init()`?" state to check, and no way to construct one that skipped the work. That's
ownership expressing an invariant that other languages enforce with documentation.

**Measured effect** (criterion, ISS element set):

| Benchmark | Time | What it shows |
|-----------|------|---------------|
| `propagate_single` (one-shot, init included) | ~890 ns | initialization + one step |
| `propagate_reused` (step on a prebuilt `Propagator`) | ~246 ns | one step alone |
| `ground_track_90min_60s` before → after | 95.1 µs → 38.1 µs | **−59%** from initializing once |

About 72% of a one-shot propagation call was initialization cost. `ground_track` and
`predict_passes` now build one `Propagator` internally and share it across the coarse
scan, bisection refinement, and culmination search. The one-shot free functions remain
as conveniences — their documentation points loops at the struct.

## 2. `static` data: the VSOP87 tables

The VSOP87 planetary theory is a large table of fixed coefficients — amplitude, phase,
frequency triples summed as `Σ A·cos(B + C·t)`. Fixed numeric tables are exactly what
Rust's `static` items are for: the data is **evaluated at compile time** (via `const fn`
constructors) and baked into the binary's read-only section. Borrowing it costs nothing,
and the type system guarantees nobody can mutate it.

Before v0.6.0 the tables were built with `vec![...]` inside getter functions, so *every
planet-position calculation* heap-allocated dozens of `Vec`s, filled them with the same
constants, and dropped them. The fix changed the field types:

```rust
pub struct Vsop87Series {
    pub series_0: &'static [Vsop87Term],   // was: Vec<Vsop87Term>
    // ...
    pub series_4: &'static [Vsop87Term],   // was: Option<Vec<Vsop87Term>> — an empty
    pub series_5: &'static [Vsop87Term],   //      slice now means "unused", no Option
}

static MARS_VSOP87: PlanetVsop87Data = PlanetVsop87Data { /* tables */ };
```

Two lessons ride along:

- **Empty slice beats `Option<Vec>`.** An absent sub-series contributes 0.0 to the sum,
  which is exactly what iterating an empty slice produces — so the `Option` special-casing
  (and its `unwrap`s) simply disappeared from the evaluation code.
- **`const fn` constructors.** The little `vt(amplitude, phase, frequency)` helper is a
  `const fn`, so it runs during compilation, not at runtime.

**Measured effect:** `planet_position_mars` dropped from ~1.01 µs to ~426 ns (**−58%**)
— the entire improvement is allocation removed from a numeric pipeline whose math didn't
change.

## 3. Traits as shared vocabulary: `FromStr`, `Display`

Rust's standard traits are the ecosystem's common language. Implementing them means
your types work with machinery you didn't write:

```rust
use ephemerust::planets::Planet;
use ephemerust::satellite::Tle;

let planet: Planet = "mars".parse()?;        // std::str::FromStr
println!("{planet}");                        // std::fmt::Display → "Mars"
let tle: Tle = tle_text.parse()?;            // FromStr, preserving TleError diagnostics
```

`str::parse` works with *anything* implementing `FromStr` — including clap's value
parsing, config deserializers, and generic code. The CLI's object dispatch
(`parse_celestial_object` in `main.rs`) routes through the same impl, so the library and
the binary agree on what a valid planet name is. `Display` similarly makes `Planet` work
with `format!`, logging macros, and any `T: Display` bound.

The inherent conveniences (`Planet::from_name` returning `Option`, `Tle::parse`) remain
— the trait impls delegate to them. The idiom: **inherent methods for ergonomics, trait
impls for interoperability.**

## 4. Errors as data: enums, `thiserror`, and teaching diagnostics

This has been an Ephemerust strength from the start, and it's worth naming as a Rust
idiom. `TleError` is an enum where each variant *carries the evidence*: the offending
line, the expected and found values, the exact column range of a bad field. `thiserror`
derives `Display` from annotations, and `#[error(transparent)]` folds `TleError` into
the crate-wide `AstroError` without flattening the structure — library callers can still
match the precise variant while the CLI renders the human-readable story plus a
corrective `Hint:` line.

Contrast with error codes (lose the evidence) or exceptions (lose the type): a Rust
`Result<T, AstroError>` is an honest function signature. The compiler makes ignoring a
failure a *choice* (`unwrap`) rather than an accident.

## 5. Documentation that cannot rot: doctests

Every `# Example` block in the rustdoc is compiled and executed by `cargo test --doc`.
The examples in `Propagator`, `FromStr for Tle`, and `Planet::from_name` are not
illustrative pseudocode — they are tests. If a future refactor breaks an example, the
build breaks. This is why the crate enforces `#![warn(missing_docs)]` and
`#![deny(rustdoc::broken_intra_doc_links)]`: documentation here is part of the checked
surface, not a parallel artifact.

## 6. Measure, don't guess: criterion benchmarks

All of the numbers in this chapter come from `benches/core_operations.rs`, run with
`cargo bench`. Criterion handles warm-up, outlier detection, and statistical comparison
against the previous run — after a change, it reports whether performance *actually*
moved (`p = 0.00 < 0.05`) rather than leaving it to eyeballs. The benchmarks cover TLE
parsing, VSOP87 planet positions, one-shot vs reused propagation, and a full ground
track, so the crate's hot paths stay honest as it evolves.

A useful discipline demonstrated by the v0.6.0 changes: **benchmark before, change,
benchmark after, publish both numbers.** The [CHANGELOG](../CHANGELOG.md) records the
before/after table for the release.

## 7. A deliberate non-idiom: no unit newtypes (yet)

The textbook type-system flex would be `Degrees(f64)` / `Radians(f64)` newtypes (or a
units crate) so the compiler rejects passing radians where degrees are expected. Ephemerust
deliberately uses documented conventions instead (degrees and UTC unless stated, RA in
hours). The trade-off: newtypes add real safety but also noise in teaching code, and a
sweeping API break. This is a genuine design tension in Rust — safety machinery has a
readability cost — and the crate currently lands on the "documented convention" side.
If the API grows more mixed-unit call sites, that decision should be revisited.

## Where to look in the source

| Idiom | Where |
|-------|-------|
| Init-once ownership | `satellite.rs` — `Propagator` |
| `static` tables + `const fn` | `planets.rs` — `MERCURY_VSOP87` … `NEPTUNE_VSOP87`, `vt`, `series` |
| Standard traits | `planets.rs` — `Display`/`FromStr for Planet`; `satellite.rs` — `FromStr for Tle` |
| Structured errors | `satellite.rs` — `TleError`; `lib.rs` — `AstroError` |
| Doctests | throughout; run `cargo test --doc` |
| Benchmarks | `benches/core_operations.rs`; run `cargo bench` |
