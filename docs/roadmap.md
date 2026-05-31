# Roadmap

This project is being built in two phases, and is gradually moving toward a standalone Rust
service that can run on a home server rack and be reached remotely.

## Phase 1 — CLI astronomy tool ✅ (in progress, core complete)

The command-line calculator for astronomical and orbital-mechanics calculations. Implemented
and working: time systems, Sun/Moon/planet positions and rise/set, RA/Dec ↔ Alt/Az,
ECEF ↔ ECI, the `orbital` command (period, true anomaly, state vectors), planet
positions for all eight planets via truncated VSOP87D, satellite `track` (modes, JSON, TLE
sources; optional `network` feature for `--tle-url` stub), `examples/track_subpoint.rs`, and
teaching notes plus the `sgp4_teaching` scaffold (`docs/sgp4.md`).

### Remaining Phase 1 work

- Extend the VSOP87 series with more terms to push accuracy toward the arcsecond level.
- Optional: a dedicated `planets` subcommand for listing/comparing multiple planets.
- Longer term: advanced orbital propagation; asteroid/comet positions; stellar positions
  and proper motion; additional frames (GCRS, ITRS); precession/nutation and refraction
  corrections (see [accuracy-and-limits.md](accuracy-and-limits.md)).

## Phase 2 — Space-weather web service ⏳ (planned)

A REST API for fetching, caching, and serving space-weather data relevant to satellite
operations, complementing the CLI tool. This phase is where the project grows from a
local tool into networked infrastructure.

### Planned features

- **Data fetching** — integration with the NOAA Space Weather API and similar sources.
- **Local caching** — reduce upstream calls and improve response times.
- **REST endpoints** — query current conditions and historical data.
- **Storage** — historical data in SQLite or PostgreSQL.
- **Production concerns** — rate limiting and authentication.
- **Deployment** — self-hosted on a personal server rack.

### Use cases

- Satellite operators monitoring space-weather conditions.
- Mission planning from historical space-weather patterns.
- Real-time alerts for solar flares and geomagnetic storms.
- Radiation-level monitoring for space missions.

## Deployment target

The long-term goal is a standalone Rust program deployable on a home server rack and
accessible from remote locations.

### Server hardware (acquired)

| Component | Spec |
|-----------|------|
| CPU | 2× 8-core / 8-thread Xeon |
| Memory | 32 GB DDR4 ECC |
| Storage | SAS3 12-drive backplane |
| Network | 4× 10G RJ45 |
| Management | IPMI |
| Power | redundant 800W PSUs |
| Expansion | room for multiple GPUs |

### Hosting considerations

The host needs to run the Rust CLI/service, the REST API, a database (SQLite or
PostgreSQL), and web-server capabilities, with optional container support for deployment
flexibility. Options under consideration: a Linux distribution (Ubuntu Server, Debian, or
custom), containerization (Docker/Kubernetes), or bare-metal deployment.
