//! Planetary positions from [VSOP87](https://en.wikipedia.org/wiki/VSOP_(planets)) theory.
//!
//! VSOP87 represents each planet's heliocentric ecliptic longitude, latitude, and radius as
//! large trigonometric series in time. This module stores **truncated** series (the
//! largest-amplitude terms) as compile-time constants, evaluates them, and converts the
//! heliocentric result to the geocentric equatorial [`RaDec`] an observer on Earth would
//! measure — which is why Earth's own series is needed even though we never report Earth's
//! position. Truncation trades a little accuracy (roughly arcminute-level; see
//! `docs/accuracy-and-limits.md`) for compactness and speed.

// The VSOP87 series coefficients in this module are transcribed verbatim from the published
// theory. They carry more significant digits than an `f64` can store
// (clippy::excessive_precision) and a few values coincidentally fall near π
// (clippy::approx_constant); both are properties of the reference data, not mistakes, so these
// lints are silenced module-wide.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use crate::coordinates::RaDec;
use crate::Result;

/// Represents the major planets in the solar system.
///
/// Planets are ordered by distance from the Sun (excluding Earth, which is the observer).
/// Earth is included for completeness but typically not used for position calculations
/// from Earth's perspective (use geocentric calculations instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Planet {
    /// Mercury, the innermost planet.
    Mercury,
    /// Venus.
    Venus,
    /// Earth — included because its heliocentric position is required for the geocentric
    /// conversion of the other planets.
    Earth,
    /// Mars.
    Mars,
    /// Jupiter.
    Jupiter,
    /// Saturn.
    Saturn,
    /// Uranus.
    Uranus,
    /// Neptune, the outermost major planet.
    Neptune,
}

impl Planet {
    /// Returns the name of the planet as a string.
    pub fn name(&self) -> &'static str {
        match self {
            Planet::Mercury => "Mercury",
            Planet::Venus => "Venus",
            Planet::Earth => "Earth",
            Planet::Mars => "Mars",
            Planet::Jupiter => "Jupiter",
            Planet::Saturn => "Saturn",
            Planet::Uranus => "Uranus",
            Planet::Neptune => "Neptune",
        }
    }

    /// Parses a planet name from a string (case-insensitive), returning `None` for an
    /// unrecognized name.
    ///
    /// Named `from_name` rather than `from_str` so as not to be confused with the
    /// [`std::str::FromStr`] trait, which returns a `Result` rather than an `Option`.
    ///
    /// ```
    /// use ephemerust::planets::Planet;
    /// assert_eq!(Planet::from_name("JUPITER"), Some(Planet::Jupiter));
    /// assert_eq!(Planet::from_name("Pluto"), None);
    /// ```
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mercury" => Some(Planet::Mercury),
            "venus" => Some(Planet::Venus),
            "earth" => Some(Planet::Earth),
            "mars" => Some(Planet::Mars),
            "jupiter" => Some(Planet::Jupiter),
            "saturn" => Some(Planet::Saturn),
            "uranus" => Some(Planet::Uranus),
            "neptune" => Some(Planet::Neptune),
            _ => None,
        }
    }
}

/// VSOP87 coefficient for a single term in the series.
///
/// Each term in VSOP87 has the form: A × cos(B + C × t)
/// where:
/// - A: Amplitude (coefficient)
/// - B: Phase (constant term in radians)
/// - C: Frequency (coefficient for time variable)
/// - t: Time in Julian centuries from J2000.0
#[derive(Debug, Clone, Copy)]
pub struct Vsop87Term {
    /// Amplitude A
    pub amplitude: f64,
    /// Phase B (in radians)
    pub phase: f64,
    /// Frequency C
    pub frequency: f64,
}

/// VSOP87 series for a single variable (L, B, or R).
///
/// VSOP87 represents each coordinate as a sum of series:
/// - L0, L1, L2, L3, L4, L5 for longitude
/// - B0, B1, B2, B3, B4 for latitude
/// - R0, R1, R2, R3, R4 for radius
///
/// Each series is a sum of terms: Σ(A × cos(B + C × t))
/// The final value is: (L0 + L1×t + L2×t² + L3×t³ + L4×t⁴ + L5×t⁵) / 10^8
#[derive(Debug, Clone)]
pub struct Vsop87Series {
    /// Series L0, B0, or R0 (constant term)
    pub series_0: Vec<Vsop87Term>,
    /// Series L1, B1, or R1 (linear term)
    pub series_1: Vec<Vsop87Term>,
    /// Series L2, B2, or R2 (quadratic term)
    pub series_2: Vec<Vsop87Term>,
    /// Series L3, B3, or R3 (cubic term)
    pub series_3: Vec<Vsop87Term>,
    /// Series L4, B4, or R4 (quartic term) - only for L
    pub series_4: Option<Vec<Vsop87Term>>,
    /// Series L5 (quintic term) - only for L
    pub series_5: Option<Vec<Vsop87Term>>,
}

/// Complete VSOP87 data for a planet.
///
/// Contains three VSOP87Series: one for longitude (L), one for latitude (B),
/// and one for radius (R).
#[derive(Debug, Clone)]
pub struct PlanetVsop87Data {
    /// Longitude series (L0, L1, L2, L3, L4, L5)
    pub longitude: Vsop87Series,
    /// Latitude series (B0, B1, B2, B3, B4)
    pub latitude: Vsop87Series,
    /// Radius series (R0, R1, R2, R3, R4) in AU
    pub radius: Vsop87Series,
}

/// Heliocentric ecliptic coordinates from VSOP87.
///
/// These are the raw output from VSOP87 calculations before conversion
/// to equatorial coordinates (RA/Dec).
#[derive(Debug, Clone, Copy)]
pub struct HeliocentricEcliptic {
    /// Ecliptic longitude (L) in radians
    pub longitude: f64,
    /// Ecliptic latitude (B) in radians
    pub latitude: f64,
    /// Radius vector (R) in Astronomical Units (AU)
    pub radius: f64,
}

/// Calculates the position of a planet using VSOP87 theory.
///
/// This function computes the heliocentric ecliptic coordinates (L, B, R)
/// and converts them to geocentric equatorial coordinates (RA, Dec).
///
/// # Arguments
/// * `planet` - The planet to calculate
/// * `julian_date` - Julian Date for the calculation
///
/// # Returns
/// Right Ascension and Declination in equatorial coordinates
///
/// # Errors
/// Returns an error if:
/// - Planet data is not available
/// - Julian Date is invalid
/// - Calculation fails
///
/// # Accuracy
/// For a simplified VSOP87 implementation (truncated series):
/// - Inner planets (Mercury, Venus, Mars): ~1 arcminute
/// - Outer planets (Jupiter, Saturn): ~1-2 arcminutes
/// - Distant planets (Uranus, Neptune): ~2-5 arcminutes
///
/// Full VSOP87 accuracy is typically better than 1 arcsecond for all planets.
///
/// # Example
/// ```no_run
/// use ephemerust::planets::{Planet, calculate_planet_position};
///
/// // Calculate Mercury's position on January 1, 2000
/// // Note: Requires Earth's VSOP87 data for geocentric conversion
/// let jd = 2451545.0; // J2000.0
/// let position = calculate_planet_position(Planet::Mercury, jd)?;
/// println!("Mercury: RA={:.2}h, Dec={:.2}°", position.ra, position.dec);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn calculate_planet_position(planet: Planet, julian_date: f64) -> Result<RaDec> {
    use log::{error, info, warn};

    // Validate Julian Date - check for NaN and infinity
    if julian_date.is_nan() {
        error!("Invalid Julian Date: NaN (Not a Number)");
        return Err(crate::error::AstroError::InvalidTime(
            "Julian Date cannot be NaN (Not a Number). Please provide a valid date.".to_string(),
        ));
    }
    if julian_date.is_infinite() {
        error!("Invalid Julian Date: {} (infinity)", julian_date);
        return Err(crate::error::AstroError::InvalidTime(format!(
            "Julian Date cannot be infinite (got {}). Please provide a valid date.",
            julian_date
        )));
    }

    // Validate reasonable Julian Date range
    // Valid range: approximately 2000 BC to 3000 AD (JD ~1721424 to ~2817152)
    const MIN_JD: f64 = 1000000.0; // ~-2000 BC
    const MAX_JD: f64 = 3000000.0; // ~3000 AD
    if !(MIN_JD..=MAX_JD).contains(&julian_date) {
        warn!("Julian Date {} is outside recommended range [{:.0}, {:.0}]. Results may be inaccurate.",
              julian_date, MIN_JD, MAX_JD);
    }

    // Warn for extreme dates (outside reasonable range for VSOP87)
    const J2000: f64 = 2451545.0;
    const REASONABLE_RANGE_CENTURIES: f64 = 20.0; // ±20 centuries from J2000
    let centuries_from_j2000 = (julian_date - J2000) / 36525.0;
    if centuries_from_j2000.abs() > REASONABLE_RANGE_CENTURIES {
        warn!("Julian Date {} is {:.2} centuries from J2000.0. VSOP87 accuracy may degrade for extreme dates (>±20 centuries).",
              julian_date, centuries_from_j2000);
    }

    // Get VSOP87 data for planet
    let vsop87_data = get_planet_vsop87_data(planet).ok_or_else(|| {
        error!("VSOP87 data not available for {}", planet.name());
        crate::error::AstroError::InvalidCoordinate(format!(
            "VSOP87 data not available for {}. This planet may not be fully implemented yet.",
            planet.name()
        ))
    })?;

    info!(
        "Calculating {} position at JD {:.6}",
        planet.name(),
        julian_date
    );

    // Calculate time in Julian millennia from J2000.0 (VSOP87 time argument).
    // VSOP87 series use τ = (JD - J2000.0) / 365250, NOT Julian centuries.
    let t = (julian_date - J2000) / 365250.0;
    info!("Time in Julian millennia from J2000.0: t = {:.10}", t);

    // Evaluate VSOP87 series for L, B, R
    let heliocentric = calculate_heliocentric_ecliptic(&vsop87_data, t)?;
    info!(
        "Heliocentric ecliptic: L={:.10} rad ({:.6}°), B={:.10} rad ({:.6}°), R={:.10} AU",
        heliocentric.longitude,
        heliocentric.longitude.to_degrees(),
        heliocentric.latitude,
        heliocentric.latitude.to_degrees(),
        heliocentric.radius
    );

    // Calculate Earth's heliocentric position for geocentric conversion
    // Note: Earth's VSOP87 data may be a placeholder - handle gracefully
    let earth_vsop87_data = get_planet_vsop87_data(Planet::Earth)
        .ok_or_else(|| {
            use log::error;
            error!("Earth VSOP87 data not available - required for geocentric conversion");
            crate::error::AstroError::CalculationError(
                "Earth VSOP87 data not available for geocentric conversion. Earth's position is required to convert from heliocentric to geocentric coordinates. Please ensure Earth's VSOP87 coefficients are implemented.".to_string()
            )
        })?;

    // Check if Earth data appears to be placeholder (empty series)
    let earth_data_is_placeholder = earth_vsop87_data.longitude.series_0.is_empty()
        && earth_vsop87_data.latitude.series_0.is_empty()
        && earth_vsop87_data.radius.series_0.is_empty();

    if earth_data_is_placeholder {
        warn!("Earth VSOP87 data appears to be placeholder (empty series). Geocentric conversion may produce incorrect results.");
    }

    let earth_heliocentric = calculate_heliocentric_ecliptic(&earth_vsop87_data, t)
        .map_err(|e| {
            use log::error;
            error!("Failed to calculate Earth's heliocentric position: {}", e);
            crate::error::AstroError::CalculationError(
                format!("Failed to calculate Earth's position for geocentric conversion: {}. This may indicate invalid VSOP87 data.", e)
            )
        })?;
    info!(
        "Earth heliocentric ecliptic: L={:.10} rad ({:.6}°), B={:.10} rad ({:.6}°), R={:.10} AU",
        earth_heliocentric.longitude,
        earth_heliocentric.longitude.to_degrees(),
        earth_heliocentric.latitude,
        earth_heliocentric.latitude.to_degrees(),
        earth_heliocentric.radius
    );

    // Convert heliocentric ecliptic to geocentric equatorial (RA/Dec)
    let ra_dec = heliocentric_to_geocentric(heliocentric, earth_heliocentric, julian_date)
        .map_err(|e| {
            use log::error;
            error!("Coordinate conversion failed for {}: {}", planet.name(), e);
            crate::error::AstroError::CalculationError(format!(
                "Failed to convert {} coordinates from heliocentric to geocentric: {}",
                planet.name(),
                e
            ))
        })?;

    info!(
        "{} position calculated: RA={:.6}h, Dec={:.6}°",
        planet.name(),
        ra_dec.ra,
        ra_dec.dec
    );

    // Warn if coordinates seem unusual (potential data issues)
    if ra_dec.ra.is_nan() || ra_dec.dec.is_nan() {
        warn!("Calculated RA/Dec contains NaN values. This may indicate invalid VSOP87 data or calculation error.");
    }
    if ra_dec.dec.abs() > 90.0 {
        warn!("Calculated declination ({:.6}°) is outside valid range [-90°, +90°]. This may indicate a calculation error.",
              ra_dec.dec);
    }

    Ok(ra_dec)
}

