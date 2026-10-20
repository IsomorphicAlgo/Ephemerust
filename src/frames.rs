//! Earth-orientation-aware reference frames: inertial (GCRF, EME2000, TEME) ↔ Earth-fixed (ITRF).
//!
//! The satellite pipeline in [`crate::satellite`] converts SGP4's TEME output to Earth-fixed
//! coordinates with a single GMST rotation, which is the standard (Vallado) bridge and agrees
//! with an IAU 2000 reference to well under an arcminute. This module adds the pieces needed
//! when the inputs are better than a TLE:
//!
//! - **Earth orientation parameters** ([`EarthOrientation`]): UT1−UTC (where the Earth has
//!   actually rotated to, as opposed to where a uniform clock says it should be) and polar
//!   motion (the wobble of the rotation axis relative to the crust). Both are published daily
//!   by the IERS (Bulletin A); leaving them at zero costs up to ~0.4 km and ~15 m respectively
//!   at the equator.
//! - **Precession–nutation**: the slow (26 000-year) and fast (18.6-year and shorter) motions of
//!   the Earth's axis that separate an inertial frame such as GCRF/EME2000 from the equator of
//!   date. Implemented with the IAU 2006 precession (Fukushima–Williams angles) and the IAU 2000B
//!   nutation series (77 luni-solar terms, ~1 milliarcsecond), combined with Greenwich apparent
//!   sidereal time and polar motion in the classical equinox-based chain:
//!
//!   **r**_ITRF = W · R3(GAST) · N · P · B · **r**_GCRF
//!
//! - **Time scales**: TAI−UTC from the leap-second table, TT = TAI + 32.184 s (precession and
//!   nutation run on TT; Earth rotation runs on UT1).
//!
//! Accuracy (see `tests/frames_reference.rs`): against astropy/ERFA (the IAU's SOFA library)
//! with identical Earth orientation inputs, positions agree to a few centimetres in LEO and
//! velocities to well under 1 mm/s; the residual is the IAU 2000B vs 2000A nutation difference.
//!
//! Velocities include the Earth-rotation transport term (**v**_fixed = R·**v**_inertial −
//! **ω** × **r**_fixed); the much slower time derivatives of precession, nutation and polar
//! motion are neglected (≲ 1 µm/s in LEO).

use crate::{AstroError, Result};
use chrono::{DateTime, TimeZone, Utc};

/// Arcseconds to radians.
const AS2R: f64 = std::f64::consts::PI / (180.0 * 3600.0);
/// 0.1 microarcsecond to radians (unit of the nutation series).
const U2R: f64 = AS2R / 1.0e7;
/// Earth rotation rate (rad/s): the time derivative of the Earth Rotation Angle.
const OMEGA_EARTH: f64 = 7.292_115_146_706_979e-5;
const TAU: f64 = std::f64::consts::TAU;

/// Earth orientation parameters (EOP) for one day, as published in IERS Bulletin A.
///
/// - `ut1_minus_utc_s` — UT1−UTC in seconds. Leap seconds keep it within ±0.9 s.
/// - `polar_x_arcsec`, `polar_y_arcsec` — pole coordinates *x*ₚ, *y*ₚ in arcseconds (typically
///   a few tenths of an arcsecond).
///
/// [`EarthOrientation::default`] (all zero) reproduces the classical "UT1 = UTC, no polar
/// motion" approximation. The struct is `#[non_exhaustive]` so further parameters (for example
/// length-of-day or celestial pole offsets) can be added without breaking callers; build it with
/// [`EarthOrientation::new`].
///
/// # Example
///
/// ```
/// use ephemerust::frames::EarthOrientation;
///
/// // IERS Bulletin A values for 2020-07-12.
/// let eop = EarthOrientation::new(-0.2273, 0.1875, 0.4171)?;
/// assert!((eop.ut1_minus_utc_s + 0.2273).abs() < 1e-12);
///
/// // Milliseconds by mistake: UT1-UTC is never more than 0.9 s.
/// assert!(EarthOrientation::new(-227.3, 0.0, 0.0).is_err());
/// # Ok::<(), ephemerust::AstroError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
#[non_exhaustive]
pub struct EarthOrientation {
    /// UT1−UTC in seconds.
    pub ut1_minus_utc_s: f64,
    /// Polar motion *x*ₚ in arcseconds.
    pub polar_x_arcsec: f64,
    /// Polar motion *y*ₚ in arcseconds.
    pub polar_y_arcsec: f64,
}

