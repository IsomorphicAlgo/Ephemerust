# Roadmap

This project is being built in **two phases**: a **library + CLI** (Phase 1) and **networked /
hosted capabilities** (Phase 2). Full space-weather ingestion + REST is intentionally scoped to
companion **Rusty_Server**; Ephemerust may still grow optional thin HTTP for standalone demos.

## Phase 1 — CLI astronomy tool ✅ (in progress, core complete)

The command-line calculator for astronomical and orbital-mechanics calculations. Implemented
and working: time systems, Sun/Moon/planet positions and rise/set, RA/Dec ↔ Alt/Az,
ECEF ↔ ECI, the `orbital` command (period, true anomaly, state vectors), planet
positions for all eight planets via truncated VSOP87D, satellite `track` (modes, JSON, TLE
sources; optional `network` feature for `--tle-url` stub), `examples/track_subpoint.rs`, and
teaching notes plus the `sgp4_teaching` scaffold (`docs/sgp4.md`). The
[satellite-tracking plan](satellite-tracking-plan.md) milestones **M0–M9** are signed off.

### Remaining Phase 1 work

- Satellite tracking follow-ups from [satellite-tracking-plan.md — Future work](satellite-tracking-plan.md#future-work) (HTTP `--tle-url`, external validation tests, etc.).
- Extend the VSOP87 series with more terms to push accuracy toward the arcsecond level.
- Optional: a dedicated `planets` subcommand for listing/comparing multiple planets.
- Longer term: advanced orbital propagation; asteroid/comet positions; stellar positions
  and proper motion; additional frames (GCRS, ITRS); precession/nutation and refraction
  corrections (see [accuracy-and-limits.md](accuracy-and-limits.md)).

## Phase 2 — Hosted services & HTTP (re-scoped) ⏳

Ephemerust remains the **library + CLI** for astronomy and satellite propagation. A separate
**Rusty_Server** deployment hosts the space-weather stack: NOAA/DONKI ingestion, MySQL,
caching, rate limits, auth, and REST—including **`/api/v1/ephemeris/...`** endpoints that call
**`ephemerust`** as a dependency. Ephemerust does **not** duplicate that NOAA product surface
as a second full web service.

### What may still land in Ephemerust for “Phase 2”

- **Optional** thin HTTP or examples for **standalone demos** of the library without Rusty_Server
  (narrow scope—not a replacement for Rusty_Server’s space-weather API).
- Library features both CLI and API consumers need (e.g. `network` / `--tle-url`, propagation
  accuracy, teaching scaffolds).

### Use cases (space weather & operations)

- **Rusty_Server** (companion repo): satellite operators and dashboards consuming space weather
  and ephemeris JSON over the network.
- **Ephemerust CLI**: local calculations, scripting, and teaching workflows without running a server.

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

For the **space-weather + ephemeris REST** product described in Phase 2 above, the intended
runtime is **Rusty_Server** (MySQL, nginx/systemd, etc.). The host below also describes the
Ephemerust author’s rack used for **CLI builds**, optional local demos, and Rusty_Server.

The machine needs to run the Rust CLI, any optional demo HTTP, a database when running
Rusty_Server (MySQL today), and web-server capabilities, with optional container support for deployment
flexibility. Options under consideration: a Linux distribution (Ubuntu Server, Debian, or
custom), containerization (Docker/Kubernetes), or bare-metal deployment.
