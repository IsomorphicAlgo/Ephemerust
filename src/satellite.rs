//! Satellite tracking and pass prediction.
//!
//! This module builds the conversion and prediction layer on top of the
//! [`sgp4`](https://crates.io/crates/sgp4) crate, which provides the validated SGP4/SDP4
//! propagator. The propagator is treated as the numerical engine; this module owns the
//! surrounding pipeline: TLE ingestion, frame conversion, observer-relative geometry, pass
//! prediction, and ground tracks.
//!
//! # Frame and unit conventions
//!
//! - The propagator emits position and velocity in the **True Equator, Mean Equinox
//!   (TEME)** frame of epoch — an Earth-centered inertial frame that does not rotate with
//!   the Earth and is distinct from J2000/ICRS.
//! - The supported conversion path is **TEME → ECEF** via a Greenwich sidereal-time
//!   rotation about the Z axis (reusing [`crate::time`]), followed by **ECEF → geodetic**
//!   latitude/longitude/altitude on the WGS84 ellipsoid.
//! - Precession and nutation are intentionally omitted, consistent with the project's
//!   existing accuracy posture (see `docs/accuracy-and-limits.md`). The propagator's gravity
//!   model is WGS72 while geodetic conversion uses WGS84; the resulting inconsistency is
//!   within the documented error budget.
//! - Positions are kilometres, velocities kilometres per second, angles degrees, and times
//!   UTC unless stated otherwise.
//!
//! The staged implementation plan lives in `docs/satellite-tracking-plan.md`. This module is
//! currently at **Milestone 1** (TLE ingestion & parsing): [`Tle`] parses and validates
//! 2- and 3-line element sets, exposing the catalog number, epoch, and orbital elements.
//! Propagation (Milestone 2) and later stages remain stubs.
//!
//! # Example
//!
//! ```
//! use cli_astro_calc::satellite::Tle;
//!
//! let tle = Tle::parse(
//!     "ISS (ZARYA)\n\
//!      1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
//!      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
//! )
//! .unwrap();
//!
//! assert_eq!(tle.catalog_number, 25544);
//! assert!((tle.inclination_deg - 51.6461).abs() < 1e-6);
//! ```

use crate::{AstroError, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};

/// A parsed and validated Two-Line Element set.
///
/// The original element-set lines are retained (`name`, `line1`, `line2`) so the propagation
/// engine can re-parse them directly in Milestone 2, while the typed fields provide an
/// inspectable, documented view of the orbital elements.
///
/// Angles are in degrees, the mean motion is in revolutions per day, and the epoch is UTC.
#[derive(Debug, Clone, PartialEq)]
pub struct Tle {
    /// Optional object name (from the leading line of a 3-line element set).
    pub name: Option<String>,
    /// First data line of the element set (69 columns).
    pub line1: String,
    /// Second data line of the element set (69 columns).
    pub line2: String,

    /// NORAD satellite catalog number.
    pub catalog_number: u32,
    /// Classification: `U` (unclassified), `C` (classified), or `S` (secret).
    pub classification: char,
    /// International designator (launch year, number, and piece), e.g. `98067A`.
    pub international_designator: String,
    /// Epoch of the element set (UTC).
    pub epoch: DateTime<Utc>,

    /// First derivative of mean motion divided by two, in revolutions per day squared.
    pub mean_motion_dot: f64,
    /// Second derivative of mean motion divided by six, in revolutions per day cubed.
    pub mean_motion_ddot: f64,
    /// B* radiation-pressure/drag coefficient, in inverse Earth radii.
    pub bstar: f64,
    /// Element set number.
    pub element_set_number: u32,

    /// Inclination in degrees.
    pub inclination_deg: f64,
    /// Right ascension of the ascending node in degrees.
    pub raan_deg: f64,
    /// Orbital eccentricity (dimensionless).
    pub eccentricity: f64,
    /// Argument of perigee in degrees.
    pub arg_perigee_deg: f64,
    /// Mean anomaly in degrees.
    pub mean_anomaly_deg: f64,
    /// Mean motion in revolutions per day.
    pub mean_motion: f64,
    /// Revolution number at epoch.
    pub revolution_number: u32,
}

/// Position and velocity in the TEME frame of epoch.
///
/// Position is in kilometres and velocity in kilometres per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemeState {
    /// Position vector `[x, y, z]` in kilometres (TEME).
    pub position_km: [f64; 3],
    /// Velocity vector `[vx, vy, vz]` in kilometres per second (TEME).
    pub velocity_km_s: [f64; 3],
}