impl EarthOrientation {
    /// Validates and builds Earth orientation parameters.
    ///
    /// # Errors
    ///
    /// [`AstroError::InvalidTime`] if UT1−UTC is not finite or exceeds ±1 s (leap seconds keep
    /// it within ±0.9 s, so a larger value is almost always milliseconds entered as seconds), and
    /// [`AstroError::InvalidCoordinate`] if a pole coordinate is not finite or exceeds ±1″ (the
    /// pole wanders by a few tenths of an arcsecond; larger values are usually milliarcseconds).
    pub fn new(ut1_minus_utc_s: f64, polar_x_arcsec: f64, polar_y_arcsec: f64) -> Result<Self> {
        if !ut1_minus_utc_s.is_finite() || ut1_minus_utc_s.abs() > 1.0 {
            return Err(AstroError::InvalidTime(format!(
                "UT1-UTC = {ut1_minus_utc_s} s is outside ±1 s. Leap seconds keep UT1-UTC within \
                 ±0.9 s; IERS Bulletin A lists it in seconds (e.g. -0.2273), so a value like -227.3 \
                 was probably entered in milliseconds."
            )));
        }
        for (name, v) in [("x", polar_x_arcsec), ("y", polar_y_arcsec)] {
            if !v.is_finite() || v.abs() > 1.0 {
                return Err(AstroError::InvalidCoordinate(format!(
                    "polar motion {name}p = {v} arcsec is outside ±1 arcsec. The pole wanders by a \
                     few tenths of an arcsecond; a value like 187.5 was probably entered in \
                     milliarcseconds."
                )));
            }
        }
        Ok(Self {
            ut1_minus_utc_s,
            polar_x_arcsec,
            polar_y_arcsec,
        })
    }
}

/// An Earth-centred inertial (or quasi-inertial) frame that can be rotated to ITRF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub enum InertialFrame {
    /// Geocentric Celestial Reference Frame (ICRS axes). CCSDS name `GCRF` (also used for `ICRF`
    /// when the origin is the Earth).
    Gcrf,
    /// Mean equator and equinox of J2000.0 (CCSDS `EME2000`, "J2000"). Differs from GCRF by the
    /// ~23 milliarcsecond frame bias.
    Eme2000,
    /// True Equator, Mean Equinox of date — the output frame of SGP4.
    Teme,
}

// ---------------------------------------------------------------------------------------------
// Time scales
// ---------------------------------------------------------------------------------------------

/// UTC dates (Julian Date at 0h) on which TAI−UTC stepped, and the new value. From the IERS
/// leap-second list (Bulletin C); the latest step is 2017-01-01 (37 s).
#[rustfmt::skip]
const LEAP_SECONDS: [(f64, f64); 28] = [
    (2441317.5, 10.0), (2441499.5, 11.0), (2441683.5, 12.0), (2442048.5, 13.0),
    (2442413.5, 14.0), (2442778.5, 15.0), (2443144.5, 16.0), (2443509.5, 17.0),
    (2443874.5, 18.0), (2444239.5, 19.0), (2444786.5, 20.0), (2445151.5, 21.0),
    (2445516.5, 22.0), (2446247.5, 23.0), (2447161.5, 24.0), (2447892.5, 25.0),
    (2448257.5, 26.0), (2448804.5, 27.0), (2449169.5, 28.0), (2449534.5, 29.0),
    (2450083.5, 30.0), (2450630.5, 31.0), (2451179.5, 32.0), (2453736.5, 33.0),
    (2454832.5, 34.0), (2456109.5, 35.0), (2457204.5, 36.0), (2457754.5, 37.0),
];

