# CLI Astro Calc — Documentation

This folder is the "encyclopedia" for the project: the mathematics, conventions, and
engineering behind each feature. The top-level [readme](../readme.md) is the quick-start
and command reference; the pages here go deep.

## Contents

| Page | What it covers |
|------|----------------|
| [Time systems](time-systems.md) | Julian Date, Greenwich/Local Sidereal Time |
| [Celestial positions](celestial-positions.md) | Sun and Moon position models |
| [Coordinate systems](coordinates.md) | RA/Dec ↔ Alt/Az, ECEF ↔ ECI, frame conventions |
| [Orbital mechanics](orbital-mechanics.md) | Kepler's equation, orbital period, state vectors |
| [VSOP87 planetary theory](vsop87.md) | Planetary ephemeris: series, data, pipeline |
| [Accuracy & limitations](accuracy-and-limits.md) | What's accurate, what's approximate, what's missing |
| [Architecture](architecture.md) | Module layout, error handling, logging, testing |
| [Satellite tracking plan](satellite-tracking-plan.md) | Iterative, stage-gated plan for TLE/SGP4 tracking and pass prediction |
| [Roadmap](roadmap.md) | Phase 2 (API service), server hardware, future work |

## How the project is organized

The project is structured in two phases:

1. **Phase 1 — CLI astronomy tool.** A command-line calculator for the standard
   astronomical and orbital-mechanics calculations used in space-mission operations and
   satellite control.
2. **Phase 2 — data services.** API-based data access, building toward a standalone Rust
   service that can be deployed on a home server rack and accessed remotely. See the
   [roadmap](roadmap.md).

## References

These docs implement algorithms based on:

- Jean Meeus, *Astronomical Algorithms* — standard reference for astronomical calculations.
- IAU conventions for coordinate systems and time.
- Kepler's laws and classical orbital mechanics.
- VSOP87 theory (P. Bretagnon & G. Francou) for planetary ephemerides.
- [JPL Horizons](https://ssd.jpl.nasa.gov/horizons/) for validation.