/// A sub-satellite (ground) point: the geodetic position directly beneath the satellite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Subpoint {
    /// Geodetic latitude in degrees, positive north.
    pub latitude_deg: f64,
    /// Geodetic longitude in degrees, positive east.
    pub longitude_deg: f64,
    /// Height above the WGS84 ellipsoid in kilometres.
    pub altitude_km: f64,
}

/// Observer-relative look angles for pointing and visibility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAngles {
    /// Azimuth in degrees, measured clockwise from true north.
    pub azimuth_deg: f64,
    /// Elevation above the local horizon in degrees.
    pub elevation_deg: f64,
    /// Slant range from observer to satellite in kilometres.
    pub range_km: f64,
    /// Range rate in kilometres per second (negative while approaching).
    pub range_rate_km_s: f64,
}

/// A single visible pass of a satellite over an observer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pass {
    /// Acquisition of signal: the satellite crosses the elevation mask while rising.
    pub aos: DateTime<Utc>,
    /// Culmination: the moment of maximum elevation during the pass.
    pub culmination: DateTime<Utc>,
    /// Loss of signal: the satellite drops below the elevation mask.
    pub los: DateTime<Utc>,
    /// Maximum elevation reached during the pass, in degrees.
    pub max_elevation_deg: f64,
}

impl Tle {
    /// Parses a TLE from text, accepting either a 2-line set or a 3-line set whose first
    /// line is the object name.
    ///
    /// Blank lines are ignored, and a leading `0 ` on a title line (as used by some
    /// catalogs) is stripped.
    pub fn parse(text: &str) -> Result<Self> {
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim_end_matches(['\r', '\n']))
            .filter(|l| !l.trim().is_empty())
            .collect();

        match lines.as_slice() {
            [l1, l2] => Self::from_lines(None, l1, l2),
            [name, l1, l2] => {
                let name = name.trim().strip_prefix("0 ").unwrap_or(name.trim()).trim();
                Self::from_lines(Some(name), l1, l2)
            }
            other => Err(AstroError::SatelliteError(format!(
                "expected a 2-line or 3-line element set, found {} non-empty line(s)",
                other.len()
            ))),
        }
    }

    /// Reads and parses a TLE from a file path.
    pub fn from_file(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Builds a [`Tle`] from an optional name and the two data lines, validating structure,
    /// checksums, and each field.
    pub fn from_lines(name: Option<&str>, line1: &str, line2: &str) -> Result<Self> {
        let l1 = validate_line(line1, '1')?;
        let l2 = validate_line(line2, '2')?;

        let catalog_1 = parse_u32(col(l1, 3, 7), "satellite catalog number (line 1)")?;
        let catalog_2 = parse_u32(col(l2, 3, 7), "satellite catalog number (line 2)")?;
        if catalog_1 != catalog_2 {
            return Err(AstroError::SatelliteError(format!(
                "catalog numbers differ between lines: {catalog_1} (line 1) vs {catalog_2} (line 2)"
            )));
        }

        let classification = col(l1, 8, 8).chars().next().unwrap_or('U');
        let international_designator = col(l1, 10, 17).trim().to_string();

        let epoch_year = parse_u32(col(l1, 19, 20), "epoch year")?;
        let epoch_day = parse_f64_loose(col(l1, 21, 32), "epoch day-of-year")?;
        let epoch = parse_epoch(epoch_year, epoch_day)?;

        let mean_motion_dot = parse_f64_loose(col(l1, 34, 43), "first derivative of mean motion")?;
        let mean_motion_ddot = parse_assumed_exp(col(l1, 45, 52), "second derivative of mean motion")?;
        let bstar = parse_assumed_exp(col(l1, 54, 61), "B* drag term")?;
        let element_set_number = parse_u32(col(l1, 65, 68), "element set number")?;

        let inclination_deg = parse_f64_loose(col(l2, 9, 16), "inclination")?;
        let raan_deg = parse_f64_loose(col(l2, 18, 25), "right ascension of ascending node")?;
        let eccentricity = parse_eccentricity(col(l2, 27, 33))?;
        let arg_perigee_deg = parse_f64_loose(col(l2, 35, 42), "argument of perigee")?;
        let mean_anomaly_deg = parse_f64_loose(col(l2, 44, 51), "mean anomaly")?;
        let mean_motion = parse_f64_loose(col(l2, 53, 63), "mean motion")?;
        let revolution_number = parse_u32(col(l2, 64, 68), "revolution number")?;

        Ok(Tle {
            name: name.map(|s| s.to_string()),
            line1: l1.to_string(),
            line2: l2.to_string(),
            catalog_number: catalog_1,
            classification,
            international_designator,
            epoch,
            mean_motion_dot,
            mean_motion_ddot,
            bstar,
            element_set_number,
            inclination_deg,
            raan_deg,
            eccentricity,
            arg_perigee_deg,
            mean_anomaly_deg,
            mean_motion,
            revolution_number,
        })
    }
}

