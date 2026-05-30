# Celestial Object Positions (Sun & Moon)

Module: `celestial.rs` (planets live in [vsop87.md](vsop87.md))

These models compute the Right Ascension (RA) and Declination (Dec) of the Sun and Moon
for any date, plus rise/set times for any observer location.

References:
[Position of the Sun](https://en.wikipedia.org/wiki/Position_of_the_Sun),
[Sunrise equation](https://en.wikipedia.org/wiki/Sunrise_equation),
[Lunar theory](https://en.wikipedia.org/wiki/Lunar_theory),
[Jean Meeus](https://en.wikipedia.org/wiki/Jean_Meeus).

## Solar Position (simplified model)

**Purpose**: The Sun's equatorial coordinates (RA/Dec) for any date.

1. **Mean anomaly** (`d` = days since J2000.0):
   ```
   M = 357.5291° + 0.98560028° × d
   ```
2. **Equation of center** (elliptical-orbit correction):
   ```
   EOC = 1.9148° × sin(M) + 0.0200° × sin(2M) + 0.0003° × sin(3M)
   ```
3. **Ecliptic longitude**:
   ```
   λ = M + EOC + 180° + 102.9372°
   ```
4. **Obliquity of the ecliptic**:
   ```
   ε = 23.4393° - 0.0000004° × d
   ```
5. **Ecliptic → equatorial**:
   ```
   RA  = atan2(sin(λ) × cos(ε), cos(λ))
   Dec = arcsin(sin(λ) × sin(ε))
   ```

Accuracy: ~1 arcminute, suitable for sunrise/sunset and most solar observations.

## Lunar Position (perturbation theory)

**Purpose**: The Moon's position, using multiple periodic terms to account for its complex
orbit.

1. **Mean arguments** (`t` in Julian centuries):
   - Mean longitude: `L' = 218.316° + 481267.881° × t + ...`
   - Mean elongation: `D = 297.850° + 445267.111° × t + ...`
   - Sun's mean anomaly: `M = 357.529° + 35999.050° × t + ...`
   - Moon's mean anomaly: `M' = 134.963° + 477198.868° × t + ...`
   - Argument of latitude: `F = 93.272° + 483202.018° × t + ...`
2. **Perturbation series** (sum of periodic terms):
   ```
   σ_L = 6288774" × sin(M') + 1274027" × sin(2D - M') + ...
   σ_B = 5128122" × sin(F)  + 280602"  × sin(M' + F) + ...
   ```
3. **Ecliptic coordinates**:
   ```
   λ = (L' + σ_L/1000000) mod 360°
   β = (σ_B/1000000) mod 360°
   ```
4. **Ecliptic → equatorial**:
   ```
   RA  = atan2(sin(λ) × cos(ε) - tan(β) × sin(ε), cos(λ))
   Dec = arcsin(sin(β) × cos(ε) + cos(β) × sin(ε) × sin(λ))
   ```

Accuracy: ~10 arcminutes (limited number of perturbation terms).

## Rise / Set Times

Given an object's position and an observer location (latitude, longitude, elevation), the
rise/set calculation determines when the object crosses the horizon.

```bash
# Sunrise/sunset in New York today
cargo run -- rise-set --object sun --latitude 40.7128 --longitude=-74.0060

# Sunrise/sunset in Seattle on a specific date
cargo run -- rise-set --object sun --latitude 47.6061 --longitude=-122.3328 --date "2024-12-25"

# Moonrise/moonset
cargo run -- rise-set --object moon --latitude 47.6061 --longitude=-122.3328 --date "2024-06-21"

# Planet rise/set (uses the VSOP87 position for the day)
cargo run -- rise-set --object jupiter --latitude 47.6061 --longitude=-122.3328 --date "2000-01-01"
# Rise: 20:23:21 UTC
# Set:  09:46:29 UTC
```

The same algorithm serves the Sun, Moon, and planets; only the horizon altitude correction
differs (Sun −0.833° for refraction + semidiameter, Moon −0.583°, planets −0.5667° for
refraction alone, since they are effectively point sources). When an object is circumpolar
or never rises at the given latitude, both rise and set are reported as "Does not rise/set".

## CLI: positions

```bash
# Sun position on the summer solstice
cargo run -- position --object sun --date "2024-06-21"
# RA:  06:00:49
# Dec: +23°26'08"

# Moon position
cargo run -- position --object moon --date "2024-01-01"
# RA:  10:58:11
# Dec: +10°02'27"
```

For planet positions, see [vsop87.md](vsop87.md).
