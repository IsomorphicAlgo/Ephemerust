# Ephemerust (*ephemeris - Rust*)

Ephemerust implements the time systems, coordinate frames, and orbital calculations

used in space-mission operations and satellite control. It serves a dual purpose: a working

astronomy toolset, and a study project in Rust and applied astrophysics.

## Acknowledgments

Ephemerust stands on the shoulders of the open-source astrodynamics community: it is built
*around* established tools rather than reinventing them. Most directly, it wraps the
[`sgp4`](https://crates.io/crates/sgp4) crate (the validated SGP4/SDP4 propagation engine) and
relies on [`chrono`](https://crates.io/crates/chrono) for time handling and
[`clap`](https://crates.io/crates/clap) for its command-line interface. Sincere thanks to the
maintainers of those crates, and to the authors of the foundational references — Jean Meeus,
David Vallado, and the VSOP87 planetary theory of Bretagnon and Francou — without whose work
this project would not be possible. Full attributions are listed under
[References](#references).

## Overview
Ephemerust is an accessible, **teaching-grade** astrodynamics library and CLI for Rust — in
spirit, the Rust counterpart to Python's [Skyfield](https://rhodesmill.org/skyfield/). It
deliberately occupies the middle ground between raw numerical engines (such as the
[sgp4](https://crates.io/crates/sgp4) crate) and ultra-high-fidelity mission toolkits (such as
[nyx-space](https://crates.io/crates/nyx-space)): it provides ergonomic, thoroughly documented
wrappers around the messy parts — time systems, coordinate-frame conversions, and planetary
theory — so that tracking a satellite or locating a planet Can be an exploratory process. 
I personally believe that learning is significantly boosted when you have the tool(s) needed
to play and hands on dive into your curiousity. Every public item documents the 
physical reasoning alongside the code, and errors are written to hopefully teach (see the structured TLE diagnostics).

The current development focus extends the project into **satellite tracking and pass
prediction** — taking a Two-Line Element set (TLE) through SGP4 propagation to ground tracks,
observer look angles, visible-pass predictions, and sampled ground-track export (CSV/JSON). The work follows a **library-first**
design: the production propagator is provided by the validated
[sgp4](https://crates.io/crates/sgp4) crate, while this project supplies the conversion and
prediction layer that the raw propagator omits (TEME → ECEF → geodetic → topocentric →
passes → ground-track sampling), documented to the same standard as the rest of the codebase. The staged plan lives in the [satellite-tracking plan](docs/satellite-tracking-plan.md); **milestones M0–M9** there (through library polish and the SGP4 teaching layer) are **signed off**.

The project is structured in two phases:

1. **Phase 1 — CLI tool** (the core of this repository): a command-line toolset for
  astronomical calculations **including** satellite tracking (see the [plan](docs/satellite-tracking-plan.md)).
2. **Phase 2 — data services**: API-based data access, building toward a standalone,
  self-hosted Rust service accessible remotely. See the [roadmap](docs/roadmap.md).

## Status at a glance


| Feature                                       | Status                                                                             |
| --------------------------------------------- | ---------------------------------------------------------------------------------- |
| Julian Date / sidereal time                   | ✅ working                                                                          |
| Sun & Moon position                           | ✅ working                                                                          |
| Sun & Moon rise/set                           | ✅ working                                                                          |
| RA/Dec ↔ Alt/Az                               | ✅ working                                                                          |
| ECEF ↔ ECI                                    | ✅ working                                                                          |
| Orbital mechanics (`orbital` command)         | ✅ working (period, true anomaly, state vectors)                                    |
| Planet positions (VSOP87)                     | ✅ working (truncated VSOP87D, ~arcminute accuracy)                                 |
| Planet rise/set                               | ✅ working                                                                          |
| Satellite TLE → TEME → subpoint (WGS84)      | ✅ working (see [plan](docs/satellite-tracking-plan.md))                            |
| Satellite look angles (TLE → observer)       | ✅ working (ENU / az–el–range–range-rate; see [plan](docs/satellite-tracking-plan.md)) |
| Satellite pass prediction (`predict_passes`) | ✅ working (coarse scan + bisection; see [plan](docs/satellite-tracking-plan.md))        |
| Satellite ground track (sampled subpoint + CSV/JSON) | ✅ working (`ground_track`; see [plan](docs/satellite-tracking-plan.md)) |
| Satellite plan (M0–M9) — library polish & teaching | ✅ **signed off** ([plan](docs/satellite-tracking-plan.md): track CLI, passes, ground track, JSON, `network` stub, `sgp4_teaching`, docs) |

## Install & build

From [crates.io](https://crates.io/crates/ephemerust) (release **0.4.0**):

```bash
cargo install ephemerust          # CLI binary on your PATH
cargo add ephemerust              # library dependency in your project
```

From this repository:

```bash
cargo build            # build
cargo test             # run the test suite (117 unit + 8 CLI integration + 20 doctests by default)
cargo test --features network   # also runs `--tle-url` CLI integration tests
cargo build --examples # compile `examples/` (library-only demos)
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

### `track` — satellite tracking from a TLE

```bash
# From an inline Two-Line Element set
cargo run -- track --tle "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992
2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008"

# Or from a file (2- or 3-line element set)
cargo run -- track --tle-file iss.tle
```

Optional: `--predict-passes-hours <n>` with `--pass-min-elevation-deg` (default 10) lists
passes for the **n** hours after the element-set epoch from the default (or supplied) observer.

`--mode` selects **tle**, **state**, **subpoint**, **look**, **passes** (needs `--predict-passes-hours` > 0), **ground** (needs `--ground-track-hours` > 0), or **all** (default). `--format json` prints one JSON object (TLE summary, observer, state, subpoint, look angles, optional passes and ground-track samples). Add **`--afspc`** for WGS72 + legacy AFSPC propagation (Vallado / NORAD reference compatibility; default is WGS84 + IAU). The `--tle-url` flag is only available when the binary is built with **`--features network`**; it still returns a clear “not implemented” error until HTTP fetch exists. `--ground-track-json` still toggles CSV vs. JSON array for the ground-track section in **human** format.

## Examples (library)

Runnable demos under [`examples/`](examples/) compile with `cargo build --examples` and do
not require the CLI:

```bash
cargo run --example track_subpoint
```

## Documentation

The full mathematics, conventions, and engineering details live in [docs/](docs/):

- [Time systems](docs/time-systems.md) — Julian Date, sidereal time
- [Celestial positions](docs/celestial-positions.md) — Sun & Moon models, rise/set
- [Coordinate systems](docs/coordinates.md) — RA/Dec ↔ Alt/Az, ECEF ↔ ECI, WGS84 geodetic, ENU look angles
- [Orbital mechanics](docs/orbital-mechanics.md) — Kepler's equation, period, state vectors
- [VSOP87 planetary theory](docs/vsop87.md) — series, data structures, conversion pipeline
- [SGP4 & TLE teaching notes](docs/sgp4.md) — SGP4 context and the `sgp4_teaching` module
- [Accuracy & limitations](docs/accuracy-and-limits.md)
- [Architecture](docs/architecture.md) — modules, errors, logging, testing
- [Roadmap](docs/roadmap.md) — Phase 2 (API service) and deployment plans
- [Network TLE fetch plan](http_plan.md) — `--tle-url`, CelesTrak HTTPS usage expectations, and acceptance criteria

## Versioning

Ephemerust follows [Semantic Versioning](https://semver.org/) and is intentionally in the
`0.x` range while its public API stabilizes. During `0.x`, the minor version marks breaking
changes and the patch version marks compatible ones; a `1.0.0` release is reserved for a
deliberately committed-stable API. The minimum supported Rust version (MSRV) is **1.88**
(see `Cargo.toml` `rust-version`). Optional **`network`** feature: enables the `track`
`--tle-url` flag (still a stub until HTTP TLE retrieval is implemented). Published on
[crates.io](https://crates.io/crates/ephemerust) as **`0.4.0`** (pre-1.0 API).

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.

## References

Ephemerust is built on, validated against, and inspired by the tools and sources below. Deep
gratitude to everyone who created and maintains them.

### Tools & libraries (Rust crates)

- [`sgp4`](https://crates.io/crates/sgp4) — SGP4/SDP4 satellite propagation engine; the core
this project wraps.
- [`chrono`](https://crates.io/crates/chrono) — date and time handling.
- [`clap`](https://crates.io/crates/clap) — command-line argument parsing.
- [`thiserror`](https://crates.io/crates/thiserror) and
[`anyhow`](https://crates.io/crates/anyhow) — structured error handling.
- [`log`](https://crates.io/crates/log) and
[`env_logger`](https://crates.io/crates/env_logger) — logging.
- [`serde_json`](https://crates.io/crates/serde_json) — JSON serialization for ground-track export.
- [`criterion`](https://crates.io/crates/criterion) — benchmarking (development dependency).

### Algorithms, theory & data

- **VSOP87 planetary theory** — Bretagnon, P. & Francou, G. (1988), *Planetary theories in
rectangular and spherical variables: VSOP87 solutions*, Astronomy & Astrophysics, 202, 309.
- **SGP4 & the Two-Line Element format** — Hoots, F. R. & Roehrich, R. L. (1980), *Spacetrack
Report No. 3*; and Vallado, D. A. et al. (2006), *Revisiting Spacetrack Report #3* (AIAA
2006-6753) — the source of the SGP4 verification vectors used in the test suite.
- **Astronomical algorithms** — Meeus, J. (1998), *Astronomical Algorithms* (2nd ed.),
Willmann-Bell.
- **Astrodynamics** — Vallado, D. A., *Fundamentals of Astrodynamics and Applications*.
- **IAU conventions** for coordinate frames and time scales.
- [JPL Horizons](https://ssd.jpl.nasa.gov/horizons/) — ephemeris data used to validate the
planet positions.
- [CelesTrak](https://celestrak.org/) (Dr. T. S. Kelso) — TLE format documentation and orbital
element data.

### Inspiration

- [Skyfield](https://rhodesmill.org/skyfield/) (Brandon Rhodes) — the Python library whose
ergonomic, accessible design Ephemerust aims to bring to the Rust ecosystem.

## External check: ISS vs [Astroviewer](https://www.astroviewer.net/iss/en/)

A quick **library** trial (not the `track` CLI, which prints state at the TLE epoch only)
compared Ephemerust to Astroviewer’s live readout for the same idea: current ISS subpoint,
speed, and altitude.

**Setup.** ISS two-line element set from CelesTrak
(`https://celestrak.org/NORAD/elements/gp.php?GROUP=stations&FORMAT=tle`, ISS ZARYA line
pair), epoch **2026-05-31 11:38:14 UTC**. Astroviewer listed **19:11:23 UTC** without a
calendar date; the run used **2026-05-31 19:11:23 UTC** so the instant lies on the same UTC
day as that TLE. Re-fetch the TLE and adjust the datetime if you repeat the check later.

**API.** `Tle::parse`, then `propagate` and `subpoint` at the chosen `chrono::DateTime<Utc>`
(see [`examples/track_subpoint.rs`](examples/track_subpoint.rs) for a minimal pattern at
epoch only).

| Quantity | Astroviewer | Ephemerust |
|----------|-------------|------------|
| Orbital speed | 4.753 mi/s (17 111 mph) | **4.753 mi/s** (17 111 mph), ‖**v**‖ in TEME |
| Altitude | 269 mi (~432.9 km) | **432.8 km** WGS84 ellipsoidal height (~269 mi) |
| Longitude | 173.67° E | **173.68° E** |
| Latitude | 32.55° S | **32.74° S** |

Speed and altitude matched within rounding; longitude agreed within **~0.01°**. Latitude
differed by **~0.2°** (~20 km along the ground track), which is still consistent with the
documented subpoint error budget (TLE age, omitted precession/nutation, simplified TEME→ECEF
bridge — see [Accuracy & limitations](docs/accuracy-and-limits.md)).

## License

MIT.