/// Validates one element-set line: ASCII, length, line number, and checksum. Returns the
/// canonical 69-column slice on success.
fn validate_line(line: &str, expected_number: char) -> Result<&str> {
    let trimmed = line.trim_end();
    if !trimmed.is_ascii() {
        return Err(AstroError::SatelliteError(format!(
            "line {expected_number} contains non-ASCII characters"
        )));
    }
    if trimmed.len() < 69 {
        return Err(AstroError::SatelliteError(format!(
            "line {expected_number} is too short: expected 69 columns, found {}",
            trimmed.len()
        )));
    }
    let line = &trimmed[..69];

    let actual_number = line.chars().next().unwrap();
    if actual_number != expected_number {
        return Err(AstroError::SatelliteError(format!(
            "expected line to start with '{expected_number}', found '{actual_number}'"
        )));
    }

    let expected_checksum = tle_checksum(line);
    let stated_checksum = col(line, 69, 69)
        .parse::<u32>()
        .map_err(|_| AstroError::SatelliteError(format!(
            "line {expected_number} checksum column is not a digit: '{}'",
            col(line, 69, 69)
        )))?;
    if expected_checksum != stated_checksum {
        return Err(AstroError::SatelliteError(format!(
            "line {expected_number} checksum mismatch: computed {expected_checksum}, stated {stated_checksum}"
        )));
    }

    Ok(line)
}

/// Computes the modulo-10 TLE checksum over columns 1–68: digits add their value, minus
/// signs add one, and all other characters add zero.
fn tle_checksum(line: &str) -> u32 {
    line.chars()
        .take(68)
        .map(|c| match c {
            '0'..='9' => c.to_digit(10).unwrap(),
            '-' => 1,
            _ => 0,
        })
        .sum::<u32>()
        % 10
}

/// Returns the 1-indexed, inclusive column range `[start, end]` of a validated line.
fn col(line: &str, start: usize, end: usize) -> &str {
    &line[start - 1..end]
}

fn parse_u32(field: &str, name: &str) -> Result<u32> {
    field.trim().parse::<u32>().map_err(|_| {
        AstroError::SatelliteError(format!("could not parse {name} from '{}'", field.trim()))
    })
}

/// Parses a decimal that may use a leading decimal point (e.g. `-.00002218`).
fn parse_f64_loose(field: &str, name: &str) -> Result<f64> {
    let s = field.trim();
    if s.is_empty() {
        return Ok(0.0);
    }
    let s = s.strip_prefix('+').unwrap_or(s);
    let normalized = if let Some(rest) = s.strip_prefix("-.") {
        format!("-0.{rest}")
    } else if let Some(rest) = s.strip_prefix('.') {
        format!("0.{rest}")
    } else {
        s.to_string()
    };
    normalized.parse::<f64>().map_err(|_| {
        AstroError::SatelliteError(format!("could not parse {name} from '{}'", field.trim()))
    })
}

/// Parses the TLE "assumed decimal point with exponent" notation (e.g. `-31515-4` →
/// `-0.31515 × 10⁻⁴`). An empty or all-zero field yields `0.0`.
fn parse_assumed_exp(field: &str, name: &str) -> Result<f64> {
    let s = field.trim();
    if s.is_empty() {
        return Ok(0.0);
    }
    let (sign, body) = match s.as_bytes()[0] {
        b'-' => (-1.0, &s[1..]),
        b'+' => (1.0, &s[1..]),
        _ => (1.0, s),
    };
    let exp_pos = body.rfind(|c| c == '+' || c == '-').ok_or_else(|| {
        AstroError::SatelliteError(format!("could not parse {name} (missing exponent) from '{s}'"))
    })?;
    let (mantissa_digits, exp_str) = body.split_at(exp_pos);
    if mantissa_digits.is_empty() {
        return Err(AstroError::SatelliteError(format!(
            "could not parse {name} (empty mantissa) from '{s}'"
        )));
    }
    let mantissa = format!("0.{mantissa_digits}")
        .parse::<f64>()
        .map_err(|_| AstroError::SatelliteError(format!("could not parse {name} mantissa from '{s}'")))?;
    let exponent = exp_str
        .parse::<i32>()
        .map_err(|_| AstroError::SatelliteError(format!("could not parse {name} exponent from '{s}'")))?;
    Ok(sign * mantissa * 10f64.powi(exponent))
}

