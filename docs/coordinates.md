# Coordinate Systems & Transformations

Module: `coordinates.rs`

Two transformation families are implemented:

- **RA/Dec ↔ Alt/Az** — equatorial (celestial) to horizontal (observer-based).
- **ECEF ↔ ECI** — Earth-Centered Earth-Fixed to Earth-Centered Inertial.
- **ECEF ↔ WGS84 geodetic** — Cartesian Earth-fixed position to latitude, longitude, and
  ellipsoidal height (`Geodetic`), and back.

References:
[Equatorial coordinate system](https://en.wikipedia.org/wiki/Equatorial_coordinate_system),
[Horizontal coordinate system](https://en.wikipedia.org/wiki/Horizontal_coordinate_system),
[Spherical trigonometry](https://en.wikipedia.org/wiki/Spherical_trigonometry),
[ECEF](https://en.wikipedia.org/wiki/ECEF),
[ECI](https://en.wikipedia.org/wiki/Earth-centered_inertial).

## RA/Dec → Alt/Az (equatorial → horizontal)

**Purpose**: Convert celestial coordinates (fixed relative to stars) into observer-based
altitude/azimuth.

1. **Hour angle**:
   ```
   HA = LST - RA   (hours; degrees: HA° = HA × 15)
   ```
2. **Altitude**:
   ```
   sin(Alt) = sin(Dec) × sin(Lat) + cos(Dec) × cos(Lat) × cos(HA)
   ```
3. **Azimuth**:
   ```
   Az = atan2(sin(HA), cos(HA) × sin(Lat) - tan(Dec) × cos(Lat)) + 180°
   Az = Az mod 360°
   ```

## Alt/Az → RA/Dec (horizontal → equatorial)

**Purpose**: Inverse transformation — telescope pointing coordinates back to celestial
coordinates.

1. **Declination**:
   ```
   sin(Dec) = sin(Alt) × sin(Lat) + cos(Alt) × cos(Lat) × cos(Az)
   ```
2. **Hour angle**:
   ```
   HA = atan2(-sin(Az), cos(Az) × sin(Lat) + tan(Alt) × cos(Lat))
   ```
3. **Right ascension**:
   ```
   RA = (LST - HA/15) mod 24 hours
   ```

> The CLI uses the current time and a default observer location
> (47.9088° N, 122.2503° W — Everett, WA) for these conversions.

## ECEF ↔ ECI

Both transformations rotate about the Z-axis (Earth's rotation axis) by an angle derived
from GMST.

**Z-axis rotation matrix**:
```
R_z(θ) = [ cos(θ)   sin(θ)   0 ]
         [-sin(θ)   cos(θ)   0 ]
         [  0        0       1 ]
```

**ECEF → ECI** ("undo" Earth's rotation):
```
θ = -GMST × 15°
[ECI] = R_z(θ) · [ECEF]
```

**ECI → ECEF** (apply Earth's rotation): same matrix with `θ = +GMST × 15°`.

**Use**: Satellite tracking, GPS, ground-station pointing — any application that needs
coordinates either fixed to the stars (ECI) or fixed to Earth's surface (ECEF).

Round-trip accuracy is ~1 mm at Earth scale. See
[accuracy-and-limits.md](accuracy-and-limits.md) for the precession/nutation caveat.

## ECEF ↔ WGS84 geodetic

**Purpose**: Turn Earth-fixed `(x, y, z)` metres into geodetic latitude, longitude, and
height above the WGS84 ellipsoid — the usual bridge from propagation output to a map.

**Implementation**: `geodetic_wgs84_to_ecef` and `ecef_to_geodetic_wgs84` in `coordinates.rs`
use the WGS84 semi-major axis and inverse flattening; the inverse path uses Bowring's
closed-form latitude followed by the prime-vertical radius for height. Round-trip accuracy is
on the order of **1 mm** at Earth scale in unit tests.

The `satellite` module wraps the inverse as `ecef_to_geodetic` into `Subpoint` (altitude in
kilometres) for the tracking API.

## Frame conventions

### Equatorial (RA/Dec)
- Reference frame: fixed relative to stars (J2000.0).
- RA: 0–24 hours, eastward from the vernal equinox.
- Dec: −90° to +90°, from the celestial equator.

### Horizontal (Alt/Az)
- Reference frame: observer-based, rotates with Earth.
- Alt: −90° to +90° above the horizon.
- Az: 0–360°, clockwise from North.

### ECEF (Earth-Centered Earth-Fixed)
- Reference frame: rotates with Earth. Units: meters.
- X: equator at the prime meridian (Greenwich). Y: equator at 90° E. Z: North Pole.

### ECI (Earth-Centered Inertial)
- Reference frame: fixed relative to stars (J2000.0). Units: meters.
- X: toward the vernal equinox. Y: completes the right-handed equatorial plane. Z: North Pole.

## CLI

```bash
# RA/Dec → Alt/Az (current time, default location)
cargo run -- convert --from ra-dec --to alt-az --coords "12.5,45.0"

# Alt/Az → RA/Dec
cargo run -- convert --from alt-az --to ra-dec --coords "45.0,180.0"

# ECEF → ECI (auto GMST from current time). Greenwich point on the equator:
cargo run -- convert --from ecef --to eci --coords "6378137.0,0.0,0.0"

# ECEF → ECI at a specific GMST
cargo run -- convert --from ecef --to eci --coords "6378137.0,0.0,0.0" --gmst 12.5

# ECI → ECEF
cargo run -- convert --from eci --to ecef --coords "6378137.0,0.0,0.0"
```

**Coordinate formats**

| Pair | Format | Example |
|------|--------|---------|
| RA/Dec | `hours,degrees` | `12.5,45.0` |
| Alt/Az | `altitude,azimuth` (deg) | `45.0,180.0` |
| ECEF/ECI | `x,y,z` (meters) | `6378137.0,0.0,0.0` |

Typical radii: Earth's surface ~6,378,137 m; LEO/ISS ~6,778,137 m (400 km);
geosynchronous ~42,164,000 m.
