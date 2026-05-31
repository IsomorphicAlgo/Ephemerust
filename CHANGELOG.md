# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/).

**Versioning policy:** the project remains in the `0.x.y` range while its public API is still
evolving. Per Cargo's SemVer rules, during `0.x` the *minor* version carries breaking changes
(`0.2.0` → `0.3.0`) and the *patch* version carries compatible ones (`0.2.0` → `0.2.1`).
A `1.0.0` release is reserved for a deliberately committed-stable API.

## [Unreleased]

### Added

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
  6 to 19. Renamed `Planet::from_str` to `Planet::from_name` to avoid confusion with the
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
  `description`, and documented the project's `0.x`-until-stable versioning policy. The
  version remains `0.2.0` (no rollback).
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
