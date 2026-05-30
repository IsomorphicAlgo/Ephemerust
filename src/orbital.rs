//! Classical (two-body Keplerian) orbital mechanics.
//!
//! This module works with the six [Keplerian orbital
//! elements](https://en.wikipedia.org/wiki/Orbital_elements) — the compact description of an
//! idealized two-body orbit — and converts them to and from the more directly usable position
//! and velocity vectors. The model assumes a single central body and no perturbations (no
//! drag, no oblateness, no third bodies), which is exact for the two-body problem and a good
//! first approximation for short arcs. For Earth satellites where these perturbations matter,
//! use the [`crate::satellite`] module's SGP4-based propagation instead.

use crate::Result;

/// The six classical [Keplerian orbital
/// elements](https://en.wikipedia.org/wiki/Orbital_elements) that define a two-body orbit and
/// the body's position along it.
///
/// The first two elements fix the orbit's size and shape; the next three orient the orbital
/// plane in space; the last places the body on the orbit at a given instant. Angles are in
/// degrees.
#[derive(Debug, Clone, Copy)]
pub struct OrbitalElements {
    /// Semi-major axis `a`: half the long axis of the ellipse; sets the orbit's size (and,
    /// via Kepler's third law, its period). Same length unit as the result (e.g. km).
    pub semi_major_axis: f64,
    /// Eccentricity `e`: orbit shape, from `0` (circle) toward `1` (parabola). Dimensionless.
    pub eccentricity: f64,
    /// Inclination `i` in degrees: tilt of the orbital plane relative to the reference
    /// (equatorial) plane.
    pub inclination: f64,
    /// Longitude of the ascending node `Ω` in degrees: where the orbit crosses the reference
    /// plane going north, measured from the reference direction.
    pub longitude_ascending_node: f64,
    /// Argument of periapsis `ω` in degrees: angle from the ascending node to periapsis,
    /// measured in the orbital plane.
    pub argument_periapsis: f64,
    /// Mean anomaly `M` in degrees: a *time-like* angle that increases uniformly and locates
    /// the body on the orbit (see [`mean_to_true_anomaly`]).
    pub mean_anomaly: f64,
}

/// A Cartesian orbital state: position and velocity in the same inertial frame.
///
/// This is the alternative to [`OrbitalElements`] for describing an orbit — equivalent
/// information, but expressed as vectors that can be propagated or transformed directly.
#[derive(Debug, Clone, Copy)]
pub struct StateVector {
    /// Position vector `[x, y, z]` in the inertial frame (length unit matches the inputs,
    /// e.g. km).
    pub position: [f64; 3],
    /// Velocity vector `[vx, vy, vz]` in the inertial frame (e.g. km/s).
    pub velocity: [f64; 3],
}

/// Converts a set of [`OrbitalElements`] into a Cartesian [`StateVector`] (position and
/// velocity) in the inertial frame.
///
/// The body is first placed in the *perifocal* frame (the orbital plane, with the x-axis
/// toward periapsis) using the conic equation `r = a(1 − e²) / (1 + e·cos ν)`, then rotated
/// into the inertial frame by the standard 3-1-3 sequence of `Ω`, `i`, and `ω`. The velocity
/// follows from the conserved specific angular momentum `h = √(μ · a(1 − e²))`.
///
/// `gm` is the standard gravitational parameter `μ = G·M` of the central body, in units
/// consistent with the elements (e.g. km³/s² with `a` in km gives km and km/s).
///
/// # Errors
///
/// Returns an error only via the [`Result`] contract; for valid two-body inputs the
/// conversion is purely arithmetic.
///
/// # Example
///
/// ```
/// use ephemerust::orbital::{OrbitalElements, elements_to_state_vector};
///
/// // A circular equatorial orbit at 7000 km, at periapsis (M = 0).
/// let elements = OrbitalElements {
///     semi_major_axis: 7000.0,
///     eccentricity: 0.0,
///     inclination: 0.0,
///     longitude_ascending_node: 0.0,
///     argument_periapsis: 0.0,
///     mean_anomaly: 0.0,
/// };
/// let mu_earth = 398_600.4418; // km^3/s^2
/// let state = elements_to_state_vector(elements, mu_earth).unwrap();
///
/// // At periapsis the body sits on the +x axis at r = a.
/// assert!((state.position[0] - 7000.0).abs() < 1e-6);
/// // Circular speed is sqrt(mu / r).
/// let speed = (state.velocity.iter().map(|v| v * v).sum::<f64>()).sqrt();
/// assert!((speed - (mu_earth / 7000.0).sqrt()).abs() < 1e-6);
/// ```
pub fn elements_to_state_vector(elements: OrbitalElements, gm: f64) -> Result<StateVector> {
    let true_anom_rad = mean_to_true_anomaly(elements.mean_anomaly, elements.eccentricity).to_radians();
    let (a, e) = (elements.semi_major_axis, elements.eccentricity);
    let r = a * (1.0 - e * e) / (1.0 + e * true_anom_rad.cos());
    
    let pos_perifocal = [r * true_anom_rad.cos(), r * true_anom_rad.sin(), 0.0];
    let h = (gm * a * (1.0 - e * e)).sqrt();
    let vel_perifocal = [-(gm / h) * true_anom_rad.sin(), (gm / h) * (e + true_anom_rad.cos()), 0.0];
    
    let (incl, raan, argp) = (
        elements.inclination.to_radians(),
        elements.longitude_ascending_node.to_radians(),
        elements.argument_periapsis.to_radians()
    );
    
    let (cr, sr, ci, si, ca, sa) = (raan.cos(), raan.sin(), incl.cos(), incl.sin(), argp.cos(), argp.sin());
    let (r11, r12, r21, r22, r31, r32) = (
        cr * ca - sr * sa * ci,
        -cr * sa - sr * ca * ci,
        sr * ca + cr * sa * ci,
        -sr * sa + cr * ca * ci,
        sa * si,
        ca * si
    );
    
    Ok(StateVector {
        position: [
            r11 * pos_perifocal[0] + r12 * pos_perifocal[1],
            r21 * pos_perifocal[0] + r22 * pos_perifocal[1],
            r31 * pos_perifocal[0] + r32 * pos_perifocal[1],
        ],
        velocity: [
            r11 * vel_perifocal[0] + r12 * vel_perifocal[1],
            r21 * vel_perifocal[0] + r22 * vel_perifocal[1],
            r31 * vel_perifocal[0] + r32 * vel_perifocal[1],
        ],
    })
}

