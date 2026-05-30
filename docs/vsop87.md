# VSOP87 Planetary Positions

Module: `planets.rs`

> **Implementation status.** Working for all eight planets, including Earth (whose
> heliocentric position drives the geocentric conversion). Each body uses a truncated
> VSOP87D series of the leading terms. Geocentric RA/Dec agrees with JPL Horizons to within
> a few arcminutes at the J2000.0 epoch. Adding more terms would tighten this toward the
> arcsecond level.
>
> The time argument is **Julian millennia** from J2000.0 (`τ = (JD − 2451545)/365250`), and
> VSOP87D spherical coefficients are stored directly in radians (L, B) and AU (R) — no
> additional scaling is applied.

## Overview

VSOP87 (*Variations Séculaires des Orbites Planétaires 1987*) is a semi-analytical model
by P. Bretagnon and G. Francou (Bureau des Longitudes, Paris). It expresses a planet's
heliocentric coordinates as sums of periodic terms.

References:
[VSOP87](<https://en.wikipedia.org/wiki/VSOP_(planets)>),
[JPL Horizons](https://ssd.jpl.nasa.gov/horizons/),
[IMCCE VSOP87 data](https://ftp.imcce.fr/pub/ephem/planets/vsop87/).

## Mathematical foundation

Each term has the form:

```
A × cos(B + C × t)
```

- `A` — amplitude (radians for L/B, AU for R)
- `B` — phase (radians)
- `C` — frequency (per Julian millennium)
- `t` — time in Julian millennia from J2000.0

Each coordinate (longitude L, latitude B, radius R) is a polynomial in `t` whose
coefficients are themselves sums of such terms:

```
L = L0 + L1×t + L2×t² + L3×t³ + L4×t⁴ + L5×t⁵
B = B0 + B1×t + B2×t² + B3×t³ + B4×t⁴
R = R0 + R1×t + R2×t² + R3×t³ + R4×t⁴          (AU)
```

where each `L0`, `L1`, … is `Σ A_i·cos(B_i + C_i·t)`.

(The VSOP87D *spherical* coefficients used here are already in final units — radians and
AU — so unlike the integer "×10⁻⁸" tabulations found in some references, no extra scaling
is applied.)

### VSOP87 variants

| Variant | Coordinates | Equinox |
|---------|-------------|---------|
| VSOP87  | ecliptic orbital elements | J2000.0 |
| VSOP87A | ecliptic rectangular | J2000.0 |
| **VSOP87B** | **ecliptic spherical** | **J2000.0** — used here |
| VSOP87C | ecliptic rectangular | of date |
| VSOP87D | ecliptic spherical | of date |
| VSOP87E | barycentric rectangular | J2000.0 |

## Time variable

VSOP87 uses time in **Julian millennia** from J2000.0:

```
τ = (JD - 2451545.0) / 365250.0
```

Example — Jan 1, 2024 (JD ≈ 2460311.0): `τ ≈ 0.0240` millennia.

(Note: this is millennia, not centuries — a common source of error when implementing
VSOP87. The series frequencies are expressed per millennium.)

## Series evaluation algorithm

1. **Each term**: `term = A × cos(B + C × t)`.
2. **Each sub-series**: sum its terms, e.g. `L0 = Σ A_i·cos(B_i + C_i·t)`.
3. **Combine** with time powers (see the formulas above).
   - L is in radians and normalized to `[0, 2π)`.
   - B is in radians (small — planets stay near the ecliptic).
   - R is in AU and must be positive.

Performance notes: time powers `t², t³, …` are pre-computed once and reused; truncated
series only evaluate the most significant terms.

## Coordinate conversion pipeline

```
VSOP87 output (L, B, R)
    ↓  obliquity ε
Ecliptic → Equatorial  (rotate by ε)
    ↓  subtract Earth's position
Heliocentric → Geocentric
    ↓
Rectangular → Spherical (RA, Dec)
```

### 1. Obliquity of the ecliptic

```
ε = 23.4393° - 0.0000004° × d        (d = days since J2000.0; simplified)
```

Higher precision (arcsecond):
```
ε = 23.439291° - 0.0130042°·t - 0.00000016°·t² + 0.000000503°·t³
```

### 2. Ecliptic → equatorial

Ecliptic rectangular coordinates:
```
x_ecl = R × cos(B) × cos(L)
y_ecl = R × cos(B) × sin(L)
z_ecl = R × sin(B)
```

Rotate about the X-axis by ε:
```
x_eq = x_ecl
y_eq = y_ecl × cos(ε) - z_ecl × sin(ε)
z_eq = y_ecl × sin(ε) + z_ecl × cos(ε)
```

### 3. Heliocentric → geocentric

Subtract Earth's heliocentric equatorial position (also from VSOP87):
```
[x_geo]   [x_planet]   [x_earth]
[y_geo] = [y_planet] - [y_earth]
[z_geo]   [z_planet]   [z_earth]
```

### 4. Rectangular → RA/Dec

```
RA  = atan2(y, x)            → hours = (degrees / 15) mod 24
r   = √(x² + y² + z²)
Dec = arcsin(z / r)          → degrees
```

### Light-time correction (optional, not implemented)

For >arcsecond precision, recompute the planet's position at `t − distance/c`. Impact is
~0.01 arcsec for inner planets, negligible for outer planets. Omitted here for simplicity.

## Coefficient data structure

The coefficients are embedded in the source (in `planets.rs`), so there is no runtime file
I/O — the data ships inside the binary. File-based loading was considered but deferred.

```rust
struct Vsop87Term {
    amplitude: f64,  // A
    phase: f64,      // B (radians)
    frequency: f64,  // C
}
```

- Each planet has three `Vsop87Series` (L, B, R).
- Each series holds sub-series (L0–L5, B0–B4, R0–R4), each a list of `Vsop87Term`.

**Full vs. truncated**: full VSOP87 has hundreds to ~1000 terms per series; this project
uses the leading terms per series, trading <1 arcsecond accuracy for a few arcminutes.

## Accuracy expectations

| Planets | Truncated (this project) | Full VSOP87 |
|---------|--------------------------|-------------|
| Inner (Mercury, Venus, Mars) | ~1 arcmin | <1 arcsec |
| Outer (Jupiter, Saturn) | ~1–2 arcmin | <1 arcsec |
| Distant (Uranus, Neptune) | ~2–5 arcmin | <1 arcsec |

## Data acquisition process

1. **Source**: IMCCE official VSOP87 files (primary), or the VSOP87 multi-language project
   for ready-made truncated versions.
2. **Process**: parse the `A B C` rows; filter by amplitude threshold for truncation;
   organize by planet, variable (L/B/R), and series (0–5).
3. **Store**: convert to `Vsop87Term` constants embedded in source.
4. **Validate**: sanity-check ranges and compare against JPL Horizons at J2000.0.

## Validation methodology

- **Unit tests**: term and series evaluation (including `t = 0`, large/negative `t`, empty
  and single-term series); coordinate-range checks (L ∈ [0, 2π), B ∈ [−π/2, π/2], R > 0).
- **Integration tests**: multiple planets at one epoch; one planet across epochs; verifying
  motion over time and longitude wrap-around.
- **Reference sources**: JPL Horizons, Meeus *Astronomical Algorithms*, IMCCE.
- **Performance**: target < 10 ms/planet; measured < 1 ms/planet; series eval < 100 µs.

## CLI

```bash
cargo run -- position --object jupiter --date "2000-01-01"
# RA:  01:35:28
# Dec: +08°35'39"

# Verbose: show VSOP87 series values and each conversion step
cargo run -- --verbose position --object jupiter --date "2000-01-01"
```

Output format: `RA: HH:MM:SS`, `Dec: ±DD°MM'SS"`.

Sample geocentric positions at 2000-01-01 (compared to JPL Horizons, all within a few
arcminutes):

| Planet | RA | Dec |
|--------|------|------|
| Mercury | 18:08:24 | −24°23′57″ |
| Venus | 15:59:36 | −18°27′09″ |
| Mars | 22:02:05 | −13°10′54″ |
| Jupiter | 01:35:28 | +08°35′39″ |
| Saturn | 02:35:04 | +12°35′44″ |
| Uranus | 21:10:09 | −17°00′32″ |
| Neptune | 20:21:41 | −19°12′23″ |

Common errors:

- **Unknown object** — name not recognized (spelling; case-insensitive).
- **Invalid date** — must be `YYYY-MM-DD`.

See [error handling in architecture.md](architecture.md#error-handling-patterns) for the
validation and logging strategy.