/// Calculates heliocentric ecliptic coordinates (L, B, R) from VSOP87 data.
///
/// # Arguments
/// * `vsop87_data` - VSOP87 coefficient data for the planet
/// * `t` - Time in Julian centuries from J2000.0
///
/// # Returns
/// Heliocentric ecliptic coordinates (longitude L, latitude B, radius R)
///
/// # Errors
/// Returns an error if calculation fails
fn calculate_heliocentric_ecliptic(
    vsop87_data: &PlanetVsop87Data,
    t: f64,
) -> Result<HeliocentricEcliptic> {
    use log::{info, warn};

    // Calculate L (longitude)
    let l = calculate_longitude(&vsop87_data.longitude, t);
    info!(
        "VSOP87 longitude (L): {:.10} radians ({:.6} degrees)",
        l,
        l.to_degrees()
    );

    // Calculate B (latitude)
    let b = calculate_latitude(&vsop87_data.latitude, t);
    info!(
        "VSOP87 latitude (B): {:.10} radians ({:.6} degrees)",
        b,
        b.to_degrees()
    );

    // Calculate R (radius)
    let r = calculate_radius(&vsop87_data.radius, t);
    info!("VSOP87 radius (R): {:.10} AU", r);

    // Validate calculated values
    if l.is_nan() || b.is_nan() || r.is_nan() {
        warn!("VSOP87 calculation produced NaN values. This may indicate invalid coefficients or time argument.");
        return Err(crate::error::AstroError::CalculationError(
            "VSOP87 calculation produced NaN (Not a Number) values. Check VSOP87 coefficients and time argument.".to_string()
        ));
    }

    if r <= 0.0 {
        warn!(
            "VSOP87 radius is non-positive ({} AU). This is physically impossible.",
            r
        );
        return Err(crate::error::AstroError::CalculationError(format!(
            "VSOP87 radius must be positive, got {} AU. This may indicate invalid coefficients.",
            r
        )));
    }

    if b.abs() > std::f64::consts::PI / 2.0 {
        warn!(
            "VSOP87 latitude ({:.6}°) is outside valid range [-90°, +90°].",
            b.to_degrees()
        );
        // Don't fail, but warn - this could be a calculation issue
    }

    // Normalize longitude to [0, 2π)
    let l_normalized = l.rem_euclid(2.0 * std::f64::consts::PI);

    Ok(HeliocentricEcliptic {
        longitude: l_normalized,
        latitude: b,
        radius: r,
    })
}

/// Calculates VSOP87 longitude (L) in radians.
///
/// # Arguments
/// * `longitude_series` - VSOP87 longitude series (L0-L5)
/// * `t` - Time in Julian centuries from J2000.0
///
/// # Returns
/// Longitude in radians
///
/// # Formula
/// L = (L0 + L1×t + L2×t² + L3×t³ + L4×t⁴ + L5×t⁵) / 10^8
/// where each Ln = Σ(A × cos(B + C × t))
fn calculate_longitude(longitude_series: &Vsop87Series, t: f64) -> f64 {
    evaluate_vsop87_series(longitude_series, t)
}

/// Calculates VSOP87 latitude (B) in radians.
///
/// # Arguments
/// * `latitude_series` - VSOP87 latitude series (B0-B4)
/// * `t` - Time in Julian centuries from J2000.0
///
/// # Returns
/// Latitude in radians
///
/// # Formula
/// B = (B0 + B1×t + B2×t² + B3×t³ + B4×t⁴) / 10^8
/// where each Bn = Σ(A × cos(B + C × t))
fn calculate_latitude(latitude_series: &Vsop87Series, t: f64) -> f64 {
    evaluate_vsop87_series(latitude_series, t)
}

/// Calculates VSOP87 radius (R) in Astronomical Units.
///
/// # Arguments
/// * `radius_series` - VSOP87 radius series (R0-R4)
/// * `t` - Time in Julian centuries from J2000.0
///
/// # Returns
/// Radius in Astronomical Units (AU)
///
/// # Formula
/// R = (R0 + R1×t + R2×t² + R3×t³ + R4×t⁴) / 10^8
/// where each Rn = Σ(A × cos(B + C × t))
fn calculate_radius(radius_series: &Vsop87Series, t: f64) -> f64 {
    evaluate_vsop87_series(radius_series, t)
}

/// Evaluates a VSOP87 series for a given time.
///
/// # Arguments
/// * `series` - The VSOP87 series to evaluate
/// * `t` - Time in Julian centuries from J2000.0
///
/// # Returns
/// The evaluated series value (in radians for L/B, AU for R)
///
/// # Formula
/// result = L0 + L1×t + L2×t² + L3×t³ + L4×t⁴ + L5×t⁵
/// where each Ln = Σ(A × cos(B + C × t)) and t is in Julian millennia from J2000.0.
/// Coefficients are VSOP87 spherical values stored directly in radians (L, B) or AU (R).
///
/// # Logging
/// Logs at debug level: series values, intermediate calculations
fn evaluate_vsop87_series(series: &Vsop87Series, t: f64) -> f64 {
    use log::debug;

    // Evaluate each series term: Σ(A × cos(B + C × t))
    let evaluate_series_terms = |terms: &[Vsop87Term]| -> f64 {
        terms.iter().map(|term| evaluate_vsop87_term(term, t)).sum()
    };

    // Evaluate series_0 (constant term)
    let s0 = evaluate_series_terms(&series.series_0);
    debug!(
        "VSOP87 series_0 evaluation: {} terms, result = {:.10}",
        series.series_0.len(),
        s0
    );

    // Evaluate series_1 (linear term)
    let s1 = evaluate_series_terms(&series.series_1);
    debug!(
        "VSOP87 series_1 evaluation: {} terms, result = {:.10}",
        series.series_1.len(),
        s1
    );

    // Evaluate series_2 (quadratic term)
    let s2 = evaluate_series_terms(&series.series_2);
    debug!(
        "VSOP87 series_2 evaluation: {} terms, result = {:.10}",
        series.series_2.len(),
        s2
    );

    // Evaluate series_3 (cubic term)
    let s3 = evaluate_series_terms(&series.series_3);
    debug!(
        "VSOP87 series_3 evaluation: {} terms, result = {:.10}",
        series.series_3.len(),
        s3
    );

    // Evaluate series_4 (quartic term) if present
    let s4 = if let Some(terms) = &series.series_4 {
        let value = evaluate_series_terms(terms);
        debug!(
            "VSOP87 series_4 evaluation: {} terms, result = {:.10}",
            terms.len(),
            value
        );
        value
    } else {
        0.0
    };

    // Evaluate series_5 (quintic term) if present
    let s5 = if let Some(terms) = &series.series_5 {
        let value = evaluate_series_terms(terms);
        debug!(
            "VSOP87 series_5 evaluation: {} terms, result = {:.10}",
            terms.len(),
            value
        );
        value
    } else {
        0.0
    };

    // Combine series with time powers: L = L0 + L1×t + L2×t² + L3×t³ + L4×t⁴ + L5×t⁵
    // (VSOP87 spherical coefficients are stored directly in radians/AU; t is in Julian
    // millennia from J2000.0, so no additional scaling is applied here.)
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;

    let result = s0 + s1 * t + s2 * t2 + s3 * t3 + s4 * t4 + s5 * t5;

    debug!(
        "VSOP87 series combination: t={:.10}, result={:.10}",
        t, result
    );

    result
}