/// TAI−UTC in seconds at a UTC instant (the accumulated leap seconds).
///
/// Covers 1972-01-01 onward (10 s, then one second per leap). Earlier dates return 10 s, which is
/// only approximate because UTC used "rubber seconds" before 1972. Leap seconds announced after
/// this release are not known; IERS has announced none after 2017-01-01 as of this writing.
///
/// # Example
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use ephemerust::frames::tai_minus_utc_s;
///
/// assert_eq!(tai_minus_utc_s(Utc.with_ymd_and_hms(2016, 12, 31, 23, 59, 59).unwrap()), 36.0);
/// assert_eq!(tai_minus_utc_s(Utc.with_ymd_and_hms(2017, 1, 1, 0, 0, 0).unwrap()), 37.0);
/// ```
pub fn tai_minus_utc_s(time: DateTime<Utc>) -> f64 {
    let jd = crate::time::julian_date(time);
    LEAP_SECONDS
        .iter()
        .rev()
        .find(|(start, _)| jd >= *start)
        .map_or(10.0, |(_, dat)| *dat)
}

/// TT−UTC in seconds (TAI−UTC + 32.184 s).
pub fn tt_minus_utc_s(time: DateTime<Utc>) -> f64 {
    tai_minus_utc_s(time) + 32.184
}

/// Seconds from J2000.0 (2000-01-01T12:00:00 on the same clock) to `time`, as f64.
fn seconds_since_j2000(time: DateTime<Utc>) -> f64 {
    let j2000 = Utc
        .with_ymd_and_hms(2000, 1, 1, 12, 0, 0)
        .single()
        .expect("valid J2000");
    let d = time - j2000;
    d.num_seconds() as f64 + f64::from(d.subsec_nanos()) * 1e-9
}

// ---------------------------------------------------------------------------------------------
// Rotation helpers (passive rotations, as in SOFA/ERFA)
// ---------------------------------------------------------------------------------------------

type Mat = [[f64; 3]; 3];