/// Parses the eccentricity field, which carries an implied leading decimal point
/// (e.g. `0001413` → `0.0001413`).
fn parse_eccentricity(field: &str) -> Result<f64> {
    let s = field.trim();
    format!("0.{s}").parse::<f64>().map_err(|_| {
        AstroError::SatelliteError(format!("could not parse eccentricity from '{s}'"))
    })
}

/// Converts a 2-digit TLE epoch year and fractional day-of-year into a UTC datetime.
///
/// Per TLE convention, years 57–99 map to 1957–1999 and 00–56 map to 2000–2056.
fn parse_epoch(two_digit_year: u32, day_of_year: f64) -> Result<DateTime<Utc>> {
    let year = if two_digit_year < 57 {
        2000 + two_digit_year as i32
    } else {
        1900 + two_digit_year as i32
    };
    if !(day_of_year.is_finite() && day_of_year >= 1.0 && day_of_year < 367.0) {
        return Err(AstroError::SatelliteError(format!(
            "epoch day-of-year out of range: {day_of_year}"
        )));
    }
    let ordinal = day_of_year.floor() as u32;
    let fraction = day_of_year - ordinal as f64;
    let date = NaiveDate::from_yo_opt(year, ordinal).ok_or_else(|| {
        AstroError::SatelliteError(format!("invalid epoch date: year {year}, day {ordinal}"))
    })?;
    let nanos = (fraction * 86_400.0 * 1e9).round() as i64;
    let naive = date.and_hms_opt(0, 0, 0).unwrap() + chrono::Duration::nanoseconds(nanos);
    Ok(Utc.from_utc_datetime(&naive))
}

