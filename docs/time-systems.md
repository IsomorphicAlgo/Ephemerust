# Time Systems

Module: `time.rs`

All time-dependent astronomical calculations need a continuous, calendar-independent time
scale and a measure of Earth's rotation relative to the stars. This project uses the full
Julian Date (not the reduced/modified variant).

Related Wikipedia references: [Julian day](https://en.wikipedia.org/wiki/Julian_day),
[Sidereal time](https://en.wikipedia.org/wiki/Sidereal_time).

## Julian Date (JD)

**Purpose**: A continuous day count for astronomical calculations, independent of calendar
systems.

```
JD = day + (153×m + 2)/5 + 365×y + y/4 - y/100 + y/400 - 32045 + time_fraction
```

Where:

- `m` = month (adjusted for the algorithm)
- `y` = year (adjusted for the algorithm)
- `time_fraction = (hour - 12 + minute/60 + second/3600) / 24`

**Use**: Foundation for all time-dependent astronomical calculations.
The J2000.0 epoch = JD 2451545.0 (January 1, 2000, 12:00:00 TT).

## Greenwich Mean Sidereal Time (GMST)

**Purpose**: Measures Earth's rotation angle relative to the vernal equinox (fixed stars),
not the Sun.

```
GMST = (GMST_J2000 + SIDEREAL_RATE × (JD - J2000)) mod 24
```

Where:

- `GMST_J2000` = 18.697374558 hours (GMST at J2000.0)
- `SIDEREAL_RATE` = 24.06570982441908 hours/day (sidereal day rate)

**Use**: Required for transformations between rotating (ECEF) and non-rotating (ECI)
frames, and for converting between equatorial and horizontal coordinates.

## Local Sidereal Time (LST)

**Purpose**: Converts GMST to local sidereal time for a specific observer longitude.

```
LST = (GMST + longitude/15) mod 24
```

Where longitude is in degrees (positive = East, negative = West).

**Use**: Essential for converting between RA/Dec and Alt/Az coordinates — it relates the
observer's local meridian to the celestial coordinate system.

## CLI

```bash
# Julian Date and GMST at noon (default time)
cargo run -- time --date "2024-01-01"
# JD:   2460311.000000
# GMST: 18:42:34

# At a specific time
cargo run -- time --date "2024-01-01" --time "18:30:45"
# JD:   2460311.271354
# GMST: 01:14:24
```
