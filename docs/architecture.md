# Architecture

## Project layout

```
CLI_Astro_Calc/
├── Cargo.toml
├── Cargo.lock
├── readme.md           # quick start + command reference
├── CHANGELOG.md        # version & change history
├── docs/               # this documentation set
└── src/
    ├── main.rs         # CLI entry point and command parsing
    ├── lib.rs          # library root, re-exports, error types
    ├── time.rs         # Julian Date, sidereal time
    ├── coordinates.rs  # RA/Dec ↔ Alt/Az, ECEF ↔ ECI
    ├── celestial.rs    # Sun/Moon positions, rise/set
    ├── orbital.rs      # Kepler's equation, period, state vectors
    ├── planets.rs      # VSOP87 planetary positions
    └── satellite.rs    # TLE/SGP4 satellite tracking (in progress)
```

## Modules

| Module | Responsibility |
|--------|----------------|
| `lib.rs` | Root; public API re-exports; `AstroError` / `Result` |
| `time.rs` | Julian Date, GMST, LST |
| `coordinates.rs` | Equatorial/horizontal and Earth-centered transforms |
| `celestial.rs` | Sun/Moon position and rise/set; dispatches planet calls |
| `orbital.rs` | Orbital mechanics |
| `planets.rs` | VSOP87 ephemeris (complex and self-contained) |
| `satellite.rs` | TLE/SGP4 satellite tracking layer over the `sgp4` crate (in progress) |

**Separation rationale**: `celestial.rs` handles the simpler Sun/Moon models and routes
planet requests to `planets.rs`, which is kept separate because VSOP87 is large and
self-contained.

### Public API

`lib.rs` re-exports the commonly used items:

- Coordinate types: `RaDec`, `AltAz`, `Ecef`, `Eci`
- Celestial types: `CelestialObject`, `ObserverLocation`, `RiseSetTimes`
- Planet types: `Planet`, `calculate_planet_position`
- Time functions: `julian_date`, `greenwich_mean_sidereal_time`, `local_sidereal_time`
- Errors: `AstroError`, `Result<T>`

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` (derive) | CLI parsing |
| `chrono` | date/time handling |
| `thiserror` / `anyhow` | error handling |
| `log` / `env_logger` | logging |
| `serde` | serialization (date/time) |
| `sgp4` | TLE parsing and SGP4/SDP4 satellite propagation engine |
| `criterion` (dev) | benchmarking |

## Error handling patterns

The error type is `AstroError` (`InvalidCoordinate`, `InvalidTime`, `CalculationError`,
`IoError`), with `Result<T> = std::result::Result<T, AstroError>`.

- **Validate early** — check inputs (NaN, infinity, range) at function entry.
- **Fail fast for critical errors** — invalid Julian Date, missing required data
  (e.g. Earth's position), non-positive radius → return immediately.
- **Graceful degradation** — extreme dates, placeholder data, and out-of-range coordinates
  warn but proceed.
- **Actionable messages** — include the offending value and a hint for fixing it.

Representative checks:

```rust
if julian_date.is_nan()      { return Err(/* "cannot be NaN" */); }
if julian_date.is_infinite() { return Err(/* "cannot be infinite" */); }
if julian_date < MIN_JD || julian_date > MAX_JD {
    warn!("Julian Date outside recommended range");
}
```

## Logging

Multi-level logging via `log` + `env_logger`; `--verbose` raises the level to debug.

| Level | Used for |
|-------|----------|
| Error | invalid inputs, calculation failures, missing data |
| Warn  | out-of-range inputs, extreme dates, placeholder data, accuracy concerns |
| Info  | major operations (transformations, planet calculations, final values) |
| Debug | intermediate values: matrices, series evaluations, conversion steps |

## Testing

The suite has **76 unit tests + 6 doctests** (all passing).

- **Unit tests** — per-function, with known reference values, edge cases (poles, equator,
  origin, large coordinates), input validation, round-trip accuracy, and benchmarks.
- **Integration tests** — end-to-end CLI workflows and cross-module pipelines.
- **Validation tests** — comparison with authoritative sources (JPL Horizons, Meeus).
- **Doctests** — keep documentation examples compiling and correct.

Tests are co-located with source in `#[cfg(test)]` modules.

```bash
cargo test                       # everything
cargo test --lib                 # unit tests only
cargo test --doc                 # doctests only
cargo test -- --nocapture        # show output
cargo test --release performance # performance tests
```

### Measured performance

| Operation | Target | Actual |
|-----------|--------|--------|
| Planet calculation | < 10 ms | < 1 ms |
| VSOP87 series eval | < 100 µs | < 100 µs |
| ECEF/ECI transform | — | < 1 µs |
| Coordinate conversion | — | < 1 µs |
