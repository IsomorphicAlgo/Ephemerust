# Roadmap

This project is being built in **two phases**: a **library + CLI** (Phase 1) and **networked /
hosted capabilities** (Phase 2). Full space-weather ingestion + REST is intentionally scoped to
companion **Rusty_Server**; Ephemerust may still grow optional thin HTTP for standalone demos.
Ephemerust is also the **astrodynamics backend for
[Chronus Gateway](https://github.com/IsomorphicAlgo/chronus-gateway)** (same maintainer): the
gateway's Physics–Telemetry Co-Validation consumes look angles, range-rate, and Sun geometry
through its `OrbitalPropagator` trait, so gateway needs are first-class inputs to this roadmap.

## Phase 1 — CLI astronomy tool ✅ (in progress, core complete)

The command-line calculator for astronomical and orbital-mechanics calculations. Implemented
and working: time systems, Sun/Moon/planet positions and rise/set, RA/Dec ↔ Alt/Az,
ECEF ↔ ECI, the `orbital` command (period, true anomaly, state vectors), planet
positions for all eight planets via truncated VSOP87D, satellite `track` (modes, JSON, TLE
sources incl. live `--tle-url` fetch with the `network` feature), `examples/track_subpoint.rs`,
and teaching notes plus the `sgp4_teaching` scaffold (`docs/sgp4.md`). The
[satellite-tracking plan](satellite-tracking-plan.md) milestones **M0–M9** are signed off.

**Modernization (0.5.0 / 0.6.0, July 2026) — done.** Rust **edition 2024** with refreshed
dependencies (0.5.0), then the "zero-cost abstractions" release (0.6.0): the reusable
**`satellite::Propagator`** (init-once SGP4, `Send + Sync`, no locking — built for the
gateway's per-frame loop), **`static` VSOP87 tables** (planet positions allocation-free,
−58%), standard traits (`FromStr`/`Display`), **criterion benchmarks** backing all
performance claims, and the [`rust-idioms.md`](rust-idioms.md) teaching chapter. Before/after
numbers live in the [CHANGELOG](../CHANGELOG.md).

**Physics & freshness (0.7.0, July 2026) — done.** The two gateway-driven gaps closed:
the **`eclipse` module** (conical umbra/penumbra shadow model with `shadow_transitions`
entry/exit search — went straight to conical rather than cylindrical-first; ~341 ns per
classification on a prebuilt `Propagator`), and the **`--tle-url` live fetch** implemented
per [http_plan.md](../http_plan.md) (bounded `ureq`/rustls client, `select_tle` +
`--tle-name` for multi-object bulletins, offline loopback-server test suite).

### Remaining Phase 1 work

- Satellite tracking follow-ups from [satellite-tracking-plan.md — Future work](satellite-tracking-plan.md#future-work) (external validation tests, Space-Track authenticated fetch if ever needed).
- Extend the VSOP87 series with more terms to push accuracy toward the arcsecond level.
- Optional: a dedicated `planets` subcommand for listing/comparing multiple planets.
- Longer term: advanced orbital propagation; asteroid/comet positions; stellar positions
  and proper motion; additional frames (GCRS, ITRS); precession/nutation and refraction
  corrections (see [accuracy-and-limits.md](accuracy-and-limits.md)).

### Downstream: Chronus Gateway integration notes

- ✅ The gateway's Ephemerust-backed `OrbitalPropagator` holds one
  **`ephemerust::Propagator`** per element set and calls its borrowing methods per frame
  (initialization is ~72% of a one-shot call; see `rust-idioms.md` §1). Migrated in
  `crates/gateway/src/propagator.rs` (July 2026).
- With **0.7.0 on crates.io**, the gateway can pin `ephemerust = "0.7"` instead of a
  sibling checkout, simplifying its CI (MSRV compatible: 1.88 ≤ 1.89), and can upgrade its
  CV-4 shadow check from the in-house Sun-geometry proxy to
  **`Propagator::shadow_state`** / `ShadowState` — real conical umbra/penumbra physics.

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