/// Computes the orbital period from [Kepler's third
/// law](https://en.wikipedia.org/wiki/Kepler%27s_laws_of_planetary_motion), `T = 2π·√(a³/μ)`.
///
/// The period depends only on the semi-major axis and the central body's gravitational
/// parameter — not on eccentricity — so all orbits with the same `a` share a period regardless
/// of shape. The result's time unit follows the inputs (e.g. `a` in km and `μ` in km³/s² give
/// seconds).
///
/// # Example
///
/// ```
/// use ephemerust::orbital::orbital_period;
///
/// // A ~6780 km orbit around Earth completes in roughly 90 minutes.
/// let period_s = orbital_period(6780.0, 398_600.0);
/// assert!((period_s - 5400.0).abs() < 300.0);
/// ```
pub fn orbital_period(semi_major_axis: f64, gm: f64) -> f64 {
    use std::f64::consts::PI;
    2.0 * PI * (semi_major_axis.powi(3) / gm).sqrt()
}

/// Converts the **mean anomaly** to the **true anomaly** (both in degrees) for an elliptical
/// orbit of the given eccentricity.
///
/// The mean anomaly `M` advances uniformly with time but is not a real geometric angle; the
/// true anomaly `ν` is the actual angle from periapsis to the body. Bridging them requires
/// solving [Kepler's equation](https://en.wikipedia.org/wiki/Kepler%27s_equation)
/// `M = E − e·sin E` for the eccentric anomaly `E`, which has no closed-form solution. This
/// uses Newton–Raphson iteration (seeded at `M`, or at `π` for high eccentricity to aid
/// convergence) and then maps `E` to `ν`. The result is normalized to `[0, 360)`.
///
/// # Example
///
/// ```
/// use ephemerust::orbital::mean_to_true_anomaly;
///
/// // For a circle (e = 0), mean and true anomaly coincide.
/// assert!((mean_to_true_anomaly(45.0, 0.0) - 45.0).abs() < 1e-6);
/// // For an eccentric orbit the body lingers near apoapsis, so at M = 90° the
/// // true anomaly has already advanced well past 90°.
/// assert!(mean_to_true_anomaly(90.0, 0.5) > 130.0);
/// ```
pub fn mean_to_true_anomaly(mean_anomaly: f64, eccentricity: f64) -> f64 {
    use std::f64::consts::PI;
    let m_rad = mean_anomaly.to_radians().rem_euclid(2.0 * PI);
    let mut e_anom = if eccentricity < 0.8 { m_rad } else { PI };
    
    for _ in 0..30 {
        let delta = (e_anom - eccentricity * e_anom.sin() - m_rad) / (1.0 - eccentricity * e_anom.cos());
        e_anom -= delta;
        if delta.abs() < 1e-10 { break; }
    }
    
    let true_anom_rad = 2.0 * (((1.0 + eccentricity) / (1.0 - eccentricity)).sqrt() * (e_anom / 2.0).tan()).atan();
    true_anom_rad.to_degrees().rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orbital_period_earth() {
        // Earth's orbit around Sun
        // Semi-major axis: ~149,600,000 km
        // Sun's GM: 1.32712440018e11 km³/s²
        let a = 149_600_000.0;
        let gm_sun = 1.32712440018e11;
        
        let period = orbital_period(a, gm_sun);
        
        // Should be approximately 1 year (31,557,600 seconds)
        let one_year_seconds = 365.25 * 24.0 * 3600.0;
        assert!((period - one_year_seconds).abs() < 100_000.0, 
                "Earth's orbital period should be ~1 year, got {} seconds", period);
    }

    #[test]
    fn test_orbital_period_iss() {
        // ISS orbit around Earth
        // Semi-major axis: ~6,780 km (Earth radius ~6,371 km + altitude ~410 km)
        // Earth's GM: 398,600 km³/s²
        let a = 6780.0;
        let gm_earth = 398_600.0;
        
        let period = orbital_period(a, gm_earth);
        
        // ISS orbital period is about 90 minutes (5,400 seconds)
        assert!((period - 5400.0).abs() < 300.0, 
                "ISS orbital period should be ~90 minutes, got {} seconds", period);
    }

    #[test]
    fn test_mean_to_true_anomaly_circular() {
        // For circular orbit (e=0), mean anomaly = true anomaly
        let mean_anom = 45.0;
        let e = 0.0;
        
        let true_anom = mean_to_true_anomaly(mean_anom, e);
        
        assert!((true_anom - mean_anom).abs() < 0.01, 
                "Circular orbit: true anomaly should equal mean anomaly, got {} vs {}", true_anom, mean_anom);
    }

    #[test]
    fn test_mean_to_true_anomaly_eccentric() {
        // Test with eccentric orbit (e=0.5)
        let mean_anom = 90.0;
        let e = 0.5;
        
        let true_anom = mean_to_true_anomaly(mean_anom, e);
        
        // For e=0.5 and M=90°, true anomaly should be > 90° (about 140°)
        // This is because for eccentric orbits, the object spends more time far from periapsis
        assert!(true_anom > 130.0 && true_anom < 150.0, 
                "True anomaly for e=0.5, M=90° should be ~140°, got {}", true_anom);
    }

    #[test]
    fn test_mean_to_true_anomaly_range() {
        // Test that output is always in valid range (0-360)
        let test_cases = vec![0.0, 90.0, 180.0, 270.0, 360.0, 450.0];
        let e = 0.3;
        
        for mean_anom in test_cases {
            let true_anom = mean_to_true_anomaly(mean_anom, e);
            assert!((0.0..360.0).contains(&true_anom), 
                    "True anomaly should be in range [0, 360), got {}", true_anom);
        }
    }

    #[test]
    fn test_elements_to_state_vector_circular() {
        // Test circular orbit
        let elements = OrbitalElements {
            semi_major_axis: 7000.0,  // km
            eccentricity: 0.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_periapsis: 0.0,
            mean_anomaly: 0.0,  // At periapsis (which equals apoapsis for circular)
        };
        let gm = 398_600.0;  // Earth's GM
        
        let state = elements_to_state_vector(elements, gm).unwrap();
        
        // Position should be approximately [7000, 0, 0] for mean anomaly = 0
        assert!((state.position[0] - 7000.0).abs() < 1.0, "X position should be ~7000 km");
        assert!(state.position[1].abs() < 1.0, "Y position should be ~0");
        assert!(state.position[2].abs() < 1.0, "Z position should be ~0");
        
        // For circular orbit, velocity magnitude should be sqrt(gm/a)
        let vel_mag = (state.velocity[0].powi(2) + state.velocity[1].powi(2) + state.velocity[2].powi(2)).sqrt();
        let expected_vel = (gm / 7000.0).sqrt();
        assert!((vel_mag - expected_vel).abs() < 0.1, 
                "Velocity magnitude should be {} km/s, got {}", expected_vel, vel_mag);
    }

    #[test]
    fn test_elements_to_state_vector_energy() {
        // Test that specific orbital energy is conserved
        // ε = v²/2 - μ/r = -μ/(2a)
        let elements = OrbitalElements {
            semi_major_axis: 8000.0,
            eccentricity: 0.2,
            inclination: 30.0,
            longitude_ascending_node: 45.0,
            argument_periapsis: 60.0,
            mean_anomaly: 120.0,
        };
        let gm = 398_600.0;
        
        let state = elements_to_state_vector(elements, gm).unwrap();
        
        // Calculate specific energy
        let r_mag = (state.position[0].powi(2) + state.position[1].powi(2) + state.position[2].powi(2)).sqrt();
        let v_mag = (state.velocity[0].powi(2) + state.velocity[1].powi(2) + state.velocity[2].powi(2)).sqrt();
        let energy = v_mag.powi(2) / 2.0 - gm / r_mag;
        let expected_energy = -gm / (2.0 * elements.semi_major_axis);
        
        assert!((energy - expected_energy).abs() < 1.0, 
                "Specific energy should be {} km²/s², got {}", expected_energy, energy);
    }
}