/// Propagates a TLE to the given UTC time and returns the TEME state.
///
/// Stub for Milestone 1. The implementation is delivered in Milestone 2, which wraps the
/// `sgp4` propagator behind this signature.
pub fn propagate(_tle: &Tle, _time: DateTime<Utc>) -> Result<TemeState> {
    Err(AstroError::SatelliteError(
        "propagation is not yet implemented (planned for Milestone 2)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    // Canonical ISS (ZARYA) element set, reused as a fixed reference across the suite.
    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_LINE1: &str =
        "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
    const ISS_LINE2: &str =
        "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

    // A second, older ISS element set (from the sgp4 crate documentation) for cross-checks.
    const ISS_2008_LINE1: &str =
        "1 25544U 98067A   08264.51782528 -.00002182  00000-0 -11606-4 0  2927";
    const ISS_2008_LINE2: &str =
        "2 25544  51.6416 247.4627 0006703 130.5360 325.0288 15.72125391563537";

    fn iss_3line() -> String {
        format!("{ISS_NAME}\n{ISS_LINE1}\n{ISS_LINE2}")
    }

    #[test]
    fn parses_all_iss_fields() {
        let tle = Tle::parse(&iss_3line()).expect("ISS TLE should parse");

        assert_eq!(tle.name.as_deref(), Some("ISS (ZARYA)"));
        assert_eq!(tle.catalog_number, 25544);
        assert_eq!(tle.classification, 'U');
        assert_eq!(tle.international_designator, "98067A");
        assert_eq!(tle.element_set_number, 999);
        assert_eq!(tle.revolution_number, 23600);

        assert!((tle.inclination_deg - 51.6461).abs() < 1e-9);
        assert!((tle.raan_deg - 221.2784).abs() < 1e-9);
        assert!((tle.eccentricity - 0.0001413).abs() < 1e-12);
        assert!((tle.arg_perigee_deg - 89.1723).abs() < 1e-9);
        assert!((tle.mean_anomaly_deg - 280.4612).abs() < 1e-9);
        assert!((tle.mean_motion - 15.49507896).abs() < 1e-9);

        assert!((tle.mean_motion_dot - (-0.00002218)).abs() < 1e-12);
        assert!(tle.mean_motion_ddot.abs() < 1e-30);
        assert!((tle.bstar - (-3.1515e-5)).abs() < 1e-12);

        assert_eq!(tle.epoch.year(), 2020);
        assert_eq!(tle.epoch.ordinal(), 194);
        // Day fraction 0.88612269 → ~21:16:01 UTC.
        assert_eq!(tle.epoch.hour(), 21);
        assert_eq!(tle.epoch.minute(), 16);
    }

    #[test]
    fn parses_two_line_set_without_name() {
        let tle = Tle::parse(&format!("{ISS_LINE1}\n{ISS_LINE2}")).expect("2-line set should parse");
        assert!(tle.name.is_none());
        assert_eq!(tle.catalog_number, 25544);
    }

    #[test]
    fn strips_leading_zero_on_title_line() {
        let text = format!("0 {ISS_NAME}\n{ISS_LINE1}\n{ISS_LINE2}");
        let tle = Tle::parse(&text).expect("3-line set with '0 ' title should parse");
        assert_eq!(tle.name.as_deref(), Some("ISS (ZARYA)"));
    }

    #[test]
    fn rejects_corrupted_checksum() {
        // Flip the final checksum digit of line 1 (…9992 → …9990).
        let bad_line1 = format!("{}0", &ISS_LINE1[..ISS_LINE1.len() - 1]);
        let err = Tle::from_lines(Some(ISS_NAME), &bad_line1, ISS_LINE2)
            .expect_err("corrupted checksum must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("checksum mismatch"), "unexpected error: {msg}");
    }

    #[test]
    fn rejects_wrong_line_count() {
        let err = Tle::parse(ISS_LINE1).expect_err("single line must be rejected");
        assert!(err.to_string().contains("2-line or 3-line"));
    }

    #[test]
    fn rejects_short_line() {
        let err = Tle::from_lines(None, "1 25544U", ISS_LINE2)
            .expect_err("truncated line must be rejected");
        assert!(err.to_string().contains("too short"));
    }

    #[test]
    fn rejects_wrong_line_number() {
        // Pass line 2 where line 1 is expected.
        let err = Tle::from_lines(None, ISS_LINE2, ISS_LINE2)
            .expect_err("misordered lines must be rejected");
        assert!(err.to_string().contains("start with '1'"));
    }

    #[test]
    fn cross_check_fields_against_sgp4_parser() {
        for (l1, l2) in [(ISS_LINE1, ISS_LINE2), (ISS_2008_LINE1, ISS_2008_LINE2)] {
            let tle = Tle::from_lines(None, l1, l2).expect("reference TLE should parse");
            let elements = sgp4::Elements::from_tle(None, l1.as_bytes(), l2.as_bytes())
                .expect("sgp4 should parse reference TLE");

            assert_eq!(tle.catalog_number as u64, elements.norad_id);
            assert_eq!(tle.revolution_number as u64, elements.revolution_number);
            assert_eq!(tle.element_set_number as u64, elements.element_set_number);

            assert!((tle.inclination_deg - elements.inclination).abs() < 1e-6);
            assert!((tle.raan_deg - elements.right_ascension).abs() < 1e-6);
            assert!((tle.eccentricity - elements.eccentricity).abs() < 1e-9);
            assert!((tle.arg_perigee_deg - elements.argument_of_perigee).abs() < 1e-6);
            assert!((tle.mean_anomaly_deg - elements.mean_anomaly).abs() < 1e-6);
            assert!((tle.mean_motion - elements.mean_motion).abs() < 1e-9);
            assert!((tle.mean_motion_dot - elements.mean_motion_dot).abs() < 1e-12);
            assert!((tle.bstar - elements.drag_term).abs() < 1e-12);

            // Epochs should agree to within a millisecond.
            let delta = tle.epoch.naive_utc() - elements.datetime;
            assert!(
                delta.num_microseconds().map(|us| us.abs() < 1_000).unwrap_or(false),
                "epoch mismatch: {} vs {}",
                tle.epoch.naive_utc(),
                elements.datetime
            );
        }
    }

    #[test]
    fn iss_propagation_yields_plausible_leo_radius() {
        // Milestone 0 smoke test: the sgp4 engine is wired in and produces a plausible state.
        let elements = sgp4::Elements::from_tle(
            Some(ISS_NAME.to_owned()),
            ISS_LINE1.as_bytes(),
            ISS_LINE2.as_bytes(),
        )
        .expect("canonical ISS TLE should parse");
        let constants =
            sgp4::Constants::from_elements(&elements).expect("ISS elements should be valid");
        let prediction = constants
            .propagate(sgp4::MinutesSinceEpoch(0.0))
            .expect("propagation at epoch should succeed");

        let [x, y, z] = prediction.position;
        let radius_km = (x * x + y * y + z * z).sqrt();
        assert!(
            (6_600.0..=7_100.0).contains(&radius_km),
            "ISS geocentric radius {radius_km:.1} km outside expected low-Earth-orbit band"
        );
    }
}
