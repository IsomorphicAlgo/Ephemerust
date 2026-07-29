# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/).

**Versioning policy:** the project remains in the `0.x.y` range while its public API is still
evolving. Per Cargo's SemVer rules, during `0.x` the *minor* version carries breaking changes
(`0.2.0` → `0.3.0`) and the *patch* version carries compatible ones (`0.2.0` → `0.2.1`).
A `1.0.0` release is reserved for a deliberately committed-stable API.

## [Unreleased]

### Changed

- **crates.io discoverability**: added the `aerospace` and `aerospace::simulation` categories
  (previously only `science` + `command-line-utilities`) and swapped the `orbital-mechanics`
  keyword — already indexed via the crate description — for `tle`. Metadata-only; takes effect
  with the next publish.

## [0.7.0] - 2026-07-29

"Physics and freshness" release: the two highest-value gaps identified for downstream
consumers (notably [Chronus Gateway](https://github.com/IsomorphicAlgo/chronus-gateway))
are closed. Satellites now know whether they are in the Earth's shadow, and element sets
can be fetched live from CelesTrak-style HTTPS sources.

### Added

- **`eclipse` module — a real Earth-shadow model.** Conical umbra/penumbra geometry (the
  standard apparent-disk-overlap test, Vallado §5.3), not a cylindrical approximation:
  - [`ShadowState`] (`Sunlit` / `Penumbra` / `Umbra`, ordered by shadow depth,
    `Display` + `Serialize`) and the pure, unit-testable classifier
    `shadow_state_from_vectors` (geocentric km vectors in, state out).
  - `Propagator::shadow_state(time)` and the free function `eclipse::shadow_state` for
    per-frame checks — **341 ns** on a prebuilt `Propagator` (criterion,
    `shadow_state_reused`), of which ~246 ns is the SGP4 step itself.
  - `eclipse::shadow_transitions(prop, start, end, step)` — entry/exit search with
    bisection refinement to ~1 ms. A coarse scan step that jumps straight from sunlit to
    umbra still yields both boundary crossings, so the seconds-long LEO penumbra ramp is
    never skipped.
  - `celestial::sun_vector_km` — geocentric equatorial Sun **position vector** (km),
    sharing one solar-elements series with the existing RA/Dec model, so direction and
    distance can never disagree.
- **`--tle-url` is implemented** (was a stub since 0.4.0). With `--features network`, the
  new `net` module fetches element-set text over HTTPS via a **bounded client** (`ureq` +
  rustls): 10 s connect / 30 s overall timeouts, 2 MiB response cap, ≤ 5 redirects,
  descriptive `User-Agent`, **no retries** — per the CelesTrak operational requirements
  recorded in `http_plan.md`. Plain HTTP is allowed only toward loopback (offline tests);
  `file:` and remote `http:` are refused. Default builds still pull **zero** HTTP/TLS deps.
- **`satellite::select_tle`** — selects one object from a multi-TLE bulletin (the normal
  shape of CelesTrak responses) by case-insensitive name substring or exact NORAD catalog
  number; errors list the available objects. Only the selected entry is fully validated,
  so an unrelated corrupt entry cannot block selection. Exposed on the CLI as
  **`--tle-name`**, which works with `--tle-file`, `--tle`, and `--tle-url` alike.
- **Offline network test suite** (`tests/network_fetch.rs`, runs under
  `cargo test --features network`): a loopback HTTP fixture server covers the fetch +
  select path, redirect following, 404-without-retry (asserted by request count), the
  size cap, and the HTTPS-only policy — deterministic and network-free, per the test
  strategy in `http_plan.md` §4.

### Changed

- The `network` feature now carries real dependencies (`ureq`/rustls) and a working
  fetch path; the "not implemented" stub error is gone.
- Multi-object input to the `track` command (file, inline, or URL) now produces a
  guided "select one by name or catalog number" error listing the objects, instead of a
  generic line-count parse failure. Single-object input keeps the exact previous
  behavior and diagnostics.

[`ShadowState`]: https://docs.rs/ephemerust/latest/ephemerust/eclipse/enum.ShadowState.html

## [0.6.0] - 2026-07-28

"Zero-cost abstractions" release: performance work that doubles as Rust teaching
material. Every change below is documented as a lesson in the new
[`docs/rust-idioms.md`](docs/rust-idioms.md) chapter, and every performance claim is
backed by a criterion benchmark (`cargo bench`, sources in `benches/core_operations.rs`).

### Measured performance (before → after, criterion, same machine)

| Benchmark | 0.5.0 | 0.6.0 | Change |
|-----------|-------|-------|--------|
| `planet_position_mars` (full VSOP87 pipeline) | 1.01 µs | 426 ns | **−58%** |
| `ground_track_90min_60s` (90 samples, one ISS orbit) | 95.1 µs | 38.1 µs | **−59%** |
| `propagate_single` (one-shot, includes SGP4 init) | 897 ns | 890 ns | unchanged |
| `propagate_reused` (step on a prebuilt `Propagator`) | — | 246 ns | new |
| `tle_parse` | 1.39 µs | 1.39 µs | unchanged |

The `propagate_single` vs `propagate_reused` gap shows that ~72% of a one-shot
propagation call was initialization — the cost `Propagator` lets callers pay once.

### Added

- **`satellite::Propagator`** — a reusable, initialized SGP4 propagator: construction
  (`Propagator::new` / `with_model`) parses the element set and derives the SGP4
  constants **once**; the borrowing methods `propagate`, `subpoint`, and `look_angles`
  are then cheap to call in loops. `ground_track` and `predict_passes` now use one
  internally (initialization once per call instead of once per sample — the source of
  the −59% ground-track improvement). The one-shot free functions remain as conveniences
  and delegate to it.
- **Standard trait implementations** — `Display` and `FromStr` for `Planet`
  (`"mars".parse::<Planet>()`, `println!("{planet}")`) and `FromStr` for `Tle`
  (`text.parse::<Tle>()`, preserving the educational `TleError` diagnostics). The CLI's
  object parsing now routes through the `Planet` impl.
- **Criterion benchmarks** — `benches/core_operations.rs` covering TLE parsing, VSOP87
  planet position, one-shot vs reused propagation, and ground-track sampling; run with
  `cargo bench`.
- **`docs/rust-idioms.md`** — a teaching chapter mapping each Rust strength (ownership
  as API design, `static` data + `const fn`, standard traits, structured errors,
  doctests, benchmarking) to the exact place it appears in this codebase.

### Changed

- **VSOP87 coefficient tables are now `static`** — `Vsop87Series` fields changed from
  `Vec<Vsop87Term>` / `Option<Vec<Vsop87Term>>` to `&'static [Vsop87Term]` (an empty
  slice means an unused sub-series), and the per-planet tables are `static` items built
  with `const fn` constructors. Planet-position calculations no longer allocate at all
  (the source of the −58% improvement). **Breaking** for code that constructed
  `Vsop87Series`/`PlanetVsop87Data` directly; the calculation API is unchanged.
- `get_planet_vsop87_data` (crate-internal) returns `Option<&'static PlanetVsop87Data>`
  instead of building an owned copy.

## [0.5.0] - 2026-07-28

Modernization release: the crate now targets **Rust edition 2024** (previously 2021).
No public API changes; the version bump is precautionary given the edition and major
dependency updates. MSRV remains **1.88**.

### Changed

- **Rust edition 2021 → 2024** — migrated with `cargo fix --edition`; the only source
  change required was in CLI logging setup, where `std::env::set_var` (now `unsafe` in
  edition 2024 because mutating the environment is not thread-safe) was replaced with a
  direct `env_logger::Builder` configuration. A user-supplied `RUST_LOG` now takes
  precedence over the `--verbose` flag's default instead of being overwritten.
- **Dependency updates** — `thiserror` 1.0 → 2.0, `env_logger` 0.10 → 0.11, and
  `criterion` (dev) 0.5 → 0.7. No code changes were required.
- **Clippy cleanup** — replaced two `is_some()` + `unwrap()` patterns in the VSOP87
  debug logging (`src/planets.rs`) with idiomatic `if let` bindings
  (`clippy::unnecessary_unwrap` on current toolchains).

## [0.4.0] - 2026-06-22

First [crates.io](https://crates.io/crates/ephemerust) release: teaching-grade astronomy,
orbital-mechanics, and satellite-tracking library + `ephemerust` CLI (`cargo add ephemerust`,
`cargo install ephemerust`).

### Added

- **`PropagationModel` and `--afspc`** — library functions `propagate_with_model`,
  `subpoint_with_model`, `look_angles_with_model`, `predict_passes_with_model`, and
  `ground_track_with_model` accept [`PropagationModel::AfspcCompatibility`] for WGS72 +
  legacy AFSPC sidereal/epoch (Vallado reference). The `track` command adds `--afspc`; JSON
  output includes `propagation_model`.
- **Satellite-tracking plan cleanup** — milestones M0–M9 marked complete; stale “Await sign-off”
  gates removed; deferred work consolidated under **Future work** in
  [`docs/satellite-tracking-plan.md`](docs/satellite-tracking-plan.md).
- **Documentation** — corrected WGS84 default propagation vs optional AFSPC path in the plan,
  [`docs/accuracy-and-limits.md`](docs/accuracy-and-limits.md), and module rustdoc (replacing
  the outdated “WGS72 propagator vs WGS84 geodetic” wording).
- **M2 / M3 regression tests** — `propagation_diverges_*` (official `sgp4` decay/divergence
  TLEs) and `subpoint_*` boundary tests (equator, polar latitude, ±180° longitude wrap).

- **Library polish (Milestone 9)** — Cargo feature `network` gates the `track --tle-url` CLI
  flag (stub handler unchanged); default integration tests assert clap rejects the flag when
  disabled. `examples/track_subpoint.rs` demonstrates `Tle` + `subpoint` from the library.
  Crate rustdoc adds MSRV/feature notes, `#![deny(rustdoc::broken_intra_doc_links)]`, and the
  satellite-tracking plan records milestones **8–9** signed off with a crates.io publication
  deferral for `0.x`.
- **SGP4 teaching layer (Milestone 8)** — new `docs/sgp4.md` (near-Earth SGP4 narrative,
  references, Ephemerust layering) and [`sgp4_teaching`](src/sgp4_teaching.rs): WGS-72
  mean-motion → **a**, linear mean-anomaly advance, two-body state vs `sgp4` oracle checks.
  Not used by production propagation. See [docs/sgp4.md](docs/sgp4.md).
- **Track CLI (Milestone 7)** — `track` supports `--mode` (`all`, `tle`, `state`, `subpoint`,
  `look`, `passes`, `ground`), `--format human|json` for a unified JSON document, exclusive
  TLE inputs (`--tle-file`, `--tle`, optional `--tle-url` with `--features network`), and validation for pass/ground
  modes. Library: `Serialize` on `TemeState`, `LookAngles`, and `Pass` for machine-readable
  output. See [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **Ground track (Milestone 6)** — `ground_track` samples `[window_start, window_end)` every
  `step` into `GroundTrackSample` (UTC + `Subpoint`); `ground_track_to_csv` and
  `ground_track_to_json` for plotting pipelines. `track` adds `--ground-track-hours`,
  `--ground-track-step-sec`, and `--ground-track-json`. See
  [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **Pass prediction (Milestone 5)** — `predict_passes` finds passes over `[window_start,
  window_end)` with a coarse elevation ladder and bisection-refined AOS/LOS; culmination via
  ternary search; [`Pass`] now includes azimuths at AOS/LOS. GEO-like element sets use a
  simplified all-or-nothing visibility. `track` accepts `--predict-passes-hours` and
  `--pass-min-elevation-deg`. See [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **Look angles (Milestone 4)** — `look_angles(tle, time, observer)` produces azimuth
  (clockwise from true north), elevation, slant range, and range rate via an **ENU** (east–
  north–up) rotation from the observer–satellite ECEF vector; satellite velocity uses the same
  TEME→ECEF rotation as position plus **ω** × **r**, and the observer station includes **ω** ×
  **r_obs**. `track` prints look angles at epoch with Everett, WA as the default observer
  (`--latitude` / `--longitude` override). See [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **Frame conversions & subpoint (Milestone 3)** — `teme_to_ecef` rotates TEME position into
  `Ecef` using the same GMST Z-rotation as ECI→ECEF; WGS84 `ecef_to_geodetic_wgs84` / `Geodetic`
  and `ecef_to_geodetic` → `Subpoint`; `subpoint` chains propagate → TEME→ECEF→geodetic.
  Bowring latitude with equator/pole/longitude tests; `track` prints the sub-satellite point at
  epoch. See [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **Documentation alignment** — added thorough Rustdoc to every public item across `lib.rs`,
  `time.rs`, `coordinates.rs`, `celestial.rs`, `orbital.rs`, `planets.rs`, and `satellite.rs`,
  each with a runnable example and the physical reasoning behind it (why SGP4 output is in the
  TEME frame, why sidereal time bridges Earth-fixed and inertial frames, why the horizon
  corrections differ for point sources versus the Sun/Moon, and so on). Added a crate-level
  overview positioning Ephemerust as a teaching-grade wrapper between the `sgp4` engine and
  full mission toolkits, enabled `#![warn(missing_docs)]` to keep the public surface
  documented, and made `cargo clippy --all-targets` and `cargo doc` clean. Doctests grew from
  6 to 20. Renamed `Planet::from_str` to `Planet::from_name` to avoid confusion with the
  `std::str::FromStr` trait.
- **Educational error handling** — introduced a structured `satellite::TleError` enum whose
  every variant explains *what was expected*, *what was found*, and *the underlying rule* of
  the fixed-column TLE format (wrong line count, non-ASCII, short line, wrong line number,
  checksum-not-a-digit, checksum mismatch, catalog mismatch, unparseable field with its named
  column range, and epoch problems). `TleError::hint()` (exposed via `AstroError::hint()`)
  returns a corrective next step. `TleError` folds into `AstroError` via
  `#[error(transparent)] Tle(#[from] TleError)`, so callers can match precise variants. The
  CLI now renders errors as `Error:`/`Hint:` lines on stderr with a non-zero exit code instead
  of the default `Debug` rendering. Added a `tests/cli_track.rs` integration test covering the
  malformed-TLE path end to end.
- **crates.io preparation** — completed the package manifest with `documentation`, `readme`,
  and an explicit `rust-version` (MSRV **1.88**, set by the `sgp4` dependency), refined the
  `description`, and documented the project's `0.x`-until-stable versioning policy.
- **Propagation wrapper (Milestone 2)** — `satellite::propagate(tle, time)` wraps the `sgp4`
  engine, converting a UTC datetime to the engine's minutes-since-epoch representation and
  returning the satellite's `TemeState` (TEME-frame position in km and velocity in km/s).
  Parse failures, invalid epoch constants, unrepresentable times (nanosecond overflow), and
  propagation divergence are all mapped to actionable `SatelliteError`s. The `track` command
  now also reports the TEME state at the element-set epoch. Validated against the published
  Vallado verification vector for satellite 00005 (within the documented WGS84/IAU-vs-AFSPC
  model tolerance), with a cross-check that the wrapper's UTC→minutes path matches direct
  engine calls across several offsets. See [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **TLE ingestion & parsing (Milestone 1)** — `Tle` now parses and validates standard 2- and
  3-line element sets, exposing the catalog number, classification, international designator,
  epoch (UTC), and the full orbital element set as typed fields. Validation covers ASCII/line
  length, line numbers, matching catalog numbers, and modulo-10 checksums, each with
  actionable error messages. TLEs can be read from a file (`Tle::from_file`) or an inline
  string (`Tle::parse`). The `track` command now loads a TLE (`--tle-file` / `--tle`) and
  prints a parsed summary. A validation test cross-checks every parsed field against the
  `sgp4` crate's own parser. See [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **Satellite-tracking foundation (Milestone 0)** — added the `sgp4` crate dependency and a
  new `satellite` module documenting the frame/unit conventions (TEME → ECEF → WGS84
  geodetic) and defining the public type stubs `Tle`, `TemeState`, `Subpoint`, `LookAngles`,
  and `Pass`. Added a `SatelliteError` variant to `AstroError` and a `track` CLI subcommand
  stub. A smoke test propagates a canonical ISS element set through the `sgp4` engine and
  asserts a physically plausible low-Earth-orbit radius. See
  [docs/satellite-tracking-plan.md](docs/satellite-tracking-plan.md).
- **`orbital` command** now computes results instead of echoing inputs: orbital period,
  true anomaly, and the orbital-elements → state-vector conversion. New optional flags
  `--raan`, `--arg-periapsis`, `--mean-anomaly`, and `--mu` (defaulting to Earth's μ).
- **Planet positions for all eight planets** via truncated VSOP87D, including Earth's
  series (required for the geocentric conversion). Geocentric RA/Dec agrees with JPL
  Horizons to within a few arcminutes at J2000.0.
- **Planet rise/set times** — the `rise-set` command now supports planets in addition to the
  Sun and Moon, using a shared rise/set routine with a point-source horizon correction.

### Fixed

- **VSOP87 time argument** now uses Julian *millennia* (`τ = (JD − 2451545)/365250`) instead
  of Julian centuries.
- **VSOP87 series scaling** — removed an erroneous `/10⁸` factor; VSOP87D spherical
  coefficients are stored directly in radians (L, B) and AU (R). Together with the time-unit
  fix, this makes planet positions correct (previously they collapsed toward zero).

### Planned next steps

See [docs/roadmap.md](docs/roadmap.md):

- Extend the VSOP87 series with more terms toward arcsecond accuracy.
- Begin Part 2: the space-weather REST service.

## [0.2.0] - 2024-12

Second development phase. Added Earth-centered coordinate transforms and the foundation
of a planetary ephemeris.

### Added

- **ECEF ↔ ECI coordinate transformations** using a GMST-based Z-axis rotation matrix,
  in both directions, with input validation (NaN/infinity), GMST normalization, and
  round-trip accuracy within ~1 mm at Earth scale.
  - Extended the `convert` command with `ecef`/`eci` source/target options and an
    optional `--gmst` flag (auto-calculated from the current time when omitted).
- **VSOP87 planetary position foundation** (`planets.rs`):
  - `Planet` enum (Mercury–Neptune), `Vsop87Term`/`Vsop87Series`/`PlanetVsop87Data`
    structures stored as compile-time constants.
  - VSOP87 series evaluation (`A·cos(B + C·t)` summed across L0–L5, B0–B4, R0–R4).
  - Full coordinate pipeline: heliocentric ecliptic (L, B, R) → equatorial (rotation by
    obliquity) → geocentric (subtract Earth) → RA/Dec.
  - `position` command extended to accept planet names (case-insensitive).
- Convenience re-exports in `lib.rs` for common types and functions.
- Educational/technical documentation for the above (now in [docs/](docs/)).

### Changed

- Enhanced error handling with contextual, actionable messages.
- Multi-level logging (Debug/Info/Warn/Error) across coordinate and planet calculations.
- Performance optimizations (pre-computed time powers for VSOP87 series).

### Test coverage

- 71 unit tests + 5 doctests, all passing.
- 15 tests for ECEF/ECI transformations; 26 tests for VSOP87 / planet positions,
  including performance benchmarks (< 1 ms per planet calculation).

### Known limitations

- Earth's VSOP87 data is a placeholder; only Mercury has truncated coefficients, so
  geocentric planet positions are not yet meaningful.
- Planet rise/set times are not implemented (the command returns an error).
- No precession/nutation (J2000.0 assumed), no light-time correction, no atmospheric
  refraction.
- The CLI `orbital` command is a stub that prints its inputs; the underlying library
  functions are implemented and tested.

## [0.1.0] - Initial phase

First development phase: a command-line astronomy toolset covering the core time,
coordinate, and orbital calculations.

### Added

- **Time systems** (`time.rs`): Julian Date, Greenwich Mean Sidereal Time (GMST),
  Local Sidereal Time (LST).
- **Solar position**: Sun RA/Dec via mean anomaly + equation of center.
- **Lunar position**: Moon RA/Dec via perturbation theory (periodic terms).
- **Rise/set times** for the Sun and Moon at any location.
- **Coordinate conversions**: RA/Dec ↔ Alt/Az.
- **Orbital mechanics** (`orbital.rs`): Kepler's equation solver, orbital period
  (Kepler's Third Law), orbital elements → state vectors.
- CLI scaffolding (clap subcommands), logging, and error-handling infrastructure.

### Development notes

This phase was originally tracked as Phases 1–4 (basic structure, core astronomy,
coordinate conversions, lunar & orbital mechanics), each with comprehensive unit tests.
The detailed per-step implementation log that previously lived in the README has been
condensed into the entries above.
