# Accuracy & Limitations

## Current accuracy

| Calculation | Accuracy |
|-------------|----------|
| Propagated TEME state (default) | Matches `sgp4` WGS84+IAU path; ~tens of metres vs Vallado/AFSPC reference at epoch |
| Propagated TEME state (`--afspc` / `PropagationModel::AfspcCompatibility`) | Sub-metre vs Vallado verification vectors at epoch |
| Solar position | ~1 arcminute |
| Lunar position | ~10 arcminutes (limited perturbation terms) |
| RA/Dec ↔ Alt/Az | sub-arcsecond (floating-point limited) |
| ECEF ↔ ECI round-trip | ~1 mm at Earth scale |
| ECEF ↔ WGS84 geodetic round-trip | ~1 mm at Earth scale |
| Satellite look angles — direction (az/el) | ≤ 0.34 arcmin vs. Skyfield (same TLE, 24 h, all elevations) |
| Satellite look angles — slant range | ≤ 75 m vs. Skyfield (same TLE) |
| Satellite look angles — range rate | ≤ 0.41 m/s vs. Skyfield (same TLE); < 1 m/s vs. its own d(range)/dt |
| Atmospheric refraction (`refraction_deg`) | ~0.1′ above 15° elevation; weather-limited (several ′) near the horizon |
| Satellite pass prediction (`predict_passes`) | AOS/LOS times typically within ~1 minute of external tools; max elevation within a few degrees | Coarse scan step, bisection/ternary refinement, same frame/TLE limitations as look angles; GEO-like orbits use a simplified visibility model |
| Satellite `sgp4_teaching` (two-body scaffold) | Matches Kepler period from TLE mean motion; **does not** match SGP4 state (documented large ‖Δ**r**‖ vs `sgp4`) | Not a propagator; deep-space / low-n element sets rejected by `teaching_supported` |
| Planet positions (truncated VSOP87D) | a few arcminutes (vs. JPL Horizons at J2000.0) |

See [vsop87.md](vsop87.md) for the per-planet VSOP87 accuracy breakdown.

## Satellite validation and error budget

`tests/external_reference.rs` compares `Propagator::look_angles` with
[Skyfield](https://rhodesmill.org/skyfield/) (IAU 2000 TEME → GCRS → ITRS chain with UT1) for
the crate's reference ISS element set, observed from Everett, WA, every 4 minutes for 24 hours.
Both sides propagate the **same TLE**, so the comparison isolates Ephemerust's frame and
velocity handling. The reference file `tests/data/skyfield_iss_everett.csv` records its
provenance in its header.

| Quantity | Test bound | Measured worst case (1.0.0) |
|----------|-----------|------------------------------|
| Line-of-sight direction | 2 arcmin | 0.34 arcmin |
| Slant range | 0.25 km | 0.075 km |
| Range rate | 2 m/s | 0.41 m/s (≈ 0.6 Hz of Doppler at 437.5 MHz) |

For real tracking, the **TLE itself dominates**: a fresh element set is typically ~1 km off,
growing by roughly 1–3 km per day for low-Earth orbit, mostly along-track. Along-track error is
a timing error, so its effect on Doppler peaks near closest approach, where the line-of-sight
acceleration is largest — for a high ISS pass at UHF, roughly 25 Hz per kilometre of
along-track error. Use fresh TLEs (a few days old at most) when holding Doppler to ~±150 Hz.

> **Correction in 1.0.0.** Releases up to 0.7.0 added Earth's rotation to the satellite's
> Earth-fixed velocity with the wrong sign and also subtracted a velocity for the (stationary)
> observer. Range rate was off by up to |ω × r_observer| — about **0.31 km/s at 48° latitude**,
> or ~450 Hz of Doppler at 437.5 MHz. Positions, azimuth, elevation and range were unaffected.
> The old range-rate test's 0.25 km/s tolerance hid the error; the new tests bound it at 1 m/s.

## Earth-orientation frames (`frames`, 1.1.0)

`tests/frames_reference.rs` rotates 56 states (LEO, polar LEO, GEO and a high-eccentricity
apogee, at seven epochs from 2000 to 2026 including one 30 s before a leap second) from GCRF and
from TEME to ITRF and compares them with astropy/ERFA (the IAU SOFA library), feeding both sides
the same IERS UT1−UTC and polar motion:

| Path | Position (worst) | Velocity (worst) | Residual source |
|------|------------------|------------------|-----------------|
| GCRF → ITRF (IAU 2006 precession, IAU 2000B nutation, GAST, polar motion) | 6.8 cm | 0.25 mm/s | IAU 2000B vs 2000A nutation (~1 mas) |
| TEME → ITRF (IAU 1982 GMST, polar motion) | 2.4 mm | 0.33 mm/s | astropy's finite-difference velocities |

With Earth orientation supplied, satellite look angles match Skyfield to **0.04″ in direction,
0.3 m in range and 0.7 mm/s in range rate** (`tests/external_reference.rs`, same WGS72 constants
and UT1 as Skyfield) — down from 0.34′ / 75 m / 0.41 m/s with UT1 = UTC.

## Limitations

- **Simplified Sun/Moon models** — not full VSOP87 / ELP2000.
- **Precession/nutation only in `frames`** — the `coordinates` ECEF/ECI helpers and the
  Sun/Moon/planet pipeline assume the J2000.0 epoch with no precession/nutation, which limits
  them to the arcminute level. The [`frames`](../src/frames.rs) module (1.1.0) applies IAU 2006
  precession + IAU 2000B nutation for GCRF/EME2000 ↔ ITRF. Satellites from SGP4 never needed
  them: TEME is already "of date", and TEME → ECEF by a GMST rotation is the standard Vallado
  bridge.
- **Earth orientation is opt-in** — `look_angles` and friends take UT1 = UTC with no polar
  motion (at most ~0.9 s of Earth rotation ≈ 0.4 km at the equator, and ~10 m); pass IERS
  Bulletin A values through `Propagator::look_angles_with_eop` / `frames::inertial_to_itrf` to
  remove both.
- **No light-time correction** for planetary positions.
- **Refraction is opt-in.** No function applies atmospheric refraction implicitly; add
  `coordinates::refraction_deg` to a geometric elevation to get the apparent one.
- **Satellite propagation model** — default propagation uses the `sgp4` crate's WGS84 + IAU
  path (aligned with WGS84 geodetic output). Optional AFSPC compatibility selects WGS72 +
  legacy sidereal/epoch for Vallado / NORAD reference reproduction (`--afspc` on `track`,
  `PropagationModel::AfspcCompatibility` in the library).
- **Truncated VSOP87D** — each planet uses only the leading terms, giving a few arcminutes
  rather than the sub-arcsecond accuracy of the full series.
- **Satellite `look_angles`** — for the satellite pipeline, `ObserverLocation::elevation` is
  interpreted as **metres above the WGS84 ellipsoid** (not orthometric height above the geoid),
  while other commands still treat it as informational where noted.

## Future enhancements

- Full/truncated VSOP87 coefficients for all planets and Earth.
- IAU 2000A (full) nutation and the CIO-based chain, for sub-centimetre work.
- Reading IERS Bulletin A / finals2000A files directly instead of passing values by hand.
- Light-time and aberration corrections for planetary positions.
- More advanced orbital propagation; asteroid/comet positions; stellar positions and proper
  motion; TOD/MOD frames.

See the [roadmap](roadmap.md) for how these fit into the larger project direction.
