# Satellite Tracking & Pass Prediction — Iterative Plan

This document defines the staged, test-driven plan for adding satellite tracking and pass
prediction to the project. It is written as an engineering plan with explicit stage gates:
each milestone is independently shippable, ships with its own test plan, and **requires
explicit sign-off before the next milestone begins**.

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
- The `sgp4` crate's gravity model is WGS72; geodetic conversion uses the WGS84 ellipsoid.
  The small resulting inconsistency is within the documented error budget and is noted in
  the accuracy tables.

## How to use this plan

- Milestones are executed in order. Each begins only after the previous milestone's stage
  gate has been signed off.
- A milestone is "done" when every item under **Deliverables** exists, every item under
  **Test plan** passes, and the **Stage gate** criteria are confirmed.
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

Dependency resolves and builds on the target (Windows/PowerShell) environment; the module
and error plumbing are in place; the smoke test passes. **Await sign-off.**

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
**Await sign-off.**

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

Propagated states match reference vectors within tolerance; error paths behave as
documented. **Await sign-off.**

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
**Await sign-off.**

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

Look angles match the external reference within tolerance; sign conventions verified.
**Await sign-off.**

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

Predicted passes match external references within tolerance; edge cases handled.
**Await sign-off.**

---

## Milestone 6 — Ground track generation

Produce sampled sub-satellite tracks for visualization and analysis.

### Deliverables

- `ground_track(tle, window, step) -> Vec<Subpoint>` sampling sub-satellite points over a
  time window.
- Output serialization to CSV and JSON for downstream plotting.

### Test plan

- The track's repeat interval matches the orbital period derived from the mean motion.
- Per-orbit westward longitude regression matches the expected value for the orbit.
- Track continuity is verified (no discontinuities other than the ±180° longitude wrap).

### Stage gate

Track geometry matches expected orbital behavior; serialization formats validated.
**Await sign-off.**

---

## Milestone 7 — CLI surface and user experience

Expose the library through a coherent, well-formatted command interface.

### Deliverables

- A `track` subcommand with modes for current position/subpoint, look angles, pass
  prediction, and ground track.
- TLE source flags: inline, file, and (placeholder for) network fetch.
- Output consistent with existing commands, plus an optional machine-readable JSON output.

### Test plan

- Integration tests exercise each mode end-to-end and assert formatted output.
- Snapshot tests cover the human-readable and JSON output formats.
- Error-message tests cover missing/invalid TLE sources and out-of-range arguments.

### Stage gate

All CLI modes function end-to-end with correct formatting and error handling.
**Await sign-off.**

---

## Milestone 8 — Teaching layer: transparent SGP4 and documentation

Deliver the educational artifact that distinguishes the project.

### Deliverables

- `docs/sgp4.md`: a step-by-step derivation and explanation of the near-Earth SGP4 model,
  in the established documentation style.
- An optional `sgp4_teaching` module implementing the near-Earth SGP4 path from first
  principles, clearly marked as educational and not the production engine.

### Test plan

- The teaching implementation is validated against the production `sgp4` crate (used as an
  oracle) across the near-Earth verification TLEs, within a documented tolerance.
- Documented and tested divergence boundaries (e.g. deep-space cases the teaching path does
  not cover).

### Stage gate

Teaching implementation tracks the production engine within tolerance; documentation is
complete and accurate. **Await sign-off.**

---

## Milestone 9 — Library polish and publication readiness

Prepare the crate for external consumption.

### Deliverables

- Public API review and rustdoc coverage with runnable examples.
- Feature flags (e.g. a `network` feature for TLE fetching) and a documented minimum
  supported Rust version.
- Updated `readme.md`, `CHANGELOG.md`, and architecture/test-count tables.

### Test plan

- Doctests and example programs compile and pass.
- `cargo clippy` is clean; `cargo doc` builds without warnings.
- The full suite (unit + integration + validation + doctests) passes.

### Stage gate

API is documented and stable; tooling is clean; a publication decision is recorded.
**Await sign-off.**

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
| Propagated TEME state (vs. verification set) | tight (matches reference engine) | numerical |
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