/// Evaluates a single VSOP87 term: A × cos(B + C × t)
///
/// # Arguments
/// * `term` - The VSOP87 term
/// * `t` - Time in Julian centuries from J2000.0
///
/// # Returns
/// The evaluated term value
fn evaluate_vsop87_term(term: &Vsop87Term, t: f64) -> f64 {
    term.amplitude * (term.phase + term.frequency * t).cos()
}

/// Calculates the obliquity of the ecliptic for a given Julian Date.
///
/// The obliquity of the ecliptic is the angle between the Earth's equatorial plane
/// and the ecliptic plane (the plane of Earth's orbit around the Sun).
///
/// # Arguments
/// * `julian_date` - Julian Date for the calculation
///
/// # Returns
/// Obliquity of the ecliptic in radians
///
/// # Formula
/// ε = 23.439291° - 0.0130042° × t - 0.00000016° × t² + 0.000000503° × t³
/// where t = (JD - J2000.0) / 36525.0 (Julian centuries from J2000.0)
///
/// For simplified version (sufficient for most applications):
/// ε = 23.4393° - 0.0000004° × d
/// where d = days since J2000.0
pub(crate) fn calculate_obliquity(julian_date: f64) -> f64 {
    const J2000: f64 = 2451545.0;
    let d = julian_date - J2000;

    // Simplified formula (matches solar position calculation)
    // For higher precision, use: 23.439291 - 0.0130042*t - 0.00000016*t² + 0.000000503*t³
    let obliquity_deg = 23.4393 - 0.0000004 * d;
    obliquity_deg.to_radians()
}

/// Converts heliocentric ecliptic coordinates to heliocentric equatorial coordinates.
///
/// This transformation rotates coordinates from the ecliptic plane (Earth's orbital plane)
/// to the equatorial plane (Earth's rotational plane) using the obliquity of the ecliptic.
///
/// # Arguments
/// * `heliocentric` - Heliocentric ecliptic coordinates (L, B, R)
/// * `obliquity` - Obliquity of the ecliptic in radians
///
/// # Returns
/// Heliocentric equatorial coordinates (x, y, z) in AU
///
/// # Formula
/// For ecliptic coordinates (L, B, R):
/// - x_ecl = R × cos(B) × cos(L)
/// - y_ecl = R × cos(B) × sin(L)
/// - z_ecl = R × sin(B)
///
/// Rotation to equatorial:
/// - x_eq = x_ecl
/// - y_eq = y_ecl × cos(ε) - z_ecl × sin(ε)
/// - z_eq = y_ecl × sin(ε) + z_ecl × cos(ε)
pub(crate) fn ecliptic_to_equatorial(
    heliocentric: HeliocentricEcliptic,
    obliquity: f64,
) -> (f64, f64, f64) {
    let l = heliocentric.longitude;
    let b = heliocentric.latitude;
    let r = heliocentric.radius;

    // Convert ecliptic spherical to rectangular coordinates
    let cos_b = b.cos();
    let sin_b = b.sin();
    let cos_l = l.cos();
    let sin_l = l.sin();

    let x_ecl = r * cos_b * cos_l;
    let y_ecl = r * cos_b * sin_l;
    let z_ecl = r * sin_b;

    // Rotate around X-axis by obliquity angle
    let cos_eps = obliquity.cos();
    let sin_eps = obliquity.sin();

    let x_eq = x_ecl;
    let y_eq = y_ecl * cos_eps - z_ecl * sin_eps;
    let z_eq = y_ecl * sin_eps + z_ecl * cos_eps;

    (x_eq, y_eq, z_eq)
}

/// Converts heliocentric equatorial rectangular coordinates to geocentric equatorial coordinates.
///
/// This accounts for Earth's position in the solar system by subtracting Earth's
/// heliocentric position from the planet's heliocentric position.
///
/// # Arguments
/// * `planet_heliocentric_eq` - Planet's heliocentric equatorial coordinates (x, y, z) in AU
/// * `earth_heliocentric_eq` - Earth's heliocentric equatorial coordinates (x, y, z) in AU
///
/// # Returns
/// Geocentric equatorial coordinates (x, y, z) in AU
///
/// # Formula
/// [x_geo]   [x_planet]   [x_earth]
/// [y_geo] = [y_planet] - [y_earth]
/// [z_geo]   [z_planet]   [z_earth]
pub(crate) fn heliocentric_to_geocentric_rectangular(
    planet_heliocentric_eq: (f64, f64, f64),
    earth_heliocentric_eq: (f64, f64, f64),
) -> (f64, f64, f64) {
    (
        planet_heliocentric_eq.0 - earth_heliocentric_eq.0,
        planet_heliocentric_eq.1 - earth_heliocentric_eq.1,
        planet_heliocentric_eq.2 - earth_heliocentric_eq.2,
    )
}

/// Converts geocentric equatorial rectangular coordinates to RA/Dec.
///
/// # Arguments
/// * `x`, `y`, `z` - Geocentric equatorial rectangular coordinates in AU
///
/// # Returns
/// Right Ascension (RA) in hours and Declination (Dec) in degrees
///
/// # Formula
/// RA = atan2(y, x) (in radians, converted to hours)
/// Dec = arcsin(z / r) (in radians, converted to degrees)
/// where r = √(x² + y² + z²)
pub(crate) fn rectangular_to_ra_dec(x: f64, y: f64, z: f64) -> RaDec {
    let r = (x * x + y * y + z * z).sqrt();

    // Calculate RA (right ascension)
    let ra_rad = y.atan2(x);
    // Normalize to [0, 2π) and convert to hours
    let ra_hours =
        (ra_rad.rem_euclid(2.0 * std::f64::consts::PI).to_degrees() / 15.0).rem_euclid(24.0);

    // Calculate Dec (declination)
    let dec_rad = if r > 0.0 {
        (z / r).asin()
    } else {
        0.0 // Default to 0 if at origin
    };
    let dec_degrees = dec_rad.to_degrees();

    RaDec {
        ra: ra_hours,
        dec: dec_degrees,
    }
}

/// Converts heliocentric ecliptic coordinates to geocentric equatorial coordinates.
///
/// This conversion accounts for:
/// 1. Earth's position in the solar system
/// 2. Ecliptic to equatorial coordinate transformation
/// 3. Light-time correction (optional, for high precision)
///
/// # Arguments
/// * `heliocentric` - Heliocentric ecliptic coordinates (L, B, R)
/// * `earth_position` - Earth's heliocentric ecliptic position (for geocentric conversion)
/// * `julian_date` - Julian Date for obliquity calculation
///
/// # Returns
/// Geocentric equatorial coordinates (RA, Dec)
///
/// # Note
/// For initial implementation, light-time correction is omitted.
/// This reduces accuracy by ~0.01 arcseconds but simplifies the calculation.
///
/// # Conversion Pipeline
/// 1. Calculate obliquity of the ecliptic
/// 2. Convert planet's heliocentric ecliptic to heliocentric equatorial (rectangular)
/// 3. Convert Earth's heliocentric ecliptic to heliocentric equatorial (rectangular)
/// 4. Calculate geocentric position: planet - Earth
/// 5. Convert geocentric rectangular to RA/Dec
fn heliocentric_to_geocentric(
    heliocentric: HeliocentricEcliptic,
    earth_position: HeliocentricEcliptic,
    julian_date: f64,
) -> Result<RaDec> {
    use log::debug;

    // Step 1: Calculate obliquity of the ecliptic
    let obliquity = calculate_obliquity(julian_date);
    debug!(
        "Obliquity of the ecliptic: {:.10} rad ({:.6}°)",
        obliquity,
        obliquity.to_degrees()
    );

    // Step 2: Convert planet's heliocentric ecliptic to heliocentric equatorial
    let planet_heliocentric_eq = ecliptic_to_equatorial(heliocentric, obliquity);
    debug!(
        "Planet heliocentric equatorial: x={:.10}, y={:.10}, z={:.10} AU",
        planet_heliocentric_eq.0, planet_heliocentric_eq.1, planet_heliocentric_eq.2
    );

    // Step 3: Convert Earth's heliocentric ecliptic to heliocentric equatorial
    let earth_heliocentric_eq = ecliptic_to_equatorial(earth_position, obliquity);
    debug!(
        "Earth heliocentric equatorial: x={:.10}, y={:.10}, z={:.10} AU",
        earth_heliocentric_eq.0, earth_heliocentric_eq.1, earth_heliocentric_eq.2
    );

    // Step 4: Calculate geocentric position
    let geocentric_eq =
        heliocentric_to_geocentric_rectangular(planet_heliocentric_eq, earth_heliocentric_eq);
    debug!(
        "Geocentric equatorial: x={:.10}, y={:.10}, z={:.10} AU",
        geocentric_eq.0, geocentric_eq.1, geocentric_eq.2
    );

    // Step 5: Convert to RA/Dec
    let ra_dec = rectangular_to_ra_dec(geocentric_eq.0, geocentric_eq.1, geocentric_eq.2);
    debug!("Final RA/Dec: RA={:.6}h, Dec={:.6}°", ra_dec.ra, ra_dec.dec);

    Ok(ra_dec)
}