fn rx(a: f64) -> Mat {
    let (s, c) = a.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]]
}
fn ry(a: f64) -> Mat {
    let (s, c) = a.sin_cos();
    [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]]
}
fn rz(a: f64) -> Mat {
    let (s, c) = a.sin_cos();
    [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]]
}
fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}
fn apply(m: &Mat, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}
fn transpose(m: &Mat) -> Mat {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

// ---------------------------------------------------------------------------------------------
// Precession, nutation, sidereal time
// ---------------------------------------------------------------------------------------------

include!("frames_nutation_table.rs");

/// IAU 2000B nutation (Δψ, Δε) in radians at `t` Julian centuries of TT since J2000, adjusted to
/// the IAU 2006 precession (the J2-rate factors of SOFA `iauNut06a`).
fn nutation_2000b(t: f64) -> (f64, f64) {
    // Delaunay arguments, linear terms only (as specified for IAU 2000B), in radians.
    let arg = |c0: f64, c1: f64| ((c0 + c1 * t) % 1_296_000.0) * AS2R;
    let el = arg(485_868.249_036, 1_717_915_923.217_8);
    let elp = arg(1_287_104.793_05, 129_596_581.048_1);
    let f = arg(335_779.526_232, 1_739_527_262.847_8);
    let d = arg(1_072_260.703_69, 1_602_961_601.209_0);
    let om = arg(450_160.398_036, -6_962_890.543_1);
    let fa = [el, elp, f, d, om];
    let (mut dpsi, mut deps) = (0.0, 0.0);
    for (mult, lon, obl) in IAU2000B_TERMS.iter().rev() {
        let a: f64 = mult.iter().zip(fa).map(|(&m, x)| f64::from(m) * x).sum();
        let (s, c) = a.sin_cos();
        dpsi += (lon[0] + lon[1] * t) * s + lon[2] * c;
        deps += (obl[0] + obl[1] * t) * c + obl[2] * s;
    }
    // Fixed offsets standing in for the omitted planetary terms (McCarthy & Luzum 2003).
    let dpsi = dpsi * U2R - 0.135e-3 * AS2R;
    let deps = deps * U2R + 0.388e-3 * AS2R;
    // IAU 2006 compatibility: J2 secular variation and the 2000→2006 obliquity-rate change.
    let fj2 = -2.7774e-6 * t;
    (dpsi * (1.0 + 0.4697e-6 + fj2), deps * (1.0 + fj2))
}

/// IAU 2006 Fukushima–Williams precession angles (γ̄, φ̄, ψ̄, ε_A) in radians.
fn fukushima_williams(t: f64) -> (f64, f64, f64, f64) {
    let poly =
        |c: [f64; 6]| (c[0] + (c[1] + (c[2] + (c[3] + (c[4] + c[5] * t) * t) * t) * t) * t) * AS2R;
    (
        poly([
            -0.052_928,
            10.556_378,
            0.493_204_4,
            -0.000_312_38,
            -0.000_002_788,
            0.000_000_026_0,
        ]),
        poly([
            84_381.412_819,
            -46.811_016,
            0.051_126_8,
            0.000_532_89,
            -0.000_000_440,
            -0.000_000_017_6,
        ]),
        poly([
            -0.041_775,
            5_038.481_484,
            1.558_417_5,
            -0.000_185_22,
            -0.000_026_452,
            -0.000_000_014_8,
        ]),
        poly([
            84_381.406,
            -46.836_769,
            -0.000_183_1,
            0.002_003_40,
            -0.000_000_576,
            -0.000_000_043_4,
        ]),
    )
}

/// Combined bias–precession–nutation matrix (GCRF → true equator and equinox of date).
fn npb_matrix(t: f64) -> (Mat, f64, f64) {
    let (gamb, phib, psib, epsa) = fukushima_williams(t);
    let (dpsi, deps) = nutation_2000b(t);
    let m = mul(
        &rx(-(epsa + deps)),
        &mul(&rz(-(psib + dpsi)), &mul(&rx(phib), &rz(gamb))),
    );
    (m, dpsi, epsa)
}

/// Frame bias matrix B (GCRF → EME2000), IERS 2003 values.
fn frame_bias() -> Mat {
    const DPSIBI: f64 = -0.041_775 * AS2R;
    const DEPSBI: f64 = -0.006_819_2 * AS2R;
    const DRA0: f64 = -0.014_6 * AS2R;
    const EPS0: f64 = 84_381.448 * AS2R;
    mul(&rx(-DEPSBI), &mul(&ry(DPSIBI * EPS0.sin()), &rz(DRA0)))
}

/// Earth Rotation Angle (radians) from days of UT1 since J2000.0.
fn earth_rotation_angle(du: f64) -> f64 {
    (TAU * (du.fract() + 0.779_057_273_264_0 + 0.002_737_811_911_354_48 * du)).rem_euclid(TAU)
}

/// Greenwich apparent sidereal time (IAU 2006 GMST + equation of the equinoxes), radians.
fn gast(du: f64, t: f64, dpsi: f64, epsa: f64) -> f64 {
    let gmst = earth_rotation_angle(du)
        + (0.014_506
            + (4_612.156_534
                + (1.391_581_7
                    + (-0.000_000_44 + (-0.000_029_956 - 0.000_000_036_8 * t) * t) * t)
                    * t)
                * t)
            * AS2R;
    // Complementary terms of the equation of the equinoxes (IERS 2010 Table 5.2e, leading terms).
    let fa = |c: [f64; 5]| {
        ((c[0] + (c[1] + (c[2] + (c[3] + c[4] * t) * t) * t) * t) % 1_296_000.0) * AS2R
    };
    let f = fa([
        335_779.526_232,
        1_739_527_262.847_8,
        -12.751_2,
        -0.001_037,
        0.000_004_17,
    ]);
    let d = fa([
        1_072_260.703_69,
        1_602_961_601.209_0,
        -6.370_6,
        0.006_593,
        -0.000_031_69,
    ]);
    let om = fa([
        450_160.398_036,
        -6_962_890.543_1,
        7.472_2,
        0.007_702,
        -0.000_059_39,
    ]);
    let ct = (2_640.96e-6 * om.sin() - 0.39e-6 * om.cos()
        + 63.52e-6 * (2.0 * om).sin()
        + 11.75e-6 * (2.0 * f - 2.0 * d + 3.0 * om).sin()
        + 11.21e-6 * (2.0 * f - 2.0 * d + om).sin()
        - 4.55e-6 * (2.0 * f - 2.0 * d + 2.0 * om).sin()
        - 0.87e-6 * t * om.sin())
        * AS2R;
    (gmst + dpsi * epsa.cos() + ct).rem_euclid(TAU)
}

/// GMST (IAU 1982, as used with TEME by Vallado et al. 2006), radians, from days of UT1 since
/// J2000.0.
fn gmst82(du: f64) -> f64 {
    let t = du / 36_525.0;
    let secs = 24_110.548_41 - 43_200.0 + (8_640_184.812_866 + (0.093_104 - 6.2e-6 * t) * t) * t;
    (TAU * (secs / 86_400.0 + du.fract())).rem_euclid(TAU)
}

/// Polar-motion matrix W (TIRS/PEF → ITRF), including the TIO locator s′.
fn polar_motion(eop: &EarthOrientation, t: f64) -> Mat {
    let sp = -47e-6 * t * AS2R;
    mul(
        &rx(-eop.polar_y_arcsec * AS2R),
        &mul(&ry(-eop.polar_x_arcsec * AS2R), &rz(sp)),
    )
}

/// Inertial → "of date" rotation (the part before polar motion) and the time arguments.
fn celestial_to_terrestrial(
    frame: InertialFrame,
    time: DateTime<Utc>,
    eop: &EarthOrientation,
) -> (Mat, Mat) {
    let s_utc = seconds_since_j2000(time);
    let du = (s_utc + eop.ut1_minus_utc_s) / 86_400.0;
    let t = (s_utc + tt_minus_utc_s(time)) / 86_400.0 / 36_525.0;
    let r_of_date = match frame {
        InertialFrame::Teme => rz(gmst82(du)),
        InertialFrame::Gcrf | InertialFrame::Eme2000 => {
            let (npb, dpsi, epsa) = npb_matrix(t);
            let c = mul(&rz(gast(du, t, dpsi, epsa)), &npb);
            if frame == InertialFrame::Eme2000 {
                // EME2000 → GCRF is Bᵀ.
                mul(&c, &transpose(&frame_bias()))
            } else {
                c
            }
        }
    };
    (r_of_date, polar_motion(eop, t))
}

fn cross_omega(r: [f64; 3]) -> [f64; 3] {
    [-OMEGA_EARTH * r[1], OMEGA_EARTH * r[0], 0.0]
}

/// Rotates a state from an inertial frame to ITRF (Earth-fixed), in any consistent length unit
/// (velocities in that unit per second).
///
/// The velocity includes the Earth-rotation transport term, so the returned ITRF velocity is the
/// velocity *relative to the rotating Earth* — the one that sets range rate and Doppler.
///
/// # Example
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use ephemerust::frames::{EarthOrientation, InertialFrame, inertial_to_itrf, itrf_to_inertial};
///
/// let t = Utc.with_ymd_and_hms(2020, 7, 12, 21, 16, 1).unwrap();
/// let eop = EarthOrientation::new(-0.2273, 0.1875, 0.4171)?;
/// let (r, v) = ([6778.137, 0.0, 0.0], [0.0, 4.763, 6.010]); // km, km/s (GCRF)
/// let (ri, vi) = inertial_to_itrf(InertialFrame::Gcrf, t, r, v, &eop);
/// // Rotations preserve length...
/// let norm = |a: [f64; 3]| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
/// assert!((norm(ri) - norm(r)).abs() < 1e-9);
/// // ...and the round trip returns the original state.
/// let (r2, v2) = itrf_to_inertial(InertialFrame::Gcrf, t, ri, vi, &eop);
/// assert!((0..3).all(|i| (r2[i] - r[i]).abs() < 1e-8 && (v2[i] - v[i]).abs() < 1e-11));
/// # Ok::<(), ephemerust::AstroError>(())
/// ```
pub fn inertial_to_itrf(
    frame: InertialFrame,
    time: DateTime<Utc>,
    r: [f64; 3],
    v: [f64; 3],
    eop: &EarthOrientation,
) -> ([f64; 3], [f64; 3]) {
    let (c, w) = celestial_to_terrestrial(frame, time, eop);
    let r_tirs = apply(&c, r);
    let wr = cross_omega(r_tirs);
    let v_rot = apply(&c, v);
    let v_tirs = [v_rot[0] - wr[0], v_rot[1] - wr[1], v_rot[2] - wr[2]];
    (apply(&w, r_tirs), apply(&w, v_tirs))
}

/// Rotates an ITRF (Earth-fixed) state to an inertial frame: the exact inverse of
/// [`inertial_to_itrf`].
pub fn itrf_to_inertial(
    frame: InertialFrame,
    time: DateTime<Utc>,
    r: [f64; 3],
    v: [f64; 3],
    eop: &EarthOrientation,
) -> ([f64; 3], [f64; 3]) {
    let (c, w) = celestial_to_terrestrial(frame, time, eop);
    let (wt, ct) = (transpose(&w), transpose(&c));
    let r_tirs = apply(&wt, r);
    let v_tirs = apply(&wt, v);
    let wr = cross_omega(r_tirs);
    let v_rot = [v_tirs[0] + wr[0], v_tirs[1] + wr[1], v_tirs[2] + wr[2]];
    (apply(&ct, r_tirs), apply(&ct, v_rot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_bias_matches_sofa() {
        // SOFA/ERFA iauBp00 frame-bias matrix (IERS 2003 bias angles; constant). The IAU 2006
        // variant (iauBp06) differs by ~1e-12, i.e. 0.2 microarcsecond.
        let rb = [
            [
                0.9999999999999942,
                -7.078279744199198e-08,
                8.056217146976134e-08,
            ],
            [
                7.078279477857338e-08,
                0.9999999999999969,
                3.3060414542221364e-08,
            ],
            [
                -8.056217380986972e-08,
                -3.306040883980552e-08,
                0.9999999999999962,
            ],
        ];
        let b = frame_bias();
        for (row_b, row_rb) in b.iter().zip(rb) {
            for (x, y) in row_b.iter().zip(row_rb) {
                assert!((x - y).abs() < 1e-15, "{x} vs {y}");
            }
        }
    }

    #[test]
    fn rotation_matrices_are_orthonormal() {
        let t = Utc.with_ymd_and_hms(2024, 3, 1, 6, 0, 0).unwrap();
        let eop = EarthOrientation::new(0.05, 0.2, 0.3).unwrap();
        for frame in [
            InertialFrame::Gcrf,
            InertialFrame::Eme2000,
            InertialFrame::Teme,
        ] {
            let (c, w) = celestial_to_terrestrial(frame, t, &eop);
            for m in [c, w] {
                let p = mul(&m, &transpose(&m));
                for (i, row) in p.iter().enumerate() {
                    for (j, x) in row.iter().enumerate() {
                        let e = if i == j { 1.0 } else { 0.0 };
                        assert!((x - e).abs() < 1e-14);
                    }
                }
            }
        }
    }

    #[test]
    fn leap_second_boundaries() {
        let t = |y, m, d, h, mi, s| Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap();
        assert_eq!(tai_minus_utc_s(t(1971, 6, 1, 0, 0, 0)), 10.0);
        assert_eq!(tai_minus_utc_s(t(1972, 6, 30, 23, 59, 59)), 10.0);
        assert_eq!(tai_minus_utc_s(t(1972, 7, 1, 0, 0, 0)), 11.0);
        assert_eq!(tai_minus_utc_s(t(2015, 6, 30, 23, 59, 59)), 35.0);
        assert_eq!(tai_minus_utc_s(t(2015, 7, 1, 0, 0, 0)), 36.0);
        assert_eq!(tai_minus_utc_s(t(2026, 10, 11, 0, 0, 0)), 37.0);
        assert!((tt_minus_utc_s(t(2026, 10, 11, 0, 0, 0)) - 69.184).abs() < 1e-12);
    }

    #[test]
    fn eop_validation_catches_unit_mistakes() {
        assert!(EarthOrientation::new(0.9, 0.5, -0.5).is_ok());
        assert!(EarthOrientation::new(-227.3, 0.0, 0.0).is_err());
        assert!(EarthOrientation::new(0.0, 187.5, 0.0).is_err());
        assert!(EarthOrientation::new(f64::NAN, 0.0, 0.0).is_err());
        let msg = EarthOrientation::new(-227.3, 0.0, 0.0)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("milliseconds"), "{msg}");
    }
}
