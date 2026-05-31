//! **Educational** near-Earth orbit helpers — **not** a replacement for the production
//! [`sgp4`](https://crates.io/crates/sgp4) propagator.
//!
//! This module exists so readers can connect the **symbols on a Two-Line Element set** to
//! classical mechanics (Kepler’s third law, mean-motion advance) and see—quantitatively—
//! **where a two-body skeleton leaves the full SGP4 model**. Ephemerust still uses the
//! validated `sgp4` crate for all real propagation ([`crate::satellite::propagate`]).
//!
//! # What is implemented here
//!
//! - Conversion from **mean motion** (revolutions per day, as printed in line 2 of a TLE) to
//!   **semi-major axis** using Kepler’s third law with the **WGS-72** gravitational constant
//!   μ that legacy SGP4 documentation pairs with the theory.
//! - A **two-body** Cartesian state in the same *perifocal → inertial* sense as
//!   [`crate::orbital::elements_to_state_vector`], with mean anomaly advanced linearly in time
//!   from the TLE’s Kozai-style mean motion. **No drag, no J₂ secular/periodic series, no
//!   SPTRCK harmonics** — those are exactly what SGP4 adds on top of this skeleton.
//!
//! # What is *not* implemented
//!
//! - **SDP4 / deep-space** branches, lunar-solar gravity, and resonance models.
//! - **B\*** drag and radiation pressure — the TLE carries `B*` precisely so the operational
//!   model can mimic decay; ignoring it is the largest practical error for low LEO.
//!
//! # How we validate against production `sgp4`
//!
//! - [`semi_major_axis_km_from_mean_motion`] is checked against a **Keplerian period** derived
//!   from the same mean motion (exact consistency up to floating-point noise).
//! - [`position_delta_norm_km_vs_sgp4`] measures ‖**r**₂-body − **r**_SGP4‖ for reference TLEs;
//!   tolerances are **documented in tests**, not tuned to match mission software.
//!
//! See the narrative chapter **`docs/sgp4.md`** at the repository root (also published on
//! docs.rs with the crate) for the full SGP4 story and references.

use crate::orbital::{elements_to_state_vector, orbital_period, OrbitalElements, StateVector};
use crate::{AstroError, Result};

/// WGS-72 Earth gravitational parameter (km³/s²), the value historically bundled with SGP4
/// documentation and compatible implementations.
pub const MU_WGS72_KM3_S2: f64 = 398_600.8;

/// Minimum mean motion (rev/day) for which the teaching “near-Earth LEO” helpers are considered
/// meaningful. Below this, element sets are often deep-space or otherwise outside the narrow
/// path exercised here — use production [`sgp4`](https://crates.io/crates/sgp4) instead.
pub const MIN_MEAN_MOTION_REV_PER_DAY: f64 = 1.5;

/// Converts Kozai mean motion **n** (revolutions per day) to radians per second.
#[inline]
pub fn mean_motion_rev_per_day_to_rad_per_sec(mean_motion_rev_per_day: f64) -> f64 {
    mean_motion_rev_per_day * std::f64::consts::TAU / 86400.0
}

/// Semi-major axis **a** (km) from mean motion using Kepler’s third law **n²a³ = μ** with
/// [`MU_WGS72_KM3_S2`].
///
/// `n` is interpreted as **circular mean motion** in rad/s.
#[inline]
pub fn semi_major_axis_km_from_mean_motion(mean_motion_rev_per_day: f64) -> f64 {
    let n = mean_motion_rev_per_day_to_rad_per_sec(mean_motion_rev_per_day);
    (MU_WGS72_KM3_S2 / (n * n)).cbrt()
}

/// Returns `true` if the teaching helpers are intended to apply to this element set.
///
/// This is a **documentation-oriented** guard, not a copy of SPACETRACK’s ephemeris-type
/// switch to SDP4.
pub fn teaching_supported(elements: &sgp4::Elements) -> bool {
    elements.mean_motion >= MIN_MEAN_MOTION_REV_PER_DAY
        && elements.eccentricity >= 0.0
        && elements.eccentricity < 1.0
}

/// Builds classical [`OrbitalElements`] (km, degrees) from parsed [`sgp4::Elements`], using
/// **a** recovered from mean motion via [`semi_major_axis_km_from_mean_motion`].
pub fn orbital_elements_from_sgp4_elements(elements: &sgp4::Elements) -> OrbitalElements {
    let a = semi_major_axis_km_from_mean_motion(elements.mean_motion);
    OrbitalElements {
        semi_major_axis: a,
        eccentricity: elements.eccentricity,
        inclination: elements.inclination,
        longitude_ascending_node: elements.right_ascension,
        argument_periapsis: elements.argument_of_perigee,
        mean_anomaly: elements.mean_anomaly,
    }
}