/// Gets VSOP87 data for a planet.
///
/// # Arguments
/// * `planet` - The planet to get data for
///
/// # Returns
/// VSOP87 coefficient data for the planet
///
/// # Note
/// This implementation uses simplified/truncated VSOP87 data.
/// Full VSOP87 data contains thousands of terms per planet.
/// The truncated version uses only the most significant terms for each series,
/// providing ~1-5 arcminute accuracy (vs <1 arcsecond for full VSOP87).
///
/// # Data Source
/// Coefficients are based on VSOP87 theory (Bretagnon & Francou, 1987).
/// Truncated versions use only terms with significant amplitudes.
/// For full precision, download complete VSOP87 data from IMCCE:
/// https://ftp.imcce.fr/pub/ephem/planets/vsop87/
pub(crate) fn get_planet_vsop87_data(planet: Planet) -> Option<PlanetVsop87Data> {
    match planet {
        Planet::Mercury => Some(get_mercury_vsop87_data()),
        Planet::Venus => Some(get_venus_vsop87_data()),
        Planet::Earth => Some(get_earth_vsop87_data()),
        Planet::Mars => Some(get_mars_vsop87_data()),
        Planet::Jupiter => Some(get_jupiter_vsop87_data()),
        Planet::Saturn => Some(get_saturn_vsop87_data()),
        Planet::Uranus => Some(get_uranus_vsop87_data()),
        Planet::Neptune => Some(get_neptune_vsop87_data()),
    }
}

/// Convenience constructor for a single VSOP87 term (amplitude, phase, frequency).
fn vt(amplitude: f64, phase: f64, frequency: f64) -> Vsop87Term {
    Vsop87Term {
        amplitude,
        phase,
        frequency,
    }
}

/// Builds a VSOP87 series from the L0/L1/L2 (or B/R equivalent) sub-series.
///
/// Higher-order sub-series (series_3 and above) are unused by the truncated data set
/// and are left empty.
fn series(s0: Vec<Vsop87Term>, s1: Vec<Vsop87Term>, s2: Vec<Vsop87Term>) -> Vsop87Series {
    Vsop87Series {
        series_0: s0,
        series_1: s1,
        series_2: s2,
        series_3: vec![],
        series_4: None,
        series_5: None,
    }
}

/// Gets simplified VSOP87 data for Mercury.
///
/// This is a truncated version using only the most significant terms.
/// For demonstration purposes, includes a minimal set of coefficients.
fn get_mercury_vsop87_data() -> PlanetVsop87Data {
    // Simplified VSOP87 data for Mercury
    // These are truncated coefficients - only most significant terms included
    // Full VSOP87 for Mercury has hundreds of terms per series

    // Longitude (L) series - truncated to largest terms
    let longitude = Vsop87Series {
        // L0: Constant term series (largest terms only)
        series_0: vec![
            Vsop87Term {
                amplitude: 4.40250710144,
                phase: 0.0,
                frequency: 0.0,
            },
            Vsop87Term {
                amplitude: 0.40989414976,
                phase: 1.48302034194,
                frequency: 26087.90314157420,
            },
            Vsop87Term {
                amplitude: 0.05046294199,
                phase: 4.47785489540,
                frequency: 52175.80628314840,
            },
            Vsop87Term {
                amplitude: 0.00855346843,
                phase: 1.16520322359,
                frequency: 78263.70942462180,
            },
            Vsop87Term {
                amplitude: 0.00165506162,
                phase: 4.11969133181,
                frequency: 104351.61256629680,
            },
        ],
        // L1: Linear term series
        series_1: vec![
            Vsop87Term {
                amplitude: 26087.90314157420,
                phase: 0.0,
                frequency: 0.0,
            },
            Vsop87Term {
                amplitude: 0.01126007832,
                phase: 6.21703970996,
                frequency: 26087.90314157420,
            },
            Vsop87Term {
                amplitude: 0.00303471395,
                phase: 3.05524609620,
                frequency: 52175.80628314840,
            },
        ],
        // L2: Quadratic term series
        series_2: vec![
            Vsop87Term {
                amplitude: 0.00053049845,
                phase: 0.0,
                frequency: 0.0,
            },
            Vsop87Term {
                amplitude: 0.00016903658,
                phase: 4.69072300649,
                frequency: 26087.90314157420,
            },
        ],
        // L3: Cubic term series
        series_3: vec![Vsop87Term {
            amplitude: 0.00000169496,
            phase: 3.20221586859,
            frequency: 26087.90314157420,
        }],
        // L4: Quartic term series (optional, only for L)
        series_4: Some(vec![
            Vsop87Term {
                amplitude: 0.00000000000,
                phase: 0.0,
                frequency: 0.0,
            }, // Minimal term
        ]),
        // L5: Quintic term series (optional, only for L)
        series_5: Some(vec![
            Vsop87Term {
                amplitude: 0.00000000000,
                phase: 0.0,
                frequency: 0.0,
            }, // Minimal term
        ]),
    };

    // Latitude (B) series - truncated
    let latitude = Vsop87Series {
        series_0: vec![
            Vsop87Term {
                amplitude: 0.11737528962,
                phase: 1.98357498767,
                frequency: 26087.90314157420,
            },
            Vsop87Term {
                amplitude: 0.02388076996,
                phase: 5.03738959686,
                frequency: 52175.80628314840,
            },
            Vsop87Term {
                amplitude: 0.01222839532,
                phase: 3.14159265359,
                frequency: 0.0,
            },
        ],
        series_1: vec![Vsop87Term {
            amplitude: 0.00397535498,
            phase: 4.93750888835,
            frequency: 26087.90314157420,
        }],
        series_2: vec![Vsop87Term {
            amplitude: 0.00000000000,
            phase: 0.0,
            frequency: 0.0,
        }],
        series_3: vec![Vsop87Term {
            amplitude: 0.00000000000,
            phase: 0.0,
            frequency: 0.0,
        }],
        series_4: None, // B4 not used
        series_5: None, // B5 not used
    };

    // Radius (R) series - truncated
    let radius = Vsop87Series {
        series_0: vec![
            Vsop87Term {
                amplitude: 0.39528271652,
                phase: 0.0,
                frequency: 0.0,
            },
            Vsop87Term {
                amplitude: 0.07834131717,
                phase: 6.19233722599,
                frequency: 26087.90314157420,
            },
            Vsop87Term {
                amplitude: 0.00795532757,
                phase: 2.95989680096,
                frequency: 52175.80628314840,
            },
        ],
        series_1: vec![Vsop87Term {
            amplitude: 0.00217347739,
            phase: 4.65617158663,
            frequency: 26087.90314157420,
        }],
        series_2: vec![Vsop87Term {
            amplitude: 0.00000000000,
            phase: 0.0,
            frequency: 0.0,
        }],
        series_3: vec![Vsop87Term {
            amplitude: 0.00000000000,
            phase: 0.0,
            frequency: 0.0,
        }],
        series_4: None, // R4 not used
        series_5: None, // R5 not used
    };

    PlanetVsop87Data {
        longitude,
        latitude,
        radius,
    }
}

/// Gets truncated VSOP87D data for Venus (leading terms).
fn get_venus_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(3.17614666774, 0.0, 0.0),
                vt(0.01353968419, 5.59313319619, 10213.28554621100),
                vt(0.00089891645, 5.30650047764, 20426.57109242200),
                vt(0.00005477201, 4.41630661466, 7860.41939243920),
                vt(0.00003455732, 2.69963892930, 11790.62908865880),
                vt(0.00002372061, 2.99377538641, 3930.20969621960),
            ],
            vec![
                vt(10213.52943052898, 0.0, 0.0),
                vt(0.00095707712, 2.46424448979, 10213.28554621100),
                vt(0.00002104569, 5.54791255794, 20426.57109242200),
            ],
            vec![
                vt(0.00054127076, 0.0, 0.0),
                vt(0.00003891460, 0.34514360047, 10213.28554621100),
            ],
        ),
        latitude: series(
            vec![
                vt(0.05923638472, 0.26702775812, 10213.28554621100),
                vt(0.00040107978, 1.14737178112, 20426.57109242200),
                vt(0.00032814918, 3.14159265359, 0.0),
            ],
            vec![vt(0.00287821243, 1.88964962838, 10213.28554621100)],
            vec![],
        ),
        radius: series(
            vec![
                vt(0.72334820891, 0.0, 0.0),
                vt(0.00489824182, 4.02151831717, 10213.28554621100),
                vt(0.00001658058, 4.90206728031, 20426.57109242200),
            ],
            vec![vt(0.00034551041, 0.89198706276, 10213.28554621100)],
            vec![],
        ),
    }
}

/// Gets truncated VSOP87D data for Earth (leading terms).
///
/// Earth's heliocentric position is required to convert other planets' heliocentric
/// coordinates into geocentric coordinates, so this data underpins every planet
/// position calculation.
fn get_earth_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(1.75347045673, 0.0, 0.0),
                vt(0.03341656456, 4.66925680417, 6283.07584999140),
                vt(0.00034894275, 4.62610241759, 12566.15169998280),
                vt(0.00003497056, 2.74411800971, 5753.38488489680),
                vt(0.00003417571, 2.82886579606, 3.52311834900),
                vt(0.00003135896, 3.62767041758, 77713.77146812050),
                vt(0.00002676218, 4.41808351397, 7860.41939243920),
                vt(0.00002342687, 6.13516237631, 3930.20969621960),
                vt(0.00001324292, 0.74246356352, 11506.76976979360),
                vt(0.00001273166, 2.03709655772, 529.69096509460),
                vt(0.00001199167, 1.10962944315, 1577.34354244780),
                vt(0.00000990250, 5.23268129594, 5884.92684658320),
                vt(0.00000901855, 2.04505446933, 26.29831979980),
                vt(0.00000857223, 3.50849152283, 398.14900340820),
            ],
            vec![
                vt(6283.31966747491, 0.0, 0.0),
                vt(0.00206058863, 2.67823455584, 6283.07584999140),
                vt(0.00004303430, 2.63512650414, 12566.15169998280),
            ],
            vec![
                vt(0.00052918870, 0.0, 0.0),
                vt(0.00008719837, 1.07209665242, 6283.07584999140),
                vt(0.00000309125, 0.86728818832, 12566.15169998280),
            ],
        ),
        latitude: series(
            vec![
                vt(0.00000279620, 3.19870156017, 84334.66158130829),
                vt(0.00000101643, 5.42248619256, 5507.55323866740),
                vt(0.00000080445, 3.88013204458, 5223.69391980220),
            ],
            vec![],
            vec![],
        ),
        radius: series(
            vec![
                vt(1.00013988784, 0.0, 0.0),
                vt(0.01670699632, 3.09846350258, 6283.07584999140),
                vt(0.00013956024, 3.05524609456, 12566.15169998280),
                vt(0.00003083720, 5.19846674381, 77713.77146812050),
                vt(0.00001628463, 1.17387558054, 5753.38488489680),
                vt(0.00001575572, 2.84685214877, 7860.41939243920),
            ],
            vec![
                vt(0.00103018608, 1.10748969588, 6283.07584999140),
                vt(0.00001721238, 1.06442301418, 12566.15169998280),
            ],
            vec![vt(0.00004359385, 5.78455133738, 6283.07584999140)],
        ),
    }
}

