# Ephemerust (*ephemeris + Rust*)

Where's the ISS right now? When does Jupiter rise tonight? How fast is that satellite closing
on your antenna? Ephemerust answers questions like these from a timestamp and a TLE. It's a
Rust library and CLI for the time systems, coordinate frames and orbital math behind satellite
operations, with the physics explained right next to the code.

## Summary

Ephemerust started as my first real Rust project. I wanted to learn the language by building
something that actually meant something to me, and space math turned out to be a great
teacher: it punishes sloppy units, rewards good types, and there's always a reference answer
to check yourself against. I'm a big believer that you learn fastest when you have a tool you
can poke at and follow your curiosity with, so that's what I set out to build.

It kept growing: planets via VSOP87, satellites via SGP4 (look angles, pass predictions,
ground tracks, Earth-shadow, live TLE fetch), and eventually it became the physics engine
behind my other project, [Chronus Gateway](https://github.com/IsomorphicAlgo/chronus-gateway).
With 1.0 I think it's polished enough to be useful to other people too, whether you want to
track a satellite from your backyard, learn how reference frames fit together, or need a
documented astrodynamics layer for your own Rust project.

**Pushing toward mission-ready.** Ephemerust began as a teaching-grade tool, and it still
explains the physics behind every function. But I'm trying to bring it as close to
mission-ready as an open-source project honestly can. Satellite look angles are tested against
an independent IAU 2000 reference (Skyfield) and agree to within a third of an arcminute,
75 m in range and 0.4 m/s in range rate. Every accuracy limit is written down, there's no
`unsafe` code, CI runs on the minimum supported Rust version as well as the latest, and the
API is stable under semver. Building those checks is how 1.0 caught and fixed a real
range-rate bug, which is exactly why they exist. It isn't flight software, and the TLEs
themselves limit real-world accuracy to around a kilometre, but the math between the TLE and
your antenna should be something you can trust. As of 1.1 it also handles Earth-orientation
data and the precise reference frames that operator ephemerides use, for when a TLE isn't good
enough.

## Acknowledgements

Ephemerust leans on a lot of other people's great work, and I'm grateful for all of it:

- **The [`sgp4`](https://crates.io/crates/sgp4) crate** does the actual orbit propagation.
  Ephemerust wraps it rather than reinventing it, and it's the reason the satellite side
  could be built on solid ground.
- **[Skyfield](https://rhodesmill.org/skyfield/) by Brandon Rhodes** is the inspiration for
  the whole project (an approachable astronomy library, but for Rust) and is now also the
  independent reference the satellite math is tested against.
- **The Rust ecosystem**: [`chrono`](https://crates.io/crates/chrono),
  [`serde`](https://crates.io/crates/serde), [`thiserror`](https://crates.io/crates/thiserror),
  [`clap`](https://crates.io/crates/clap), [`ureq`](https://crates.io/crates/ureq) and
  [`criterion`](https://crates.io/crates/criterion), plus the Rust community's books and docs,
  which carried me through learning the language in the first place.
- **The authors who wrote the math down**: Jean Meeus (*Astronomical Algorithms*), David
  Vallado (*Fundamentals of Astrodynamics and Applications* and the SGP4 verification
  vectors), Hoots & Roehrich (*Spacetrack Report No. 3*), and Bretagnon & Francou (VSOP87).
- **[JPL Horizons](https://ssd.jpl.nasa.gov/horizons/)** for the planet-position checks and
  **[CelesTrak](https://celestrak.org/) (Dr. T. S. Kelso)** for TLE data and documentation.

Full citations are under [References](#references).

## What's inside

Ephemerust is library-first: the CLI is a thin layer over the same public API that other
crates use. It sits between raw numerical engines (such as the `sgp4` crate, which emits TEME
state vectors and nothing else) and high-fidelity mission toolkits (such as
[nyx-space](https://crates.io/crates/nyx-space)), supplying the conversion and prediction
layer in between:

- **Time** — Julian Date, Greenwich and local sidereal time.
- **Coordinates** — RA/Dec ↔ Alt/Az, ECEF ↔ ECI, WGS84 geodetic, opt-in atmospheric refraction.
- **Reference frames** — Earth orientation (UT1−UTC, polar motion), leap seconds, and
  GCRF / EME2000 / TEME ↔ ITRF with IAU 2006 precession and IAU 2000B nutation, for precise
  ephemerides (e.g. CCSDS OEM) as well as TLEs.
- **Sun, Moon and planets** — positions and rise/set times (planets via truncated VSOP87D).
- **Orbital mechanics** — Kepler's equation, period, elements → state vectors.
- **Satellites** — TLE parsing with teaching-oriented error messages, SGP4 propagation,
  TEME → ECEF → geodetic → topocentric look angles (azimuth, elevation, range, range rate),
  pass prediction, ground tracks with CSV/JSON export, and a conical umbra/penumbra
  Earth-shadow model. A reusable `Propagator` initializes SGP4 once for tight per-frame loops.
- **Live TLE fetch** — optional `network` feature with a bounded HTTPS client.

Every public item documents the physical reasoning and conventions (frames, units, epochs)
alongside the code. The satellite work followed a staged
[satellite-tracking plan](docs/satellite-tracking-plan.md) (milestones M0–M9, all signed
off). Hosted space-weather and ephemeris services are scoped to the companion Rusty_Server
project, which uses Ephemerust as a dependency; see the [roadmap](docs/roadmap.md).

## Status at a glance

| Feature                                       | Status                                                                             |
| --------------------------------------------- | ---------------------------------------------------------------------------------- |
| Julian Date / sidereal time                   | ✅ working                                                                          |
| Sun & Moon position                           | ✅ working                                                                          |
| Sun & Moon rise/set                           | ✅ working                                                                          |
| RA/Dec ↔ Alt/Az                               | ✅ working                                                                          |
| ECEF ↔ ECI                                    | ✅ working                                                                          |
| Orbital mechanics (`orbital` command)         | ✅ working (period, true anomaly, state vectors)                                    |
| Planet positions (VSOP87)                     | ✅ working (truncated VSOP87D, within a few arcminutes of JPL Horizons)              |
| Planet rise/set                               | ✅ working                                                                          |
| Satellite TLE → TEME → subpoint (WGS84)      | ✅ working (see [plan](docs/satellite-tracking-plan.md))                            |
| Satellite look angles (TLE → observer)       | ✅ working (ENU / az–el–range–range-rate; see [plan](docs/satellite-tracking-plan.md)) |
| Satellite pass prediction (`predict_passes`) | ✅ working (coarse scan + bisection; see [plan](docs/satellite-tracking-plan.md))        |
| Satellite ground track (sampled subpoint + CSV/JSON) | ✅ working (`ground_track`; see [plan](docs/satellite-tracking-plan.md)) |
| Satellite plan (M0–M9) — library polish & teaching | ✅ **signed off** ([plan](docs/satellite-tracking-plan.md): track CLI, passes, ground track, JSON, `sgp4_teaching`, docs) |
| Earth-shadow (eclipse) model                 | ✅ working (conical umbra/penumbra, `eclipse::shadow_state` + entry/exit search; since **0.7.0**) |
| Live TLE fetch (`--tle-url`, CelesTrak)      | ✅ working with `--features network` (bounded HTTPS client + `--tle-name` selection; see [http_plan.md](http_plan.md)) |
| Atmospheric refraction (`refraction_deg`)    | ✅ new in **1.0.0** (Sæmundsson standard atmosphere, optional pressure/temperature) |
| Earth orientation & frames (`frames`)        | ✅ new in **1.1.0** (UT1−UTC, polar motion, leap seconds; GCRF/EME2000/TEME ↔ ITRF with IAU 2006/2000B, within 7 cm of ERFA) |
| Independent validation (Skyfield reference)  | ✅ look angles within 0.34′, range within 75 m, range rate within 0.41 m/s of an IAU 2000 reference ([accuracy](docs/accuracy-and-limits.md)) |

## Install & build

From [crates.io](https://crates.io/crates/ephemerust) (release **1.1.0**):

```bash
cargo install ephemerust          # CLI binary on your PATH
cargo add ephemerust              # library dependency in your project
cargo add ephemerust --no-default-features   # library only, without the CLI's clap/env_logger
```

From this repository:

```bash
cargo build            # build
cargo test             # run the test suite (unit, CLI integration, Skyfield + ERFA cross-checks, doctests)
cargo test --features network   # adds the offline `--tle-url` fetch tests (loopback fixture server)
cargo build --examples # compile `examples/` (library-only demos)
cargo run -- --help    # list all commands
```

## Commands

The default observer location is Everett, WA (47.9088° N, 122.2503° W). Add `--verbose` to
any command for detailed logging.

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
# Elements: a=6778 km, e=0.0001, i=51.6°, Ω=0°, ω=0°, M=0°
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

`track` reports state, subpoint and look angles at the **element-set epoch**. Set the
observer with `--latitude` / `--longitude` (defaults to Everett, WA).

Optional: `--predict-passes-hours <n>` with `--pass-min-elevation-deg` (default 10) lists
passes for the **n** hours after the element-set epoch, and `--ground-track-hours <n>` with
`--ground-track-step-sec` (default 60) samples the ground track over the same kind of window.

`--mode` selects **tle**, **state**, **subpoint**, **look**, **passes** (needs `--predict-passes-hours` > 0), **ground** (needs `--ground-track-hours` > 0), or **all** (default). `--format json` prints one JSON object (TLE summary, observer, state, subpoint, look angles, optional passes and ground-track samples). Add **`--afspc`** for WGS72 + legacy AFSPC propagation (Vallado / NORAD reference compatibility; default is WGS84 + IAU). `--ground-track-json` still toggles CSV vs. JSON array for the ground-track section in **human** format.

With **`--features network`**, **`--tle-url`** fetches element-set text live over HTTPS
(bounded client: timeouts, 2 MiB cap, no retries — please respect
[CelesTrak's usage guidance](http_plan.md); GP data refreshes only ~every 2 h). Bulletins
usually contain many objects, so pair it with **`--tle-name`** (case-insensitive name
substring, or exact NORAD catalog number). `--tle-name` also works with `--tle-file`
and `--tle`:

```bash
cargo run --features network -- track \
  --tle-url "https://celestrak.org/NORAD/elements/gp.php?GROUP=stations&FORMAT=tle" \
  --tle-name "ISS (ZARYA)" --mode subpoint
```

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
- [Satellite-tracking plan](docs/satellite-tracking-plan.md) — the staged M0–M9 build of the satellite pipeline
- [Accuracy & limitations](docs/accuracy-and-limits.md) — measured accuracy, the Skyfield cross-check, and the satellite error budget
- [Architecture](docs/architecture.md) — modules, errors, logging, testing
- [Rust idioms](docs/rust-idioms.md) — the Rust techniques behind the performance work, with benchmarks
- [Roadmap](docs/roadmap.md) — what's done, what's next, and how Ephemerust fits with Chronus Gateway and Rusty_Server
- [Network TLE fetch plan](http_plan.md) — `--tle-url`, CelesTrak HTTPS usage expectations, and acceptance criteria

## Versioning

Ephemerust follows [Semantic Versioning](https://semver.org/). From **1.0.0** the public API is
stable: breaking changes happen only in a new major version. Error enums, models and result
structs are `#[non_exhaustive]`, so minor releases can add variants and output fields without
breaking callers. Numerical results may *improve* in a minor or patch release when a
correction brings them closer to physical truth; every such change is listed in the
[changelog](CHANGELOG.md).

The crate targets **Rust edition 2024** with a minimum supported Rust version (MSRV) of
**1.89** (see `Cargo.toml` `rust-version`), matching
[Chronus Gateway](https://github.com/IsomorphicAlgo/chronus-gateway). The MSRV is raised only in
a minor release. CI builds on the MSRV and on the latest stable toolchain; for security fixes
in the compiler and standard library, build with an up-to-date stable (`rustup update stable`).

Cargo features: **`cli`** (default) builds the `ephemerust` binary; library users can turn it
off with `default-features = false`. **`network`** enables the `net` module and the `track`
`--tle-url` flag (bounded HTTPS fetch via `ureq`/rustls; builds without it carry no HTTP/TLS
dependencies).

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.

## References

### Rust crates

- [`sgp4`](https://crates.io/crates/sgp4) — SGP4/SDP4 satellite propagation engine; the core
  this project wraps.
- [`chrono`](https://crates.io/crates/chrono) — date and time handling.
- [`serde`](https://crates.io/crates/serde) and [`serde_json`](https://crates.io/crates/serde_json)
  — serialization for JSON output and ground-track export.
- [`thiserror`](https://crates.io/crates/thiserror) — structured error types.
- [`log`](https://crates.io/crates/log) — logging facade.
- [`clap`](https://crates.io/crates/clap) and [`env_logger`](https://crates.io/crates/env_logger)
  — command-line parsing and log output (`cli` feature).
- [`ureq`](https://crates.io/crates/ureq) with [`rustls`](https://crates.io/crates/rustls) —
  bounded HTTPS client for live TLE fetch (`network` feature).
- [`criterion`](https://crates.io/crates/criterion) — benchmarking (development dependency).

### Algorithms and theory

- **VSOP87 planetary theory** — Bretagnon, P. & Francou, G. (1988), *Planetary theories in
  rectangular and spherical variables: VSOP87 solutions*, Astronomy & Astrophysics, 202, 309.
- **SGP4 and the Two-Line Element format** — Hoots, F. R. & Roehrich, R. L. (1980),
  *Spacetrack Report No. 3*; and Vallado, D. A. et al. (2006), *Revisiting Spacetrack Report
  #3* (AIAA 2006-6753), the source of the SGP4 verification vectors used in the test suite.
- **Astronomical algorithms** — Meeus, J. (1998), *Astronomical Algorithms* (2nd ed.),
  Willmann-Bell; used for Julian Date, sidereal time (Example 12.a is a regression test),
  Sun/Moon models and atmospheric refraction (ch. 16).
- **Atmospheric refraction** — Sæmundsson, Þ. (1986), "Astronomical Refraction", *Sky and
  Telescope* 72, 70.
- **Precession, nutation and Earth orientation** — Capitaine, N., Wallace, P. T. & Chapront, J.
  (2003), IAU 2006 precession (A&A 412, 567); Hilton, J. L. et al. (2006), Fukushima–Williams
  angles; McCarthy, D. D. & Luzum, B. J. (2003), *An abridged model of the precession-nutation of
  the celestial pole* (IAU 2000B, Celest. Mech. 85, 37); Petit, G. & Luzum, B. (eds.) (2010),
  *IERS Conventions*, IERS Technical Note 36; IERS Bulletins A and C (Earth orientation, leap
  seconds).
- **Astrodynamics** — Vallado, D. A., *Fundamentals of Astrodynamics and Applications*; the
  TEME → ECEF bridge and the conical Earth-shadow model.
- **IAU conventions** for coordinate frames and time scales.

### Validation data and sources

- [Skyfield](https://rhodesmill.org/skyfield/) (Brandon Rhodes) — independent IAU 2000
  reference for the satellite look-angle cross-check (`tests/data/skyfield_iss_everett.csv`),
  and the design inspiration for Ephemerust; its bundled IAU 2000 nutation table is the source
  of the 77-term IAU 2000B series in `src/frames_nutation_table.rs`.
- [astropy](https://www.astropy.org/) / [pyerfa](https://github.com/liberfa/pyerfa) (ERFA, the
  open implementation of the IAU's SOFA library) — reference for the frame-rotation tests
  (`tests/data/astropy_frames.csv`).
- [JPL Horizons](https://ssd.jpl.nasa.gov/horizons/) — ephemeris data used to validate the
  planet positions.
- [CelesTrak](https://celestrak.org/) (Dr. T. S. Kelso) — TLE format documentation and orbital
  element data.
- [Astroviewer](https://www.astroviewer.net/iss/en/) — live ISS readout used for the spot
  check below.

## External check: ISS vs [Astroviewer](https://www.astroviewer.net/iss/en/)

A quick **library** trial (not the `track` CLI, which prints state at the TLE epoch only)
compared Ephemerust to Astroviewer's live readout for the same idea: current ISS subpoint,
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
| Orbital speed | 4.753 mi/s (17 111 mph) | **4.753 mi/s** (17 111 mph), ‖**v**‖ in TEME |
| Altitude | 269 mi (~432.9 km) | **432.8 km** WGS84 ellipsoidal height (~269 mi) |
| Longitude | 173.67° E | **173.68° E** |
| Latitude | 32.55° S | **32.74° S** |

Speed and altitude matched within rounding, and longitude agreed within **~0.01°**. Latitude
differed by **~0.2°** (~20 km), which is about three seconds of ISS motion over the ground.
That points to timing (a live readout versus a hand-copied whole-second timestamp) or a
different element set on Astroviewer's side, plus ~7.5 h of TLE age, rather than to the frame
conversion: the [Skyfield cross-check](docs/accuracy-and-limits.md) bounds Ephemerust's own
TEME → ECEF → geodetic error to well under an arcminute.

## License

MIT.
