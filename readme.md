# CLI Astro Calc

A command-line astronomy and orbital-mechanics calculator, written in Rust.

## Overview

CLI Astro Calc implements the time systems, coordinate frames, and orbital calculations
used in space-mission operations and satellite control. It serves a dual purpose: a working
astronomy toolset, and a study project in Rust and applied astrophysics.

The project is structured in two phases:

1. **Phase 1 — CLI tool** (the core of this repository): a command-line toolset for
   astronomical calculations.
2. **Phase 2 — data services**: API-based data access, building toward a standalone Rust
   service deployable on a home server rack and accessible remotely. See the
   [roadmap](docs/roadmap.md).

## Status at a glance

| Feature | Status |
|---------|--------|
| Julian Date / sidereal time | ✅ working |
| Sun & Moon position | ✅ working |
| Sun & Moon rise/set | ✅ working |
| RA/Dec ↔ Alt/Az | ✅ working |
| ECEF ↔ ECI | ✅ working |
| Orbital mechanics (`orbital` command) | ✅ working (period, true anomaly, state vectors) |
| Planet positions (VSOP87) | ✅ working (truncated VSOP87D, ~arcminute accuracy) |
| Planet rise/set | ✅ working |

## Install & build

```bash
cargo build            # build
cargo test             # run the test suite (75 unit + 5 doctests)
cargo run -- --help    # list all commands
```

## Commands

The default observer location (used by RA/Dec ↔ Alt/Az) is Everett, WA
(47.9088° N, 122.2503° W). Add `--verbose` to any command for detailed logging.

### `time` — Julian Date & sidereal time

```bash
cargo run -- time --date "2024-01-01" --time "18:30:45"
# JD:   2460311.271354
# GMST: 01:14:24
```

### `position` — RA/Dec of a celestial object

```bash
cargo run -- position --object jupiter --date "2000-01-01"
# RA:  01:35:28
# Dec: +08°35'39"
```

Objects: `sun`, `moon`, and planet names (`mercury` … `neptune`). Planet positions use
truncated VSOP87D and agree with JPL Horizons to within a few arcminutes.

### `rise-set` — rise/set times for a location

```bash
cargo run -- rise-set --object sun --latitude 47.6061 --longitude=-122.3328 --date "2024-12-25"
# Rise: 15:56:45 UTC
# Set:  00:22:44 UTC
```

Works for `sun`, `moon`, and any planet (e.g. `--object jupiter`).

### `convert` — coordinate system conversions

```bash
# Equatorial → horizontal (uses current time + default location)
cargo run -- convert --from ra-dec --to alt-az --coords "12.5,45.0"

# Earth-fixed → inertial (auto GMST, or pass --gmst <hours>)
cargo run -- convert --from ecef --to eci --coords "6378137.0,0.0,0.0"
```

Supported pairs: `ra-dec`↔`alt-az`, `ecef`↔`eci`. Formats: RA/Dec `hours,degrees`;
Alt/Az `altitude,azimuth` (deg); ECEF/ECI `x,y,z` (meters).

### `orbital` — orbital period & state vectors

```bash
cargo run -- orbital --semi-major 6778 --eccentricity 0.0001 --inclination 51.6
# Period:   5553.5 s (92.56 min)
# True anomaly: 0.0000°
# State vector (inertial frame):
#   Position [km]:   x=6777.322 y=0.000 z=0.000
#   Velocity [km/s]: vx=-0.000000 vy=4.763832 vz=6.010461
```

Optional flags: `--raan`, `--arg-periapsis`, `--mean-anomaly` (degrees), and `--mu`
(gravitational parameter in km³/s², default Earth).

## Documentation

The full mathematics, conventions, and engineering details live in [`docs/`](docs/):

- [Time systems](docs/time-systems.md) — Julian Date, sidereal time
- [Celestial positions](docs/celestial-positions.md) — Sun & Moon models, rise/set
- [Coordinate systems](docs/coordinates.md) — RA/Dec ↔ Alt/Az, ECEF ↔ ECI, frame conventions
- [Orbital mechanics](docs/orbital-mechanics.md) — Kepler's equation, period, state vectors
- [VSOP87 planetary theory](docs/vsop87.md) — series, data structures, conversion pipeline
- [Accuracy & limitations](docs/accuracy-and-limits.md)
- [Architecture](docs/architecture.md) — modules, errors, logging, testing
- [Roadmap](docs/roadmap.md) — Phase 2 (API service) and deployment plans

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.

## License

MIT.