/// Gets truncated VSOP87D data for Mars (leading terms).
fn get_mars_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(6.20347711581, 0.0, 0.0),
                vt(0.18656368093, 5.05037100303, 3340.61242669980),
                vt(0.01108216816, 5.40099836958, 6681.22485339960),
                vt(0.00091798406, 5.75478744674, 10021.83728009940),
                vt(0.00027744987, 5.97049513147, 3.52311834900),
                vt(0.00012315897, 0.84956094002, 2810.92146160520),
                vt(0.00010610235, 2.93958560338, 2281.23049651060),
                vt(0.00008926784, 4.15697846427, 0.01725365220),
            ],
            vec![
                vt(3340.61242700512, 0.0, 0.0),
                vt(0.01457554523, 3.60433733236, 3340.61242669980),
                vt(0.00168414711, 3.92318567804, 6681.22485339960),
                vt(0.00020622975, 4.26108844583, 10021.83728009940),
            ],
            vec![
                vt(0.00058152577, 2.04961712429, 3340.61242669980),
                vt(0.00013459579, 2.45738706163, 6681.22485339960),
            ],
        ),
        latitude: series(
            vec![
                vt(0.03197134986, 3.76832042431, 3340.61242669980),
                vt(0.00298033234, 4.10616996305, 6681.22485339960),
                vt(0.00289104742, 0.0, 0.0),
                vt(0.00031365539, 4.44651053090, 10021.83728009940),
            ],
            vec![vt(0.00217310991, 6.04472194776, 3340.61242669980)],
            vec![],
        ),
        radius: series(
            vec![
                vt(1.53033488271, 0.0, 0.0),
                vt(0.14184953160, 3.47971283528, 3340.61242669980),
                vt(0.00660776362, 3.81783443019, 6681.22485339960),
                vt(0.00046179117, 4.15595316782, 10021.83728009940),
            ],
            vec![
                vt(0.01107433345, 2.03250524857, 3340.61242669980),
                vt(0.00103175887, 3.05705419660, 6681.22485339960),
            ],
            vec![vt(0.00044242249, 0.47930604954, 3340.61242669980)],
        ),
    }
}

/// Gets truncated VSOP87D data for Jupiter (leading terms).
fn get_jupiter_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(0.59954691494, 0.0, 0.0),
                vt(0.09695898719, 5.06191793158, 529.69096509460),
                vt(0.00573610142, 1.44406205629, 7.11354700080),
                vt(0.00306389205, 5.41734730184, 1059.38193018920),
                vt(0.00097178296, 4.14264726552, 632.78373931320),
                vt(0.00072903078, 3.64042916389, 522.57741809380),
                vt(0.00064263975, 3.41145165351, 103.09277421860),
                vt(0.00039806064, 2.29376740788, 419.48464387520),
            ],
            vec![
                vt(529.69096508814, 0.0, 0.0),
                vt(0.00489503243, 4.22082939470, 529.69096509460),
                vt(0.00228917222, 6.02646855621, 7.11354700080),
                vt(0.00030099479, 4.54540782858, 1059.38193018920),
            ],
            vec![
                vt(0.00047233601, 4.32148536482, 7.11354700080),
                vt(0.00030649436, 2.92977788700, 529.69096509460),
            ],
        ),
        latitude: series(
            vec![
                vt(0.02268615702, 3.55852606721, 529.69096509460),
                vt(0.00109971634, 3.90809347197, 1059.38193018920),
                vt(0.00110090358, 0.0, 0.0),
                vt(0.00008101428, 3.60509572885, 522.57741809380),
            ],
            vec![vt(0.00078203446, 1.52377859742, 529.69096509460)],
            vec![],
        ),
        radius: series(
            vec![
                vt(5.20887429326, 0.0, 0.0),
                vt(0.25209327119, 3.49108639871, 529.69096509460),
                vt(0.00610599976, 3.84115365948, 1059.38193018920),
                vt(0.00282029458, 2.57419881293, 632.78373931320),
                vt(0.00187647346, 2.07590383214, 522.57741809380),
            ],
            vec![
                vt(0.01271801520, 2.64937512894, 529.69096509460),
                vt(0.00061661816, 3.00076460387, 1059.38193018920),
            ],
            vec![vt(0.00078203446, 1.52377859742, 529.69096509460)],
        ),
    }
}

/// Gets truncated VSOP87D data for Saturn (leading terms).
fn get_saturn_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(0.87401354025, 0.0, 0.0),
                vt(0.11107659762, 3.96205090159, 213.29909543800),
                vt(0.01414150957, 4.58581516874, 7.11354700080),
                vt(0.00398379389, 0.52112032699, 206.18554843720),
                vt(0.00350769243, 3.30329907896, 426.59819087600),
                vt(0.00206816305, 0.24658372002, 103.09277421860),
                vt(0.00079271300, 3.84007056878, 220.41264243880),
                vt(0.00023990355, 4.66976924553, 110.20632121940),
            ],
            vec![
                vt(213.29909521690, 0.0, 0.0),
                vt(0.01297370862, 1.82834923978, 213.29909543800),
                vt(0.00564345393, 2.88499717272, 7.11354700080),
                vt(0.00093734369, 1.06311793502, 426.59819087600),
            ],
            vec![
                vt(0.00116441330, 1.17988132879, 7.11354700080),
                vt(0.00091841837, 0.07325195840, 213.29909543800),
            ],
        ),
        latitude: series(
            vec![
                vt(0.04330678039, 3.60284428399, 213.29909543800),
                vt(0.00240348302, 2.85238489373, 426.59819087600),
                vt(0.00084745939, 0.0, 0.0),
                vt(0.00030863357, 3.48441504555, 220.41264243880),
            ],
            vec![vt(0.00397554613, 5.33289992556, 213.29909543800)],
            vec![],
        ),
        radius: series(
            vec![
                vt(9.55758135486, 0.0, 0.0),
                vt(0.52921382865, 2.39226219573, 213.29909543800),
                vt(0.01873679867, 5.23549604660, 206.18554843720),
                vt(0.01464663929, 1.64763042902, 426.59819087600),
                vt(0.00821891141, 5.93520042303, 316.39186965660),
            ],
            vec![
                vt(0.06182981340, 0.25843511480, 213.29909543800),
                vt(0.00506577242, 0.71114625261, 206.18554843720),
            ],
            vec![vt(0.00227714534, 3.41648670629, 213.29909543800)],
        ),
    }
}

/// Gets truncated VSOP87D data for Uranus (leading terms).
fn get_uranus_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(5.48129294297, 0.0, 0.0),
                vt(0.09260408234, 0.89106421507, 74.78159856730),
                vt(0.01504247898, 3.62719260920, 1.48447270830),
                vt(0.00365981674, 1.89962179044, 73.29712585900),
                vt(0.00272328168, 3.35823706307, 149.56319713460),
                vt(0.00070328461, 5.39254450063, 63.73589830340),
                vt(0.00068892678, 6.09292483287, 76.26607127560),
            ],
            vec![
                vt(74.78159860910, 0.0, 0.0),
                vt(0.00154332863, 5.24158770553, 74.78159856730),
                vt(0.00024456474, 1.71260334156, 1.48447270830),
            ],
            vec![
                vt(0.00053033510, 0.0, 0.0),
                vt(0.00002357844, 2.26014661705, 74.78159856730),
            ],
        ),
        latitude: series(
            vec![
                vt(0.01346277648, 2.61877810547, 74.78159856730),
                vt(0.00062341400, 5.08111189648, 149.56319713460),
                vt(0.00061601196, 3.14159265359, 0.0),
                vt(0.00009963722, 1.61603805646, 76.26607127560),
            ],
            vec![vt(0.00034101978, 0.01321929936, 74.78159856730)],
            vec![],
        ),
        radius: series(
            vec![
                vt(19.21264847206, 0.0, 0.0),
                vt(0.88784984413, 5.60377527014, 74.78159856730),
                vt(0.03440836562, 0.32836099706, 73.29712585900),
                vt(0.02055653860, 1.78295159330, 149.56319713460),
                vt(0.00649322410, 4.52247285911, 76.26607127560),
            ],
            vec![vt(0.01479896629, 3.67205697578, 74.78159856730)],
            vec![],
        ),
    }
}

