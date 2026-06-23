# Satellite Tracking & Pass Prediction — Iterative Plan

This document records the staged, test-driven plan for satellite tracking and pass prediction.
**Milestones M0–M9 are complete** (see stage gates below). Remaining items live under
[Future work](#future-work).

## Strategy and positioning

The work follows a **library-first** design that serves two reinforcing goals at once:

1. **A reliable satellite-tracking toolset.** End-to-end prediction from a Two-Line Element
   set (TLE) to observer-relative look angles and visible passes.
2. **A transparent, well-documented Rust library.** The conversion and prediction layer is
   documented to the same standard as the existing `docs/` set, filling a gap in the Rust
   ecosystem.

### Build-vs-buy decision

| Concern | Decision | Rationale |
|---------|----------|-----------|
| SGP4/SDP4 propagator | **Depend on the [`sgp4`](https://crates.io/crates/sgp4) crate** | The algorithm is intricate (deep-space resonance, drag, edge cases) and the crate is validated against the reference implementation. Reimplementing it for production carries high correctness risk and no functional payoff. |
| Conversion & prediction layer (TEME → ECEF → geodetic → topocentric → passes → ground track) | **Implement in this project** | This is the layer the raw propagator deliberately omits, and the project already provides most of the required primitives (GMST/sidereal time, ECEF↔ECI, Alt/Az, observer location). |
| Teaching implementation of SGP4 | **Implement separately as an educational module + documentation** | Lower correctness pressure; validated against the production `sgp4` crate as an oracle. Serves the teaching goal without endangering production output. |

### Frame and convention notes

- The `sgp4` propagator emits position and velocity in the **True Equator Mean Equinox
  (TEME)** frame. TEME is an Earth-centered inertial frame that does not rotate with the
  Earth and is distinct from J2000/ICRS.
- The supported conversion path is **TEME → ECEF via a Greenwich sidereal-time rotation
  about the Z axis**, then **ECEF → geodetic latitude/longitude/altitude on the WGS84
  ellipsoid**. Precession and nutation are intentionally omitted at this stage, consistent
  with the project's existing accuracy posture (see
  [accuracy-and-limits.md](accuracy-and-limits.md)).
- Production propagation uses the `sgp4` crate's **default (WGS84 + IAU)** path via
  [`propagate`](../src/satellite.rs); geodetic output uses the **WGS84** ellipsoid — the
  two are aligned. Optional **AFSPC compatibility** (`--afspc` / [`PropagationModel::AfspcCompatibility`])
  selects WGS72 + legacy sidereal/epoch handling to match Vallado / legacy NORAD reference
  implementations (tens-of-metres difference vs the default path). The [`sgp4_teaching`](../src/sgp4_teaching.rs)
  module still uses WGS-72 μ for pedagogy. Residual subpoint/look-angle error is dominated by
  TLE age and omitted precession/nutation (see [accuracy-and-limits.md](accuracy-and-limits.md)).

## How to use this plan

- Milestones were executed in order; each stage gate below records completion status.
- A milestone is "done" when every item under **Deliverables** exists, every item under
  **Test plan** passes (or documented substitutes), and the **Stage gate** is marked complete.
- Each milestone updates the `[Unreleased]` section of [`CHANGELOG.md`](../CHANGELOG.md),
  the status tables in [`readme.md`](../readme.md), and the test counts in
  [`architecture.md`](architecture.md).

---

## Milestone 0 — Foundation and dependency integration

Establish the dependency, module skeleton, and conventions so later milestones have a stable
base.

### Deliverables

- Add the `sgp4` crate to `Cargo.toml`.
- Create a `satellite` module (`src/satellite.rs`) with a documented module header stating
  the frame conventions above, and register it in `lib.rs`.
- Add public type stubs (`Tle`, `TemeState`, `Subpoint`, `LookAngles`, `Pass`) with rustdoc
  and `TODO` bodies that compile.
- Extend `AstroError` with a `SatelliteError` (or equivalent) variant for TLE/propagation
  failures.
- Add a `track` CLI subcommand stub that parses arguments and returns a "not yet
  implemented" message.

### Test plan

- `cargo build` and `cargo test` are green with the new dependency.
- A smoke test propagates a canonical reference TLE (the ISS) and asserts the position
  magnitude lies in a plausible low-Earth-orbit band (approximately 6,600–7,100 km from
  Earth's center).
- A doctest on the module header compiles.

### Stage gate

Dependency resolves and builds on the target environment; the module and error plumbing are
in place; the smoke test passes. **Signed off** (Milestone 0 complete; types and `track` are
fully implemented, not stubs).

---

## Milestone 1 — TLE ingestion and parsing

Provide robust parsing and validation of element sets from local input.

### Deliverables

- A typed `Tle` wrapper that parses standard 2-line and 3-line (named) TLE text, exposing
  the satellite catalog number, epoch, and orbital elements.
- Input validation: line-length checks, TLE checksum verification, and actionable error
  messages consistent with the project's error-handling conventions.
- Support for reading TLEs from a file path and from an inline string argument.
- (Network fetch from CelesTrak/Space-Track is deferred to a later, feature-gated
  milestone.)

### Test plan

- Parse a canonical ISS TLE and assert each parsed field against known reference values.
- Checksum validation: a TLE with a corrupted checksum is rejected with a clear error.
- Malformed inputs (wrong line count, truncated lines, non-numeric fields) each produce a
  distinct, actionable error.
- Round-trip selected fields against published Vallado verification element sets.

### Stage gate

Valid TLEs parse correctly; invalid TLEs fail with clear errors; reference fields match.
**Signed off** (Milestone 1 complete; network fetch deferred — see [Future work](#future-work)).

---

## Milestone 2 — Propagation wrapper (TEME state)

Wrap the `sgp4` propagator behind a small, typed, well-documented interface.

### Deliverables

- `propagate(tle, time) -> TemeState`, returning position (km) and velocity (km/s) in the
  TEME frame, with the input time expressed as a UTC datetime.
- Conversion between UTC datetime and the propagator's minutes-since-epoch representation.
- Documented handling of propagation errors (element divergence, out-of-range epoch).

### Test plan

- Propagate the published SGP4 verification TLEs at the verification time offsets and
  compare position/velocity to the reference values within a documented tolerance.
- Propagate at epoch (t = 0) and confirm agreement with the element-set state.
- Confirm error propagation for a deliberately divergent element set.

### Stage gate

Propagated states match reference vectors within tolerance (default WGS84 path; optional AFSPC
mode matches Vallado reference to sub-metre); error paths behave as documented, including
`sgp4` regression divergence fixtures. **Signed off** (Milestone 2 complete).

---

## Milestone 3 — Frame conversions: TEME → ECEF → geodetic

Turn inertial states into Earth-fixed positions and sub-satellite ground points.

### Deliverables

- `teme_to_ecef(state, time)` using a Greenwich sidereal-time Z-axis rotation, reusing the
  existing GMST implementation.
- `ecef_to_geodetic(ecef) -> Subpoint` (geodetic latitude, longitude, altitude) on the
  WGS84 ellipsoid via a documented closed-form or iterative method (e.g. Bowring).
- A `subpoint(tle, time)` convenience returning the sub-satellite latitude/longitude/altitude.

### Test plan

- `ecef_to_geodetic` round-trips against a forward geodetic-to-ECEF computation within
  ~1 mm at Earth scale.
- The sub-satellite point for a reference satellite at a known time agrees with an external
  reference (Skyfield or Heavens-Above) within the documented error budget (target: tens of
  kilometers, dominated by TLE age and omitted precession/nutation).
- Boundary cases: equatorial subpoint, high-inclination subpoint near the poles, and
  longitude wrap at ±180°.

### Stage gate

Round-trip accuracy holds; sub-satellite point matches the external reference within budget.
**Signed off** (Milestone 3 approved; geodetic round-trip, plausibility, and subpoint boundary
tests — equator crossing, polar latitude, ±180° longitude wrap — in-tree).

---

## Milestone 4 — Topocentric look angles (azimuth, elevation, range)

Compute observer-relative geometry, the core of pointing and visibility.

### Deliverables

- `look_angles(tle, time, observer) -> LookAngles` producing azimuth, elevation, slant
  range, and range-rate, reusing the existing `ObserverLocation`.
- Internal topocentric (SEZ) transform documented alongside the existing coordinate
  conventions.

### Test plan

- A satellite placed directly above the observer yields elevation ≈ 90°.
- Azimuth and elevation for a reference pass match an external reference within a documented
  tolerance (target: a few degrees, dominated by TLE age).
- Range-rate sign convention is verified (negative while approaching, positive while
  receding).

### Stage gate

**Signed off** (Milestone 4 approved; ENU look angles, range-rate checks, and CLI output in-tree).

---

## Milestone 5 — Pass prediction

Find and characterize visible passes over an observer for a time window.

### Deliverables

- `predict_passes(tle, observer, window, min_elevation) -> Vec<Pass>`, where each `Pass`
  reports acquisition-of-signal (AOS), culmination (time and maximum elevation), and
  loss-of-signal (LOS) times plus the corresponding azimuths.
- A coarse-step horizon-crossing search with refinement of AOS/LOS and culmination times,
  honoring a configurable elevation mask.

### Test plan

- ISS passes over a known location and date agree with Heavens-Above / Skyfield: AOS/LOS
  within ~1 minute and maximum elevation within a few degrees.
- Edge cases: a window with no passes returns empty; a geostationary satellite returns
  either a persistent or absent pass as appropriate for the observer.
- Determinism: repeated runs over the same inputs produce identical results.

### Stage gate

**Signed off** (Milestone 5 approved; `predict_passes`, azimuths at AOS/LOS, GEO shortcut, CLI, and tests in-tree).

---

## Milestone 6 — Ground track generation

Produce sampled sub-satellite tracks for visualization and analysis.

### Deliverables

- `ground_track(tle, window_start, window_end, step) -> Vec<GroundTrackSample>` sampling
  sub-satellite points over `[window_start, window_end)` (end exclusive), each row carrying
  UTC time plus `Subpoint` (latitude, longitude, ellipsoidal altitude in km).
- Output serialization: `ground_track_to_csv` and `ground_track_to_json` for downstream
  plotting tools.

### Test plan

- The track's repeat interval matches the orbital period derived from the mean motion.
- Per-orbit westward longitude regression matches the expected value for the orbit.
- Track continuity is verified (no discontinuities other than the ±180° longitude wrap).

### Stage gate

Track geometry matches expected orbital behavior; serialization formats validated.
**Signed off** (Milestone 6 approved; `ground_track`, CSV/JSON, CLI flags, and tests in-tree).

---

## Milestone 7 — CLI surface and user experience

Expose the library through a coherent, well-formatted command interface.

### Deliverables

- `track` **modes** (`--mode`): `all` (default), `tle`, `state`, `subpoint`, `look`, `passes`
  (requires `--predict-passes-hours` > 0), `ground` (requires `--ground-track-hours` > 0).
- **Output format** (`--format`): `human` (default) or `json` — a single pretty-printed JSON
  document for scripting (`tle`, `observer`, `state`, `subpoint`, `look_angles`, optional
  `passes` / `ground_track`, and pass/ground metadata fields when applicable).
- TLE sources: `--tle-file`, `--tle`, and (when built with **`--features network`**) **`--tle-url`**
  (exclusive); URL fetch still returns a clear “not implemented yet” error until HTTP TLE
  retrieval is implemented behind the same feature.
- Human output remains section-oriented; `--ground-track-json` still selects CSV vs. raw JSON
  array for the ground-track block when `--format human`.

### Test plan

- Integration tests exercise each mode end-to-end and assert formatted output (including
  structural checks on JSON where applicable).
- Error-message tests cover missing/invalid TLE sources and out-of-range arguments.

### Stage gate

All CLI modes function end-to-end with correct formatting and error handling.
**Signed off** (Milestone 7 approved; `track` modes, JSON output, TLE source rules, and integration tests in-tree).

---

## Milestone 8 — Teaching layer: transparent SGP4 and documentation

Deliver the educational artifact that distinguishes the project.

### Deliverables

- **`docs/sgp4.md`** — near-Earth SGP4 / TLE narrative, Kepler’s-law link to mean motion,
  conceptual pipeline, references (Spacetrack #3, Vallado 2006), and how Ephemerust layers
  `sgp4` + `satellite` + `sgp4_teaching`.
- **`sgp4_teaching` module** — WGS-72 μ, mean-motion → semi-major axis, linear mean-anomaly
  advance, two-body state via `orbital::elements_to_state_vector`; compared to the `sgp4`
  oracle in unit tests with **documented loose** position tolerances; `teaching_supported` gate
  for low mean motion / non-LEO band.

### Test plan

- The teaching implementation is validated against the production `sgp4` crate (used as an
  oracle) across the near-Earth verification TLEs, within a documented tolerance.
- Documented and tested divergence boundaries (e.g. deep-space cases the teaching path does
  not cover).

### Stage gate

Teaching implementation tracks the production engine within tolerance; documentation is
complete and accurate. **Signed off** (Milestone 8 approved; `docs/sgp4.md`, `sgp4_teaching`,
and regression tests in-tree).

---

## Milestone 9 — Library polish and publication readiness

Prepare the crate for external consumption.

### Deliverables

- Public API review and rustdoc coverage with runnable examples (crate-level MSRV/feature
  table; `#![deny(rustdoc::broken_intra_doc_links)]`; existing per-item examples retained).
- Feature flags: **`network`** gates `--tle-url` on the `track` command (stub only until HTTP
  fetch is implemented); MSRV documented in `Cargo.toml` and `readme.md`.
- Updated `readme.md`, `CHANGELOG.md`, architecture/test-count tables, and this plan
  (Milestones 8–9 signed off; first crates.io release **0.4.0**).
- Example program: `examples/track_subpoint.rs` (`cargo build --examples`).

### Test plan

- Doctests and example programs compile and pass (`cargo test --doc`, `cargo build --examples`).
- `cargo clippy` is clean; `cargo doc` builds without warnings.
- The full suite (unit + integration + validation + doctests) passes; run
  `cargo test --features network` occasionally to cover `--tle-url` integration tests.

### Stage gate

API is documented and stable; tooling is clean; a publication decision is recorded.
**Signed off** (Milestone 9 approved; `network` feature + `examples/track_subpoint`, rustdoc
MSRV/feature table, `cargo clippy` / `cargo doc` clean; published to crates.io as **0.4.0**
(see [`readme.md`](../readme.md) and [`CHANGELOG.md`](../CHANGELOG.md).)

---

## Cross-cutting test strategy

The plan reuses the project's existing testing conventions (tests co-located in
`#[cfg(test)]` modules; integration tests for end-to-end flows; validation tests against
authoritative sources; doctests to keep examples correct).

### Test categories

| Category | Purpose |
|----------|---------|
| Unit | Per-function correctness, boundary conditions, input validation. |
| Integration | End-to-end CLI and library flows (TLE → passes / ground track). |
| Validation | Comparison against external references (Vallado verification sets, Skyfield, Heavens-Above). |
| Regression | Lock in fixed behavior, including the teaching-vs-production SGP4 comparison. |
| Doctest | Keep documentation examples compiling and correct. |

### Reference sources

- **Vallado SGP4 verification TLEs and expected state vectors** — primary numerical oracle
  for propagation (Milestone 2).
- **The `sgp4` crate** — oracle for the teaching implementation (Milestone 8).
- **Skyfield / Heavens-Above** — external references for sub-satellite points, look angles,
  and pass times (Milestones 3–5).
- **CelesTrak / Space-Track** — sources of current TLE data.

### Error budget (targets, to be confirmed per milestone)

| Quantity | Target tolerance | Dominant error source |
|----------|------------------|------------------------|
| Propagated TEME state (default WGS84+IAU) | ~tens of m vs Vallado/AFSPC reference | numerical model choice |
| Propagated TEME state (`--afspc`) | sub-metre vs Vallado verification set | numerical |
| Sub-satellite point | tens of km | TLE age, omitted precession/nutation |
| Look-angle azimuth/elevation | a few degrees | TLE age |
| Pass AOS/LOS time | ~1 minute | search refinement, TLE age |
| Geodetic round-trip | ~1 mm | floating point |

### Network-dependent tests

Tests that fetch live TLE data are placed behind a feature flag (or marked ignored by
default) so the default `cargo test` run remains offline, fast, and deterministic.

### Running the suite

```bash
cargo test                 # full suite (unit + integration + doctests)
cargo test --lib           # unit tests only (fast)
cargo test --doc           # doctests only
cargo clippy               # lint
```

---

## Future work

Items below were explicitly deferred, partially met with in-tree substitutes, or moved out of
M0–M9 scope. They are **not** blockers for the completed satellite-tracking milestone set.

| Item | Origin | Status / next step |
|------|--------|-------------------|
| **HTTP TLE fetch (`--tle-url`)** | M1 (deferred), M7 deliverable | Stub behind `network` feature; implement per [`http_plan.md`](../http_plan.md) |
| **Space-Track authenticated fetch** | Out of scope for initial `--tle-url` | Separate compliance/API design if needed |
| **Automated Skyfield / Heavens-Above validation** | M3–M5 test plans | In-tree plausibility + Vallado vectors; manual Astroviewer check in [`readme.md`](../readme.md); optional pinned external regression tests |
| **M3 subpoint boundary tests** (equator, high inclination, ±180° lon) | M3 test plan | ✅ In-tree (`subpoint_*` tests in `satellite.rs`) |
| **M2 divergent-orbit error test** | M2 test plan | ✅ In-tree (`propagation_diverges_*` using `sgp4` regression TLEs) |
| **crates.io publication** | M9 | ✅ **0.4.0** pre-1.0 release |
| **Precession/nutation, refraction** | Accuracy posture | See [roadmap.md](roadmap.md) Phase 1 remaining work |
