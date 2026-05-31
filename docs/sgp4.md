# SGP4 and the Two-Line Element set (near-Earth overview)

This chapter explains **what** the Simplified General Perturbations / Deep-Space Perturbations
(SGP4/SDP4) models are trying to do, **how** they relate to the numbers printed on a TLE, and
**where** Ephemerust sits: the production path uses the audited [`sgp4`](https://crates.io/crates/sgp4)
crate; the [`sgp4_teaching`](../src/sgp4_teaching.rs) module holds a small, **explicitly
non-operational** two-body skeleton you can compare against that engine.

---

## 1. Why TLEs exist

A Two-Line Element set is a **compact broadcast format** for approximate satellite state. It
was never intended as a sub-metre ephemeris: it encodes **mean** elements averaged over the fit
span, drag proxy **B\***, and secular rates so a compact algorithm can reproduce NORAD-style
predictions quickly on hardware from the 1980s.

Operational users treat TLEs as **inputs to a fixed propagator** (today: SGP4/SDP4), not as
osculating Keplerian elements at face value.

---

## 2. Near-Earth vs deep-space

- **Near-Earth (SGP4)** — mean motion large enough that the orbit is treated as “close” in the
  sense of the original Spacetrack report: zonal harmonics **J₂–J₄**, drag-like terms built from
  **B\***, and a long list of **secular and periodic** corrections to the mean elements before
  short-periodics are applied.
- **Deep-space (SDP4)** — lunar–solar gravity and resonance handling; different code path in the
  reference implementation.

The `sgp4` Rust crate implements the full branching model. Ephemerust does **not** reimplement
that tree; it calls the crate and then applies its own **TEME → ECEF → geodetic** layer for
ground tracks and observer geometry.

---

## 3. What the printed fields mean (line 2, conceptually)

| Field | Role in the story |
|-------|-------------------|
| Inclination **i** | Orbit plane tilt from the equator. |
| RAAN **Ω** | Longitude of the ascending node on the equator. |
| Eccentricity **e** | Orbit shape (from implied decimal on the TLE). |
| Argument of perigee **ω** | Orientation of the ellipse in the plane. |
| Mean anomaly **M** | Uniform-in-time proxy locating the satellite on the mean ellipse. |
| Mean motion **n** | Kozai-style **revolutions per day** — fixes the orbit size once μ is fixed. |
| **B\*** | Empirical drag / radiation parameter in inverse Earth radii — feeds decay and higher
  harmonics in the real model. |

SGP4 **does not** hand you osculating position by plugging these means into Kepler’s equation
once: it first **updates** means and adds periodic pieces, then builds an osculating vector in
**TEME** (True Equator, Mean Equinox of date).

---

## 4. Kepler’s third law link (the teaching hook)

For a circular idealisation, mean motion in rad/s relates to semi-major axis **a** (km) by

\[
n^2 a^3 = \mu .
\]

Legacy SGP4 documentation historically pairs the theory with **WGS-72** Earth gravity
\(\mu \approx 398\,600.8\ \mathrm{km^3/s^2}\). Converting printed **n** (rev/day) to rad/s and
solving for **a** reproduces the orbit scale used in many textbook derivations.

The [`sgp4_teaching::semi_major_axis_km_from_mean_motion`] function encodes exactly that step.
The unit test `keplerian_period_matches_reciprocal_mean_motion` locks it to the identity

\[
T = \frac{2\pi}{\sqrt{\mu/a^3}} = \frac{86400}{n_{\mathrm{rev/day}}}\ \mathrm{s}
\]

up to floating noise — **no** SGP4 code path involved.

---

## 5. Outline of the real near-Earth SGP4 pipeline (conceptual)

The operational algorithm (Hoots & Roehrich; revised by Vallado *et al.*, 2006) is a long
sequence of algebraic and trigonometric updates. At a **bird’s-eye** level:

1. **Unpack** the BCD-like TLE fields into floating values with fixed units and conventions.
2. **Recover** orbit size from mean motion with the model’s Earth constants (not necessarily
   WGS-84 μ).
3. Apply **secular** rates to Ω, ω, M (and related quantities) from J₂ and higher zonal terms.
4. Add **long-period** and **short-period** perturbations in the plane and out of plane.
5. Convert to an **inertial Cartesian** state in **TEME** for output.
6. (Outside SGP4 itself) rotate to **ECEF** with Earth rotation and convert to geodetic lat/lon.

Ephemerust’s [`satellite::propagate`] performs step 5–6 boundary via `sgp4` + Greenwich sidereal
time, as documented in [`satellite`](../src/satellite.rs) and [`accuracy-and-limits.md`](accuracy-and-limits.md).

---

## 6. The `sgp4_teaching` module (what it is *for*)

[`sgp4_teaching`](../src/sgp4_teaching.rs) implements a **deliberately reduced** path:

- Recover **a** from **n** with WGS-72 μ.
- Advance **M** linearly in time using the printed mean motion.
- Call the existing **two-body** [`elements_to_state_vector`](../src/orbital.rs) to obtain a
  Cartesian state.

It **omits** drag, zonal harmonics beyond the Keplerian backbone, and every SPTRCK harmonic —
so positions **diverge** from `sgp4::Constants::propagate` by hundreds to thousands of kilometres
over tens of minutes for typical ISS-class TLEs. The regression test
`teaching_two_body_vs_sgp4_iss_within_documented_loose_bound` encodes only a **loose** upper bound
and exists to **pin** that behaviour, not to claim parity.

When mean motion falls below [`MIN_MEAN_MOTION_REV_PER_DAY`](../src/sgp4_teaching.rs), the
module refuses to run: that band is where SDP4 / exotic regimes dominate — use production `sgp4`.

---

## 7. References (read these, not this page alone)

- F. R. Hoots & R. L. Roehrich, *Spacetrack Report No. 3* (1980) — original SGP4/SDP4 definition.
- D. A. Vallado *et al.*, “Revisiting Spacetrack Report #3” (AIAA 2006-6753) — widely used
  clarifications and test vectors.
- D. A. Vallado, *Fundamentals of Astrodynamics and Applications* — textbook context for TEME,
  frames, and perturbation theory.
- The [`sgp4`](https://crates.io/crates/sgp4) crate source and tests — the **oracle** Ephemerust
  relies on for propagation.

---

## 8. Ephemerust policy

| Layer | Responsibility |
|-------|------------------|
| `sgp4` crate | Numerical SGP4/SDP4 propagation to TEME. |
| `satellite` | TLE parsing, teaching-grade parse errors, TEME→ECEF→WGS84, passes, tracks. |
| `sgp4_teaching` | Pedagogy + tight self-consistency checks — **never** used by `propagate`. |

If you need mission-grade ephemerides, use tools that ingest **full force models** and **not**
only public TLEs.