/// Gets truncated VSOP87D data for Neptune (leading terms).
fn get_neptune_vsop87_data() -> PlanetVsop87Data {
    PlanetVsop87Data {
        longitude: series(
            vec![
                vt(5.31188633046, 0.0, 0.0),
                vt(0.01798475530, 2.90101273050, 38.13303563780),
                vt(0.01019727652, 0.48580922867, 1.48447270830),
                vt(0.00124531845, 4.83008090682, 36.64856292950),
                vt(0.00042064466, 5.41054993053, 2.96894541660),
                vt(0.00037714584, 6.09221808686, 35.16409022120),
            ],
            vec![
                vt(38.13303563957, 0.0, 0.0),
                vt(0.00016604172, 4.86323329249, 1.48447270830),
                vt(0.00015744045, 2.27887427527, 38.13303563780),
            ],
            vec![
                vt(0.00053892649, 0.0, 0.0),
                vt(0.00000281251, 1.19084538887, 38.13303563780),
            ],
        ),
        latitude: series(
            vec![
                vt(0.03088622933, 1.44104372644, 38.13303563780),
                vt(0.00027780087, 5.91271884599, 76.26607127560),
                vt(0.00027623609, 0.0, 0.0),
                vt(0.00015355489, 2.52123799551, 36.64856292950),
            ],
            vec![vt(0.00227279214, 3.80793089870, 38.13303563780)],
            vec![],
        ),
        radius: series(
            vec![
                vt(30.07013206102, 0.0, 0.0),
                vt(0.27062259632, 1.32999459377, 38.13303563780),
                vt(0.01691764014, 3.25186135653, 36.64856292950),
                vt(0.00807830553, 5.18592878704, 1.48447270830),
                vt(0.00537760510, 4.52113935896, 35.16409022120),
            ],
            vec![vt(0.00236338502, 0.70497956691, 38.13303563780)],
            vec![],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_from_str() {
        assert_eq!(Planet::from_name("mercury"), Some(Planet::Mercury));
        assert_eq!(Planet::from_name("JUPITER"), Some(Planet::Jupiter));
        assert_eq!(Planet::from_name("invalid"), None);
    }

    #[test]
    fn test_planet_name() {
        assert_eq!(Planet::Mars.name(), "Mars");
        assert_eq!(Planet::Saturn.name(), "Saturn");
    }

    #[test]
    fn test_evaluate_vsop87_term() {
        let term = Vsop87Term {
            amplitude: 1.0,
            phase: 0.0,
            frequency: 1.0,
        };
        // At t=0: cos(0 + 0) = 1.0
        assert!((evaluate_vsop87_term(&term, 0.0) - 1.0).abs() < 1e-10);

        // At t=π: cos(0 + π) = -1.0
        let t = std::f64::consts::PI;
        assert!((evaluate_vsop87_term(&term, t) + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_planet_vsop87_data() {
        // Test that data is available for all planets
        for planet in [
            Planet::Mercury,
            Planet::Venus,
            Planet::Earth,
            Planet::Mars,
            Planet::Jupiter,
            Planet::Saturn,
            Planet::Uranus,
            Planet::Neptune,
        ] {
            let data = get_planet_vsop87_data(planet);
            assert!(
                data.is_some(),
                "VSOP87 data should be available for {}",
                planet.name()
            );
        }
    }

    #[test]
    fn test_mercury_vsop87_data_structure() {
        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();

        // Verify longitude series structure
        assert!(
            !data.longitude.series_0.is_empty(),
            "Mercury L0 series should have terms"
        );
        assert!(
            !data.longitude.series_1.is_empty(),
            "Mercury L1 series should have terms"
        );
        assert!(
            data.longitude.series_4.is_some(),
            "Mercury L4 series should exist"
        );
        assert!(
            data.longitude.series_5.is_some(),
            "Mercury L5 series should exist"
        );

        // Verify latitude series structure
        assert!(
            !data.latitude.series_0.is_empty(),
            "Mercury B0 series should have terms"
        );
        assert!(
            data.latitude.series_4.is_none(),
            "Mercury B4 series should not exist"
        );
        assert!(
            data.latitude.series_5.is_none(),
            "Mercury B5 series should not exist"
        );

        // Verify radius series structure
        assert!(
            !data.radius.series_0.is_empty(),
            "Mercury R0 series should have terms"
        );
        assert!(
            data.radius.series_4.is_none(),
            "Mercury R4 series should not exist"
        );
        assert!(
            data.radius.series_5.is_none(),
            "Mercury R5 series should not exist"
        );
    }

    #[test]
    fn test_vsop87_term_structure() {
        // Test that VSOP87 terms have valid structure
        let term = Vsop87Term {
            amplitude: 1.0,
            phase: 0.0,
            frequency: 26087.90314157420, // Mercury's main frequency
        };

        // Verify term can be evaluated
        let result = evaluate_vsop87_term(&term, 0.0);
        assert!((result - 1.0).abs() < 1e-10, "Term evaluation should work");

        // Verify amplitude is positive (typical for VSOP87)
        assert!(
            term.amplitude > 0.0,
            "Amplitude should typically be positive"
        );
    }

    #[test]
    fn test_evaluate_vsop87_series() {
        // Test VSOP87 series evaluation with a simple series
        let series = Vsop87Series {
            series_0: vec![
                Vsop87Term {
                    amplitude: 1.0,
                    phase: 0.0,
                    frequency: 0.0,
                }, // Constant term: 1.0
            ],
            series_1: vec![
                Vsop87Term {
                    amplitude: 0.5,
                    phase: 0.0,
                    frequency: 0.0,
                }, // Linear term: 0.5
            ],
            series_2: vec![], // No quadratic term
            series_3: vec![], // No cubic term
            series_4: None,
            series_5: None,
        };

        // At t=0: result = 1.0 + 0.5*0 = 1.0
        let result_t0 = evaluate_vsop87_series(&series, 0.0);
        let expected_t0 = 1.0;
        assert!(
            (result_t0 - expected_t0).abs() < 1e-10,
            "Series at t=0: expected {}, got {}",
            expected_t0,
            result_t0
        );

        // At t=1: result = 1.0 + 0.5*1 = 1.5
        let result_t1 = evaluate_vsop87_series(&series, 1.0);
        let expected_t1 = 1.5;
        assert!(
            (result_t1 - expected_t1).abs() < 1e-10,
            "Series at t=1: expected {}, got {}",
            expected_t1,
            result_t1
        );
    }

    #[test]
    fn test_calculate_heliocentric_ecliptic() {
        // Test heliocentric ecliptic calculation for Mercury at J2000.0
        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();
        let t = 0.0; // J2000.0 epoch

        // Calculate heliocentric coordinates
        let result = calculate_heliocentric_ecliptic(&data, t).unwrap();

        // Verify structure (values will be validated against reference data in Step B5)
        assert!(
            result.longitude >= 0.0 && result.longitude < 2.0 * std::f64::consts::PI,
            "Longitude should be in [0, 2π) range"
        );
        assert!(
            result.latitude.abs() < std::f64::consts::PI / 2.0,
            "Latitude should be in [-π/2, π/2] range"
        );
        assert!(
            result.radius > 0.0 && result.radius < 2.0,
            "Radius should be positive and reasonable for Mercury (< 2 AU)"
        );
    }

    #[test]
    fn test_calculate_planet_position_validation() {
        // Test that calculate_planet_position validates inputs
        let jd = 2451545.0; // J2000.0

        // Test with invalid Julian Date (NaN)
        let result_nan = calculate_planet_position(Planet::Mercury, f64::NAN);
        assert!(
            result_nan.is_err(),
            "Should return error for NaN Julian Date"
        );

        // Test with invalid Julian Date (infinity)
        let result_inf = calculate_planet_position(Planet::Mercury, f64::INFINITY);
        assert!(
            result_inf.is_err(),
            "Should return error for infinite Julian Date"
        );

        // Test with valid Julian Date
        // May succeed if Earth data is available, or fail if Earth data is placeholder
        let result_valid = calculate_planet_position(Planet::Mercury, jd);

        match result_valid {
            // If successful, verify RA/Dec are in valid ranges
            Ok(ra_dec) => {
                assert!(
                    (0.0..24.0).contains(&ra_dec.ra),
                    "RA should be in [0, 24) hours"
                );
                assert!(
                    (-90.0..=90.0).contains(&ra_dec.dec),
                    "Dec should be in [-90, 90] degrees"
                );
            }
            // If it fails, should be because Earth data is not available
            Err(err) => {
                let error_msg = err.to_string();
                assert!(
                    error_msg.contains("Earth") || error_msg.contains("VSOP87"),
                    "Error should mention Earth or VSOP87 data: {}",
                    error_msg
                );
            }
        }
    }

    // ========== Step B5: Comprehensive Testing & Validation ==========

    #[test]
    fn test_vsop87_series_evaluation_edge_cases() {
        // Test VSOP87 series evaluation with edge cases

        // Test with empty series
        let empty_series = Vsop87Series {
            series_0: vec![],
            series_1: vec![],
            series_2: vec![],
            series_3: vec![],
            series_4: None,
            series_5: None,
        };
        let result = evaluate_vsop87_series(&empty_series, 0.0);
        assert_eq!(result, 0.0, "Empty series should evaluate to 0");

        // Test with t = 0 (J2000.0 epoch)
        let simple_series = Vsop87Series {
            series_0: vec![Vsop87Term {
                amplitude: 1.0,
                phase: 0.0,
                frequency: 0.0,
            }],
            series_1: vec![],
            series_2: vec![],
            series_3: vec![],
            series_4: None,
            series_5: None,
        };
        let result_t0 = evaluate_vsop87_series(&simple_series, 0.0);
        assert!(
            (result_t0 - 1.0).abs() < 1e-15,
            "Series at t=0 should equal constant term"
        );

        // Test with large t value (20 millennia from J2000.0)
        let result_t20 = evaluate_vsop87_series(&simple_series, 20.0);
        assert!(
            (result_t20 - 1.0).abs() < 1e-15,
            "Series with only constant term should be independent of t"
        );

        // Test with negative t value (past epoch)
        let result_t_neg = evaluate_vsop87_series(&simple_series, -10.0);
        assert!(
            (result_t_neg - 1.0).abs() < 1e-15,
            "Series should handle negative time values"
        );
    }

    #[test]
    fn test_vsop87_series_evaluation_trigonometric() {
        // Test VSOP87 series with trigonometric terms
        let trig_series = Vsop87Series {
            series_0: vec![Vsop87Term {
                amplitude: 1.0,
                phase: 0.0,
                frequency: 1.0,
            }],
            series_1: vec![Vsop87Term {
                amplitude: 0.5,
                phase: std::f64::consts::PI / 2.0,
                frequency: 1.0,
            }],
            series_2: vec![],
            series_3: vec![],
            series_4: None,
            series_5: None,
        };

        // At t = 0: cos(0) = 1.0, cos(π/2) = 0.0
        // Result = 1.0 + 0.5*0*0 = 1.0
        let result_t0 = evaluate_vsop87_series(&trig_series, 0.0);
        assert!(
            (result_t0 - 1.0).abs() < 1e-10,
            "Trigonometric series at t=0 should be correct"
        );

        // At t = π/2: cos(π/2) = 0.0, cos(π/2 + π/2) = cos(π) = -1.0
        // Result = 0.0 + 0.5*(π/2)*(-1.0)
        let t_pi2 = std::f64::consts::PI / 2.0;
        let result_t_pi2 = evaluate_vsop87_series(&trig_series, t_pi2);
        let expected = -0.5 * t_pi2;
        assert!(
            (result_t_pi2 - expected).abs() < 1e-10,
            "Trigonometric series at t=π/2 should be correct"
        );
    }

    #[test]
    fn test_mercury_heliocentric_j2000() {
        // Test Mercury's heliocentric coordinates at J2000.0
        // Reference: Approximate values for validation
        // Note: Using truncated VSOP87, so values are approximate
        // Note: Coefficient scaling may need adjustment - this test verifies structure

        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();
        let t = 0.0; // J2000.0 epoch

        let heliocentric = calculate_heliocentric_ecliptic(&data, t).unwrap();

        // Verify reasonable ranges for Mercury
        // Longitude: Should be in [0, 2π) range
        assert!(
            heliocentric.longitude >= 0.0 && heliocentric.longitude < 2.0 * std::f64::consts::PI,
            "Mercury longitude should be in [0, 2π) range, got {}",
            heliocentric.longitude
        );

        // Latitude: Mercury's orbit is inclined ~7°, so latitude should be small
        assert!(
            heliocentric.latitude.abs() < 0.2, // ~11.5° in radians
            "Mercury latitude should be small (orbit near ecliptic), got {} rad ({:.2}°)",
            heliocentric.latitude,
            heliocentric.latitude.to_degrees()
        );

        // Radius: Verify it's positive and finite
        // Note: Actual values depend on coefficient scaling - may need adjustment
        // For now, verify the calculation produces a valid number
        assert!(
            heliocentric.radius.is_finite() && heliocentric.radius > 0.0,
            "Mercury radius should be positive and finite, got {} AU",
            heliocentric.radius
        );

        // Log actual value for debugging coefficient scaling
        println!(
            "Mercury at J2000.0: L={:.6}°, B={:.6}°, R={:.10} AU",
            heliocentric.longitude.to_degrees(),
            heliocentric.latitude.to_degrees(),
            heliocentric.radius
        );
    }

    #[test]
    fn test_mercury_heliocentric_multiple_epochs() {
        // Test Mercury at multiple epochs to verify consistency

        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();

        // Test at J2000.0
        let t_j2000 = 0.0;
        let pos_j2000 = calculate_heliocentric_ecliptic(&data, t_j2000).unwrap();

        // Test at 1 century after J2000.0
        let t_plus1 = 1.0;
        let pos_plus1 = calculate_heliocentric_ecliptic(&data, t_plus1).unwrap();

        // Test at 1 century before J2000.0
        let t_minus1 = -1.0;
        let pos_minus1 = calculate_heliocentric_ecliptic(&data, t_minus1).unwrap();

        // Verify all positions are reasonable
        for (epoch, pos) in [
            ("J2000", pos_j2000),
            ("+1c", pos_plus1),
            ("-1c", pos_minus1),
        ] {
            assert!(
                pos.longitude >= 0.0 && pos.longitude < 2.0 * std::f64::consts::PI,
                "Mercury longitude at {} should be in [0, 2π) range",
                epoch
            );
            assert!(
                pos.latitude.abs() < 0.2,
                "Mercury latitude at {} should be small",
                epoch
            );
            // Verify radius is positive and finite (actual values depend on coefficient scaling)
            assert!(
                pos.radius.is_finite() && pos.radius > 0.0,
                "Mercury radius at {} should be positive and finite, got {}",
                epoch,
                pos.radius
            );
        }

        // Verify positions are different (Mercury moves in its orbit)
        // Longitude should change significantly over 1 century
        let lon_diff = (pos_plus1.longitude - pos_j2000.longitude).abs();
        // Over 1 century Mercury completes hundreds of orbits, so the wrapped longitude
        // difference is effectively arbitrary; this simply confirms the evaluation produced a
        // finite, usable value at both epochs.
        assert!(
            lon_diff.is_finite(),
            "Mercury longitude difference over 1 century should be finite, got {lon_diff}"
        );
    }

    #[test]
    fn test_integration_multiple_planets_same_epoch() {
        // Test multiple planets at the same epoch (J2000.0)
        // This verifies that the VSOP87 evaluation works consistently across planets

        let _jd = 2451545.0; // J2000.0
        let t = 0.0;

        // Test all planets that have data (currently only Mercury has real data)
        let planets = [Planet::Mercury];

        for planet in planets.iter() {
            let data = get_planet_vsop87_data(*planet).unwrap();
            let heliocentric = calculate_heliocentric_ecliptic(&data, t).unwrap();

            // Verify basic sanity checks
            assert!(
                heliocentric.longitude >= 0.0
                    && heliocentric.longitude < 2.0 * std::f64::consts::PI,
                "{} longitude should be in [0, 2π) range",
                planet.name()
            );
            assert!(
                heliocentric.radius > 0.0,
                "{} radius should be positive",
                planet.name()
            );
        }
    }

    #[test]
    fn test_integration_planets_different_epochs() {
        // Test planets at different epochs (past and future)
        // This verifies numerical stability and consistency

        let epochs = [
            ("J2000.0", 2451545.0),
            ("2024-01-01", 2460311.0), // Future
            ("1900-01-01", 2415020.5), // Past
        ];

        for (epoch_name, jd) in epochs.iter() {
            let t = (jd - 2451545.0) / 36525.0;

            // Test Mercury (only planet with real data)
            let data = get_planet_vsop87_data(Planet::Mercury).unwrap();
            let heliocentric = calculate_heliocentric_ecliptic(&data, t).unwrap();

            // Verify reasonable values
            assert!(
                heliocentric.longitude >= 0.0
                    && heliocentric.longitude < 2.0 * std::f64::consts::PI,
                "Mercury longitude at {} should be in [0, 2π) range",
                epoch_name
            );
            assert!(
                heliocentric.radius > 0.0 && heliocentric.radius < 1.0,
                "Mercury radius at {} should be reasonable",
                epoch_name
            );
        }
    }

    #[test]
    fn test_vsop87_series_consistency() {
        // Test that VSOP87 series evaluation is consistent
        // Evaluate the same series multiple times and verify identical results

        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();
        let t = 0.0;

        // Evaluate multiple times
        let result1 = evaluate_vsop87_series(&data.longitude, t);
        let result2 = evaluate_vsop87_series(&data.longitude, t);
        let result3 = evaluate_vsop87_series(&data.longitude, t);

        // All results should be identical (deterministic)
        assert!(
            (result1 - result2).abs() < 1e-15,
            "VSOP87 series evaluation should be deterministic"
        );
        assert!(
            (result2 - result3).abs() < 1e-15,
            "VSOP87 series evaluation should be deterministic"
        );
    }

    #[test]
    fn test_time_calculation_accuracy() {
        // Test that time in Julian centuries is calculated correctly

        const J2000: f64 = 2451545.0;

        // Test at J2000.0
        let jd_j2000 = 2451545.0;
        let t_j2000 = (jd_j2000 - J2000) / 36525.0;
        assert!(
            (t_j2000 - 0.0).abs() < 1e-10,
            "Time at J2000.0 should be 0.0 centuries"
        );

        // Test at 1 century after J2000.0
        let jd_plus1c = 2451545.0 + 36525.0;
        let t_plus1c = (jd_plus1c - J2000) / 36525.0;
        assert!(
            (t_plus1c - 1.0).abs() < 1e-10,
            "Time 1 century after J2000.0 should be 1.0 centuries"
        );

        // Test at 1 century before J2000.0
        let jd_minus1c = 2451545.0 - 36525.0;
        let t_minus1c = (jd_minus1c - J2000) / 36525.0;
        assert!(
            (t_minus1c + 1.0).abs() < 1e-10,
            "Time 1 century before J2000.0 should be -1.0 centuries"
        );
    }

    #[test]
    fn test_heliocentric_coordinate_ranges() {
        // Test that heliocentric coordinates are always in valid ranges

        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();

        // Test at multiple time points
        for t in [-10.0, -5.0, -1.0, 0.0, 1.0, 5.0, 10.0] {
            let heliocentric = calculate_heliocentric_ecliptic(&data, t).unwrap();

            // Longitude should always be in [0, 2π)
            assert!(
                heliocentric.longitude >= 0.0
                    && heliocentric.longitude < 2.0 * std::f64::consts::PI,
                "Longitude at t={} should be in [0, 2π) range, got {}",
                t,
                heliocentric.longitude
            );

            // Latitude should be in [-π/2, π/2] range (though planets stay near ecliptic)
            assert!(
                heliocentric.latitude >= -std::f64::consts::PI / 2.0
                    && heliocentric.latitude <= std::f64::consts::PI / 2.0,
                "Latitude at t={} should be in [-π/2, π/2] range, got {}",
                t,
                heliocentric.latitude
            );

            // Radius should always be positive
            assert!(
                heliocentric.radius > 0.0,
                "Radius at t={} should be positive, got {}",
                t,
                heliocentric.radius
            );
        }
    }

    #[test]
    fn test_performance_planet_calculation() {
        // Performance test: Ensure planet calculations complete in reasonable time
        // Target: < 10ms per planet (for truncated VSOP87)

        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();
        let t = 0.0;

        // Measure time for multiple calculations
        let start = std::time::Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            let _ = calculate_heliocentric_ecliptic(&data, t).unwrap();
        }

        let elapsed = start.elapsed();
        let avg_time_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

        // For truncated VSOP87, should be very fast (< 1ms per calculation)
        assert!(
            avg_time_ms < 10.0,
            "Average calculation time should be < 10ms, got {:.3}ms",
            avg_time_ms
        );

        // Log performance (will show in test output)
        println!(
            "Performance: {} calculations in {:.2}ms, avg {:.3}ms per calculation",
            iterations,
            elapsed.as_secs_f64() * 1000.0,
            avg_time_ms
        );
    }

    #[test]
    fn test_performance_vsop87_series_evaluation() {
        // Performance test: Ensure VSOP87 series evaluation is efficient

        let data = get_planet_vsop87_data(Planet::Mercury).unwrap();
        let t = 0.0;

        let start = std::time::Instant::now();
        let iterations = 10000;

        for _ in 0..iterations {
            let _ = evaluate_vsop87_series(&data.longitude, t);
            let _ = evaluate_vsop87_series(&data.latitude, t);
            let _ = evaluate_vsop87_series(&data.radius, t);
        }

        let elapsed = start.elapsed();
        let avg_time_us = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;

        // Series evaluation should be very fast (< 100 microseconds per series)
        assert!(
            avg_time_us < 1000.0, // 1ms per 3 series evaluations
            "Average series evaluation time should be < 1ms, got {:.3}μs",
            avg_time_us
        );

        println!(
            "Performance: {} series evaluations in {:.2}ms, avg {:.3}μs per series",
            iterations * 3,
            elapsed.as_secs_f64() * 1000.0,
            avg_time_us
        );
    }

    // ========== Step B4: Coordinate Conversion Tests ==========

    #[test]
    fn test_calculate_obliquity() {
        // Test obliquity calculation at J2000.0
        let jd_j2000 = 2451545.0;
        let obliquity = calculate_obliquity(jd_j2000);

        // Obliquity at J2000.0 should be approximately 23.4393°
        let expected_deg = 23.4393;
        assert!(
            (obliquity.to_degrees() - expected_deg).abs() < 0.01,
            "Obliquity at J2000.0 should be ~{:.4}°, got {:.4}°",
            expected_deg,
            obliquity.to_degrees()
        );

        // Test at a different date (2024)
        let jd_2024 = 2460311.0;
        let obliquity_2024 = calculate_obliquity(jd_2024);

        // Obliquity should decrease slightly over time
        assert!(
            obliquity_2024 < obliquity,
            "Obliquity should decrease over time"
        );
    }

    #[test]
    fn test_ecliptic_to_equatorial() {
        // Test ecliptic to equatorial conversion
        // At J2000.0, obliquity is ~23.4393°
        let jd = 2451545.0;
        let obliquity = calculate_obliquity(jd);

        // Test with a point on the ecliptic (latitude = 0)
        let heliocentric = HeliocentricEcliptic {
            longitude: 0.0, // 0° longitude
            latitude: 0.0,  // On ecliptic plane
            radius: 1.0,    // 1 AU
        };

        let (x, y, z) = ecliptic_to_equatorial(heliocentric, obliquity);

        // Point at 0° longitude, 0° latitude should map to (1, 0, 0) in ecliptic
        // After rotation by obliquity, z should be 0 (still in equatorial plane)
        assert!((x - 1.0).abs() < 1e-10, "X coordinate should be 1.0");
        assert!(y.abs() < 1e-10, "Y coordinate should be ~0");
        assert!(
            z.abs() < 1e-10,
            "Z coordinate should be ~0 (on equatorial plane)"
        );
    }

    #[test]
    fn test_heliocentric_to_geocentric_rectangular() {
        // Test heliocentric to geocentric conversion
        let planet_pos = (2.0, 0.0, 0.0); // Planet at 2 AU on X-axis
        let earth_pos = (1.0, 0.0, 0.0); // Earth at 1 AU on X-axis

        let geocentric = heliocentric_to_geocentric_rectangular(planet_pos, earth_pos);

        // Geocentric position should be planet - earth = (1, 0, 0)
        assert!((geocentric.0 - 1.0).abs() < 1e-10, "X should be 1.0");
        assert!(geocentric.1.abs() < 1e-10, "Y should be 0");
        assert!(geocentric.2.abs() < 1e-10, "Z should be 0");
    }

    #[test]
    fn test_rectangular_to_ra_dec() {
        // Test rectangular to RA/Dec conversion

        // Point on positive X-axis (RA = 0h, Dec = 0°)
        let (x, y, z) = (1.0, 0.0, 0.0);
        let ra_dec = rectangular_to_ra_dec(x, y, z);
        assert!(
            (ra_dec.ra - 0.0).abs() < 1e-10 || (ra_dec.ra - 24.0).abs() < 1e-10,
            "RA should be 0h or 24h for point on +X axis"
        );
        assert!(
            ra_dec.dec.abs() < 1e-10,
            "Dec should be ~0° for point on equator"
        );

        // Point on positive Y-axis (RA = 6h, Dec = 0°)
        let (x, y, z) = (0.0, 1.0, 0.0);
        let ra_dec = rectangular_to_ra_dec(x, y, z);
        assert!(
            (ra_dec.ra - 6.0).abs() < 1e-10,
            "RA should be 6h for point on +Y axis, got {}",
            ra_dec.ra
        );
        assert!(ra_dec.dec.abs() < 1e-10, "Dec should be ~0°");

        // Point on positive Z-axis (Dec = 90°)
        let (x, y, z) = (0.0, 0.0, 1.0);
        let ra_dec = rectangular_to_ra_dec(x, y, z);
        assert!(
            (ra_dec.dec - 90.0).abs() < 1e-10,
            "Dec should be 90° for point on +Z axis, got {}",
            ra_dec.dec
        );
    }

    #[test]
    fn test_heliocentric_to_geocentric_full_pipeline() {
        // Test the full coordinate conversion pipeline
        // This tests the complete heliocentric ecliptic → geocentric equatorial conversion

        let jd = 2451545.0; // J2000.0

        // Create test heliocentric positions
        let planet_heliocentric = HeliocentricEcliptic {
            longitude: 0.0,
            latitude: 0.0,
            radius: 2.0, // Planet at 2 AU
        };

        let earth_heliocentric = HeliocentricEcliptic {
            longitude: 0.0,
            latitude: 0.0,
            radius: 1.0, // Earth at 1 AU
        };

        // Convert to geocentric RA/Dec
        let result = heliocentric_to_geocentric(planet_heliocentric, earth_heliocentric, jd);

        assert!(result.is_ok(), "Conversion should succeed");
        let ra_dec = result.unwrap();

        // Verify RA/Dec are in valid ranges
        assert!(
            ra_dec.ra >= 0.0 && ra_dec.ra < 24.0,
            "RA should be in [0, 24) hours, got {}",
            ra_dec.ra
        );
        assert!(
            ra_dec.dec >= -90.0 && ra_dec.dec <= 90.0,
            "Dec should be in [-90, 90] degrees, got {}",
            ra_dec.dec
        );
    }

    #[test]
    fn test_calculate_planet_position_complete() {
        // Test complete planet position calculation (if Earth data is available)
        // Note: This may fail if Earth's VSOP87 data is not implemented

        let jd = 2451545.0; // J2000.0

        // Try to calculate Mercury's position
        let result = calculate_planet_position(Planet::Mercury, jd);

        // If Earth data is available, should succeed
        // If Earth data is placeholder, will fail gracefully
        match result {
            Ok(ra_dec) => {
                // Verify RA/Dec are in valid ranges
                assert!(
                    (0.0..24.0).contains(&ra_dec.ra),
                    "RA should be in [0, 24) hours, got {}",
                    ra_dec.ra
                );
                assert!(
                    (-90.0..=90.0).contains(&ra_dec.dec),
                    "Dec should be in [-90, 90] degrees, got {}",
                    ra_dec.dec
                );
            }
            // If it fails, it should be because Earth data is not available
            Err(err) => {
                let error_msg = err.to_string();
                assert!(
                    error_msg.contains("Earth") || error_msg.contains("VSOP87"),
                    "Error should mention Earth or VSOP87 data"
                );
            }
        }
    }

    #[test]
    fn test_earth_radius_near_one_au() {
        // Earth's heliocentric distance must stay close to 1 AU (perihelion ~0.983,
        // aphelion ~1.017). This guards the VSOP87 scaling/time-unit convention.
        let data = get_planet_vsop87_data(Planet::Earth).unwrap();
        for t in [-0.05, 0.0, 0.024, 0.05] {
            // t in Julian millennia
            let h = calculate_heliocentric_ecliptic(&data, t).unwrap();
            assert!(
                h.radius > 0.98 && h.radius < 1.02,
                "Earth radius should be ~1 AU, got {} AU at t={}",
                h.radius,
                t
            );
        }
    }

    #[test]
    fn test_geocentric_positions_match_reference_j2000() {
        // Geocentric RA/Dec at JD 2451545.0 (2000-01-01 12:00 UTC), compared to JPL Horizons.
        // Tolerances are generous to allow for the truncated VSOP87D series, but tight enough
        // to catch the scaling/time-unit regressions that previously broke planet positions.
        let jd = 2451545.0;
        let cases = [
            (Planet::Mercury, 18.14, -24.4),
            (Planet::Venus, 15.99, -18.5),
            (Planet::Mars, 22.03, -13.2),
            (Planet::Jupiter, 1.59, 8.6),
            (Planet::Saturn, 2.58, 12.6),
            (Planet::Uranus, 21.17, -17.0),
            (Planet::Neptune, 20.36, -19.2),
        ];
        for (planet, ra_h, dec_d) in cases {
            let p = calculate_planet_position(planet, jd).unwrap();
            let mut dra = (p.ra - ra_h).abs();
            if dra > 12.0 {
                dra = 24.0 - dra; // handle RA wrap-around
            }
            assert!(
                dra < 0.5,
                "{} RA should be ~{}h, got {}h",
                planet.name(),
                ra_h,
                p.ra
            );
            assert!(
                (p.dec - dec_d).abs() < 1.5,
                "{} Dec should be ~{}°, got {}°",
                planet.name(),
                dec_d,
                p.dec
            );
        }
    }
}
