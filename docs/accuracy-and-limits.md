# Accuracy & Limitations

## Current accuracy

| Calculation | Accuracy |
|-------------|----------|
| Solar position | ~1 arcminute |
| Lunar position | ~10 arcminutes (limited perturbation terms) |
| RA/Dec ↔ Alt/Az | sub-arcsecond (floating-point limited) |
| ECEF ↔ ECI round-trip | ~1 mm at Earth scale |
| ECEF ↔ WGS84 geodetic round-trip | ~1 mm at Earth scale |
| Satellite look angles (azimuth / elevation) | on the order of a few degrees vs. external tools | TLE age, omitted precession/nutation, simplified TEME→ECEF bridge |
| Satellite pass prediction (`predict_passes`) | AOS/LOS times typically within ~1 minute of external tools; max elevation within a few degrees | Coarse scan step, bisection/ternary refinement, same frame/TLE limitations as look angles; GEO-like orbits use a simplified visibility model |
| Planet positions (truncated VSOP87D) | a few arcminutes (vs. JPL Horizons at J2000.0) |

See [vsop87.md](vsop87.md) for the per-planet VSOP87 accuracy breakdown.

## Limitations

- **Simplified Sun/Moon models** — not full VSOP87 / ELP2000.
- **No precession/nutation** — ECEF/ECI assumes the J2000.0 epoch with no
  precession/nutation corrections (IAU-76/FK5 and IAU-2000/2006 documented for the future).
- **No light-time correction** for planetary positions.
- **No atmospheric refraction** in Alt/Az conversions.
- **Truncated VSOP87D** — each planet uses only the leading terms, giving a few arcminutes
  rather than the sub-arcsecond accuracy of the full series.
- **Satellite `look_angles`** — for the satellite pipeline, `ObserverLocation::elevation` is
  interpreted as **metres above the WGS84 ellipsoid** (not orthometric height above the geoid),
  while other commands still treat it as informational where noted.

## Future enhancements

- Full/truncated VSOP87 coefficients for all planets and Earth.
- IAU-2000/2006 precession–nutation for high-precision ECEF/ECI.
- Atmospheric refraction in Alt/Az.
- Light-time and aberration corrections for planetary positions.
- More advanced orbital propagation; asteroid/comet positions; stellar positions and proper
  motion; additional frames (GCRS, ITRS).

See the [roadmap](roadmap.md) for how these fit into the larger project direction.