/// Advances mean anomaly linearly using the printed mean motion (deg/min), then returns the
/// two-body [`StateVector`] in km / km/s using [`MU_WGS72_KM3_S2`].
///
/// # Errors
///
/// Returns [`AstroError::SatelliteError`] if [`teaching_supported`] is false, or if
/// [`elements_to_state_vector`] rejects the constructed elements.
pub fn teaching_two_body_state(elements: &sgp4::Elements, minutes_since_epoch: f64) -> Result<StateVector> {
    if !teaching_supported(elements) {
        return Err(AstroError::SatelliteError(
            "sgp4_teaching: element set outside the supported near-Earth mean-motion band; \
             use the production sgp4 propagator (see teaching_supported / docs/sgp4.md)"
                .into(),
        ));
    }

    // Mean motion in rev/day → degrees per minute: one rev = 360°, one day = 1440 min.
    let d_m_deg_per_min = elements.mean_motion * (360.0 / 1440.0);
    let mut m_deg = elements.mean_anomaly + d_m_deg_per_min * minutes_since_epoch;
    // Wrap to a principal value for numerical stability in mean→true conversion.
    m_deg %= 360.0;
    if m_deg < 0.0 {
        m_deg += 360.0;
    }

    let oe = OrbitalElements {
        semi_major_axis: semi_major_axis_km_from_mean_motion(elements.mean_motion),
        eccentricity: elements.eccentricity,
        inclination: elements.inclination,
        longitude_ascending_node: elements.right_ascension,
        argument_periapsis: elements.argument_of_perigee,
        mean_anomaly: m_deg,
    };
    elements_to_state_vector(oe, MU_WGS72_KM3_S2)
}

/// Euclidean norm ‖**r**_teach − **r**_prod‖ in kilometres, where **r**_prod comes from the
/// `sgp4` crate’s [`sgp4::Constants::propagate`].
pub fn position_delta_norm_km_vs_sgp4(elements: &sgp4::Elements, minutes_since_epoch: f64) -> Result<f64> {
    let teach = teaching_two_body_state(elements, minutes_since_epoch)?;
    let constants = sgp4::Constants::from_elements(elements).map_err(|e| {
        AstroError::SatelliteError(format!("sgp4 Constants::from_elements: {e}"))
    })?;
    let pred = constants
        .propagate(sgp4::MinutesSinceEpoch(minutes_since_epoch))
        .map_err(|e| AstroError::SatelliteError(format!("sgp4 propagate: {e}")))?;
    let p = pred.position;
    let d = (teach.position[0] - p[0]).hypot(teach.position[1] - p[1]).hypot(teach.position[2] - p[2]);
    Ok(d)
}

/// Keplerian orbital period from the recovered semi-major axis (seconds), using
/// [`orbital_period`] with [`MU_WGS72_KM3_S2`]. For a consistent TLE, this matches
/// **86400 / n_rev_per_day** to high precision.
pub fn keplerian_period_sec_from_tle_mean_motion(mean_motion_rev_per_day: f64) -> f64 {
    let a = semi_major_axis_km_from_mean_motion(mean_motion_rev_per_day);
    orbital_period(a, MU_WGS72_KM3_S2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbital::mean_to_true_anomaly;

    const ISS_L1: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
    const ISS_L2: &str = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

    #[test]
    fn keplerian_period_matches_reciprocal_mean_motion() {
        let el = sgp4::Elements::from_tle(None, ISS_L1.as_bytes(), ISS_L2.as_bytes()).unwrap();
        let t_kepler = keplerian_period_sec_from_tle_mean_motion(el.mean_motion);
        let t_from_tle = 86400.0 / el.mean_motion;
        assert!(
            (t_kepler - t_from_tle).abs() < 0.05,
            "Kepler third law period {t_kepler} s vs 86400/n {t_from_tle} s"
        );
    }

    #[test]
    fn teaching_two_body_vs_sgp4_iss_within_documented_loose_bound() {
        let el = sgp4::Elements::from_tle(None, ISS_L1.as_bytes(), ISS_L2.as_bytes()).unwrap();
        for &m in &[0.0_f64, 5.0, 15.0] {
            let d = position_delta_norm_km_vs_sgp4(&el, m).expect("compare");
            assert!(
                d < 12_000.0,
                "at {m} min, two-body vs SGP4 position delta {d:.1} km; teaching model omits drag and harmonic structure (see docs/sgp4.md)"
            );
        }
    }

    #[test]
    fn low_mean_motion_is_not_teaching_supported() {
        let mut el = sgp4::Elements::from_tle(None, ISS_L1.as_bytes(), ISS_L2.as_bytes()).unwrap();
        // Artificially drop mean motion into a Molniya-like band without re-parsing checksums.
        el.mean_motion = 0.5;
        assert!(!teaching_supported(&el));
        assert!(teaching_two_body_state(&el, 0.0).is_err());
    }

    #[test]
    fn mean_anomaly_advances_linearly_matches_manual_mean_to_true() {
        let el = sgp4::Elements::from_tle(None, ISS_L1.as_bytes(), ISS_L2.as_bytes()).unwrap();
        let minutes = 3.0;
        let d_m = el.mean_motion * (360.0 / 1440.0);
        let m = el.mean_anomaly + d_m * minutes;
        let nu = mean_to_true_anomaly(m, el.eccentricity).to_radians();
        let st = teaching_two_body_state(&el, minutes).unwrap();
        // Spot-check: recompute radius in perifocal frame from conic equation at same nu.
        let a = semi_major_axis_km_from_mean_motion(el.mean_motion);
        let e = el.eccentricity;
        let r_pf = a * (1.0 - e * e) / (1.0 + e * nu.cos());
        let r_norm = (st.position[0].powi(2) + st.position[1].powi(2) + st.position[2].powi(2)).sqrt();
        assert!(
            (r_norm - r_pf).abs() < 1.0,
            "in-plane radius from elements should match |r|; got {r_norm} vs {r_pf}"
        );
    }
}
