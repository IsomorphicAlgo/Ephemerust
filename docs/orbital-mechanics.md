# Orbital Mechanics

Module: `orbital.rs`

> **Implementation status.** Fully implemented and exposed through the `orbital` command:
> orbital period, true anomaly, and orbital-elements → state-vector conversion.

References:
[Kepler's laws](https://en.wikipedia.org/wiki/Kepler%27s_laws_of_planetary_motion),
[Kepler's equation](https://en.wikipedia.org/wiki/Kepler%27s_equation),
[Orbital elements](https://en.wikipedia.org/wiki/Orbital_elements),
[Orbital state vectors](https://en.wikipedia.org/wiki/Orbital_state_vectors).

## Kepler's Equation

**Purpose**: Relates mean anomaly (time-based) to eccentric anomaly (geometry-based) for
elliptical orbits.

```
M = E - e × sin(E)
```

- `M` = mean anomaly (degrees)
- `E` = eccentric anomaly (degrees)
- `e` = eccentricity (0–1)

**Solution** — Newton–Raphson iteration:
```
E_{n+1} = E_n - (E_n - e × sin(E_n) - M) / (1 - e × cos(E_n))
```

The implementation seeds `E = M` for `e < 0.8` and `E = π` otherwise, iterating up to 30
times or until the step falls below `1e-10`.

## True Anomaly from Eccentric Anomaly

```
ν = 2 × arctan(√((1+e)/(1-e)) × tan(E/2))
```

Gives the actual angular position from periapsis.

## Orbital Period (Kepler's Third Law)

```
T = 2π × √(a³ / μ)
```

- `T` = period (seconds)
- `a` = semi-major axis (meters)
- `μ` = standard gravitational parameter (m³/s²)

## Orbital Elements → State Vectors

**Purpose**: Convert classical elements `(a, e, i, Ω, ω, M)` into position and velocity
vectors.

1. Solve Kepler's equation for the true anomaly `ν`.
2. Radius: `r = a(1 − e²) / (1 + e·cos(ν))`.
3. Compute perifocal position and velocity (in the orbital plane).
4. Rotate from perifocal to the inertial frame via three rotations:
   - argument of periapsis (ω),
   - inclination (i),
   - longitude of ascending node (Ω).

**Use**: Satellite propagation, mission planning, and converting between orbital
representations.

## CLI

```bash
cargo run -- orbital --semi-major 6778 --eccentricity 0.0001 --inclination 51.6
# Elements: a=6778 km, e=0.0001, i=51.6°, Ω=0°, ω=0°, M=0°
# Period:   5553.5 s (92.56 min)
# True anomaly: 0.0000°
# State vector (inertial frame):
#   Position [km]:   x=6777.322 y=0.000 z=0.000
#   Velocity [km/s]: vx=-0.000000 vy=4.763832 vz=6.010461
```

Optional flags supply the remaining classical elements and the central body:

| Flag | Meaning | Default |
|------|---------|---------|
| `--raan` | longitude of ascending node Ω (deg) | 0 |
| `--arg-periapsis` | argument of periapsis ω (deg) | 0 |
| `--mean-anomaly` | mean anomaly M (deg) | 0 |
| `--mu` | gravitational parameter μ (km³/s²) | 398600.4418 (Earth) |

Inputs use km for the semi-major axis and km³/s² for μ, so outputs are in km and km/s.
