# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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
