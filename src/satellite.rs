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
//! use ephemerust::satellite::Tle;
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
use thiserror::Error;

/// A structured, *educational* error describing why a Two-Line Element set could not be
/// parsed.
///
/// Every variant is designed to be a teaching moment: its [`Display`](std::fmt::Display)
/// message states **what was expected**, **what was found**, and **the underlying rule** of
/// the TLE format, and [`TleError::hint`] offers a short, actionable next step. The TLE
/// format is unforgiving precisely because it is *fixed-column*: each field is read from an
/// exact character range, so a single shifted or altered character changes the meaning of
/// everything after it.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TleError {
    /// The input did not contain exactly two data lines (optionally preceded by a name).
    #[error(
        "expected a 2-line or 3-line element set, but found {found} non-empty line(s). \
         A TLE is two 69-column data lines, optionally preceded by a name line (the common \
         \"3-line\" form)"
    )]
    WrongLineCount {
        /// Number of non-empty lines actually found.
        found: usize,
    },

    /// A data line contained a non-ASCII character.
    #[error(
        "TLE line {line} contains a non-ASCII character. TLE lines are strictly ASCII with \
         fixed column positions; a stray Unicode character (a smart quote or non-breaking \
         space, for example) shifts every field that follows it"
    )]
    NonAscii {
        /// Offending line number (1 or 2).
        line: u8,
    },

    /// A data line was shorter than the required 69 columns.
    #[error(
        "TLE line {line} has {found} columns, but 69 are required. TLE lines use strict \
         fixed-column widths, so each field is read by position — a short line means a field \
         has been truncated or trailing spaces were dropped"
    )]
    LineTooShort {
        /// Offending line number (1 or 2).
        line: u8,
        /// Number of columns actually found.
        found: usize,
    },

    /// A data line did not begin with its expected line-number digit.
    #[error(
        "expected TLE line {expected} to begin with '{expected}', but it begins with \
         '{found}'. The first column of each data line is its line number (1 or 2); the two \
         lines may be swapped or out of order"
    )]
    WrongLineNumber {
        /// The digit the line was expected to start with ('1' or '2').
        expected: char,
        /// The character actually found in column 1.
        found: char,
    },

    /// The checksum column held a non-digit character.
    #[error(
        "the checksum column (column 69) of TLE line {line} is '{found}', which is not a \
         digit. Each data line ends with a single modulo-10 check digit"
    )]
    ChecksumNotDigit {
        /// Offending line number (1 or 2).
        line: u8,
        /// The non-digit text found in the checksum column.
        found: String,
    },

    /// The computed checksum did not match the stated check digit.
    #[error(
        "TLE line {line} fails its checksum: the line's columns sum to {computed} (mod 10), \
         but the stated check digit is {stated}. The modulo-10 checksum adds each column \
         (digits add their value, a minus sign adds 1, everything else adds 0); a mismatch \
         usually means a character was altered in transit"
    )]
    ChecksumMismatch {
        /// Offending line number (1 or 2).
        line: u8,
        /// Checksum computed from columns 1–68.
        computed: u32,
        /// Check digit stated in column 69.
        stated: u32,
    },

    /// The two lines carried different satellite catalog numbers.
    #[error(
        "the two lines describe different satellites: line 1 catalog number is {line1}, but \
         line 2 is {line2}. Both lines of a set must carry the same catalog number — lines \
         from two different element sets may have been combined"
    )]
    CatalogMismatch {
        /// Catalog number read from line 1.
        line1: u32,
        /// Catalog number read from line 2.
        line2: u32,
    },

    /// A specific field could not be parsed; names the field and its column range.
    #[error(
        "could not parse the {field} field (line {line}, columns {start}-{end}) from \
         '{found}': {reason}"
    )]
    Field {
        /// Human-readable field name.
        field: String,
        /// Line the field lives on (1 or 2).
        line: u8,
        /// 1-indexed start column (inclusive).
        start: usize,
        /// 1-indexed end column (inclusive).
        end: usize,
        /// The raw text that failed to parse.
        found: String,
        /// Why the parse failed, including the expected shape.
        reason: String,
    },

    /// The epoch day-of-year was outside the valid `[1.0, 367.0)` range.
    #[error(
        "the epoch day-of-year is {day_of_year}, which is out of range. The TLE epoch is a \
         fractional day-of-year in [1.0, 366.99…]; a value outside that range indicates a \
         corrupted epoch field"
    )]
    EpochOutOfRange {
        /// The out-of-range day-of-year value.
        day_of_year: f64,
    },

    /// The epoch year and day-of-year did not resolve to a real calendar date.
    #[error("the epoch does not resolve to a valid calendar date (year {year}, day-of-year {ordinal})")]
    InvalidEpochDate {
        /// Reconstructed four-digit year.
        year: i32,
        /// Day-of-year ordinal.
        ordinal: u32,
    },
}

impl TleError {
    /// Returns a short, actionable formatting hint for this error, suitable for display on a
    /// dedicated `Hint:` line after the error message.
    pub fn hint(&self) -> Option<&'static str> {
        Some(match self {
            TleError::WrongLineCount { .. } => {
                "Provide exactly two data lines (or a name line plus two data lines), and quote \
                 the whole set so the line breaks are preserved."
            }
            TleError::NonAscii { .. } => {
                "Re-copy the element set as plain text, replacing smart quotes and non-breaking \
                 spaces with plain ASCII characters."
            }
            TleError::LineTooShort { .. } => {
                "A complete TLE data line is exactly 69 characters, including any trailing spaces."
            }
            TleError::WrongLineNumber { .. } => {
                "Line 1 must start with '1' and line 2 with '2'; check that the lines are in order."
            }
            TleError::ChecksumMismatch { .. } | TleError::ChecksumNotDigit { .. } => {
                "Re-download the element set from its source; a single altered character breaks \
                 the check digit."
            }
            TleError::CatalogMismatch { .. } => {
                "Make sure both lines come from the same element set for a single satellite."
            }
            TleError::Field { .. } => {
                "Compare the field against the fixed TLE column layout in \
                 docs/satellite-tracking-plan.md."
            }
            TleError::EpochOutOfRange { .. } | TleError::InvalidEpochDate { .. } => {
                "The epoch is a 2-digit year followed by a fractional day-of-year (001.0–366.x)."
            }
        })
    }
}

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
            other => Err(TleError::WrongLineCount { found: other.len() }.into()),
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
        let l1 = validate_line(line1, 1)?;
        let l2 = validate_line(line2, 2)?;

        let catalog_1 = parse_u32(&FieldSpec::new("satellite catalog number", 1, 3, 7), l1)?;
        let catalog_2 = parse_u32(&FieldSpec::new("satellite catalog number", 2, 3, 7), l2)?;
        if catalog_1 != catalog_2 {
            return Err(TleError::CatalogMismatch { line1: catalog_1, line2: catalog_2 }.into());
        }

        let classification = col(l1, 8, 8).chars().next().unwrap_or('U');
        let international_designator = col(l1, 10, 17).trim().to_string();

        let epoch_year = parse_u32(&FieldSpec::new("epoch year", 1, 19, 20), l1)?;
        let epoch_day = parse_f64_loose(&FieldSpec::new("epoch day-of-year", 1, 21, 32), l1)?;
        let epoch = parse_epoch(epoch_year, epoch_day)?;

        let mean_motion_dot =
            parse_f64_loose(&FieldSpec::new("first derivative of mean motion", 1, 34, 43), l1)?;
        let mean_motion_ddot =
            parse_assumed_exp(&FieldSpec::new("second derivative of mean motion", 1, 45, 52), l1)?;
        let bstar = parse_assumed_exp(&FieldSpec::new("B* drag term", 1, 54, 61), l1)?;
        let element_set_number =
            parse_u32(&FieldSpec::new("element set number", 1, 65, 68), l1)?;

        let inclination_deg = parse_f64_loose(&FieldSpec::new("inclination", 2, 9, 16), l2)?;
        let raan_deg =
            parse_f64_loose(&FieldSpec::new("right ascension of ascending node", 2, 18, 25), l2)?;
        let eccentricity = parse_eccentricity(&FieldSpec::new("eccentricity", 2, 27, 33), l2)?;
        let arg_perigee_deg =
            parse_f64_loose(&FieldSpec::new("argument of perigee", 2, 35, 42), l2)?;
        let mean_anomaly_deg = parse_f64_loose(&FieldSpec::new("mean anomaly", 2, 44, 51), l2)?;
        let mean_motion = parse_f64_loose(&FieldSpec::new("mean motion", 2, 53, 63), l2)?;
        let revolution_number =
            parse_u32(&FieldSpec::new("revolution number", 2, 64, 68), l2)?;

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
fn validate_line(line: &str, line_no: u8) -> std::result::Result<&str, TleError> {
    let expected_number = (b'0' + line_no) as char;
    let trimmed = line.trim_end();
    if !trimmed.is_ascii() {
        return Err(TleError::NonAscii { line: line_no });
    }
    if trimmed.len() < 69 {
        return Err(TleError::LineTooShort { line: line_no, found: trimmed.len() });
    }
    let line = &trimmed[..69];

    let actual_number = line.chars().next().unwrap();
    if actual_number != expected_number {
        return Err(TleError::WrongLineNumber { expected: expected_number, found: actual_number });
    }

    let computed = tle_checksum(line);
    let check_text = col(line, 69, 69);
    let stated = check_text
        .parse::<u32>()
        .map_err(|_| TleError::ChecksumNotDigit { line: line_no, found: check_text.to_string() })?;
    if computed != stated {
        return Err(TleError::ChecksumMismatch { line: line_no, computed, stated });
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

/// Describes one fixed-column TLE field: its human-readable name, which line it lives on, and
/// its 1-indexed inclusive column range. Centralizing this lets parse failures report exactly
/// where the field is and what it should look like.
struct FieldSpec {
    name: &'static str,
    line: u8,
    start: usize,
    end: usize,
}

impl FieldSpec {
    fn new(name: &'static str, line: u8, start: usize, end: usize) -> Self {
        FieldSpec { name, line, start, end }
    }

    /// Extracts this field's raw text from a validated 69-column line.
    fn text<'a>(&self, line: &'a str) -> &'a str {
        col(line, self.start, self.end)
    }

    /// Builds a [`TleError::Field`] for this field, recording what was found and why it failed.
    fn error(&self, found: &str, reason: &str) -> TleError {
        TleError::Field {
            field: self.name.to_string(),
            line: self.line,
            start: self.start,
            end: self.end,
            found: found.to_string(),
            reason: reason.to_string(),
        }
    }
}

fn parse_u32(spec: &FieldSpec, line: &str) -> std::result::Result<u32, TleError> {
    let text = spec.text(line);
    text.trim()
        .parse::<u32>()
        .map_err(|_| spec.error(text.trim(), "expected a base-10 integer"))
}

/// Parses a decimal that may use a leading decimal point (e.g. `-.00002218`).
fn parse_f64_loose(spec: &FieldSpec, line: &str) -> std::result::Result<f64, TleError> {
    let s = spec.text(line).trim();
    if s.is_empty() {
        return Ok(0.0);
    }
    let body = s.strip_prefix('+').unwrap_or(s);
    let normalized = if let Some(rest) = body.strip_prefix("-.") {
        format!("-0.{rest}")
    } else if let Some(rest) = body.strip_prefix('.') {
        format!("0.{rest}")
    } else {
        body.to_string()
    };
    normalized
        .parse::<f64>()
        .map_err(|_| spec.error(s, "expected a decimal number (an implied leading point is allowed, e.g. `-.00002218`)"))
}

/// Parses the TLE "assumed decimal point with exponent" notation (e.g. `-31515-4` →
/// `-0.31515 × 10⁻⁴`). An empty or all-zero field yields `0.0`.
fn parse_assumed_exp(spec: &FieldSpec, line: &str) -> std::result::Result<f64, TleError> {
    let s = spec.text(line).trim();
    if s.is_empty() {
        return Ok(0.0);
    }
    let (sign, body) = match s.as_bytes()[0] {
        b'-' => (-1.0, &s[1..]),
        b'+' => (1.0, &s[1..]),
        _ => (1.0, s),
    };
    let exp_pos = body.rfind(['+', '-']).ok_or_else(|| {
        spec.error(s, "missing exponent sign in assumed-decimal notation (e.g. `-31515-4` means -0.31515e-4)")
    })?;
    let (mantissa_digits, exp_str) = body.split_at(exp_pos);
    if mantissa_digits.is_empty() {
        return Err(spec.error(s, "empty mantissa in assumed-decimal notation"));
    }
    let mantissa = format!("0.{mantissa_digits}")
        .parse::<f64>()
        .map_err(|_| spec.error(s, "non-numeric mantissa in assumed-decimal notation"))?;
    let exponent = exp_str
        .parse::<i32>()
        .map_err(|_| spec.error(s, "non-numeric exponent in assumed-decimal notation"))?;
    Ok(sign * mantissa * 10f64.powi(exponent))
}

/// Parses the eccentricity field, which carries an implied leading decimal point
/// (e.g. `0001413` → `0.0001413`).
fn parse_eccentricity(spec: &FieldSpec, line: &str) -> std::result::Result<f64, TleError> {
    let s = spec.text(line).trim();
    format!("0.{s}")
        .parse::<f64>()
        .map_err(|_| spec.error(s, "expected an implied-decimal fraction of digits only (e.g. `0001413` → 0.0001413)"))
}

/// Converts a 2-digit TLE epoch year and fractional day-of-year into a UTC datetime.
///
/// Per TLE convention, years 57–99 map to 1957–1999 and 00–56 map to 2000–2056.
fn parse_epoch(two_digit_year: u32, day_of_year: f64) -> std::result::Result<DateTime<Utc>, TleError> {
    let year = if two_digit_year < 57 {
        2000 + two_digit_year as i32
    } else {
        1900 + two_digit_year as i32
    };
    if !(day_of_year.is_finite() && (1.0..367.0).contains(&day_of_year)) {
        return Err(TleError::EpochOutOfRange { day_of_year });
    }
    let ordinal = day_of_year.floor() as u32;
    let fraction = day_of_year - ordinal as f64;
    let date = NaiveDate::from_yo_opt(year, ordinal)
        .ok_or(TleError::InvalidEpochDate { year, ordinal })?;
    let nanos = (fraction * 86_400.0 * 1e9).round() as i64;
    let naive = date.and_hms_opt(0, 0, 0).unwrap() + chrono::Duration::nanoseconds(nanos);
    Ok(Utc.from_utc_datetime(&naive))
}

/// Propagates a TLE to the given UTC time and returns the [`TemeState`].
///
/// The orbital elements are re-parsed from the original element-set lines and handed to the
/// `sgp4` engine, which performs the SGP4/SDP4 propagation. The target time is converted to
/// the engine's minutes-since-epoch representation; the result is the satellite's position
/// (km) and velocity (km/s) in the TEME frame of epoch.
///
/// # Errors
///
/// Returns [`AstroError::SatelliteError`] if the element set cannot be parsed or yields
/// invalid epoch constants, if the target time is too far from the epoch to represent
/// (nanosecond overflow), or if the propagation itself diverges (for example, a decayed
/// orbit or an eccentricity driven out of range).
pub fn propagate(tle: &Tle, time: DateTime<Utc>) -> Result<TemeState> {
    let elements = sgp4::Elements::from_tle(
        tle.name.clone(),
        tle.line1.as_bytes(),
        tle.line2.as_bytes(),
    )
    .map_err(|e| {
        AstroError::SatelliteError(format!("could not parse element set for propagation: {e}"))
    })?;

    let constants = sgp4::Constants::from_elements(&elements).map_err(|e| {
        AstroError::SatelliteError(format!("invalid orbital elements for propagation: {e}"))
    })?;

    let minutes = elements
        .datetime_to_minutes_since_epoch(&time.naive_utc())
        .map_err(|e| {
            AstroError::SatelliteError(format!(
                "target time is too far from the element-set epoch to represent: {e}"
            ))
        })?;

    let prediction = constants.propagate(minutes).map_err(|e| {
        AstroError::SatelliteError(format!("SGP4 propagation diverged: {e}"))
    })?;

    Ok(TemeState {
        position_km: prediction.position,
        velocity_km_s: prediction.velocity,
    })
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

    // Vallado SGP4 verification satellite 00005 (Vanguard), with published reference outputs.
    const SAT5_LINE1: &str =
        "1 00005U 58002B   00179.78495062  .00000023  00000-0  28098-4 0  4753";
    const SAT5_LINE2: &str =
        "2 00005  34.2682 348.7242 1859667 331.7664  19.3264 10.82419157413667";

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
        assert!(
            matches!(err, AstroError::Tle(TleError::ChecksumMismatch { line: 1, .. })),
            "unexpected error: {err:?}"
        );
        // The message must teach the rule, not merely report a mismatch.
        let msg = err.to_string();
        assert!(msg.contains("modulo-10"), "missing the checksum rule: {msg}");
        assert!(err.hint().is_some(), "checksum errors should carry a hint");
    }

    #[test]
    fn rejects_wrong_line_count() {
        let err = Tle::parse(ISS_LINE1).expect_err("single line must be rejected");
        assert!(matches!(err, AstroError::Tle(TleError::WrongLineCount { found: 1 })));
        assert!(err.to_string().contains("2-line or 3-line"));
    }

    #[test]
    fn rejects_short_line() {
        let err = Tle::from_lines(None, "1 25544U", ISS_LINE2)
            .expect_err("truncated line must be rejected");
        assert!(matches!(err, AstroError::Tle(TleError::LineTooShort { line: 1, .. })));
        // The message must state both the requirement and the underlying fixed-column rule.
        let msg = err.to_string();
        assert!(msg.contains("69 are required"), "missing the column requirement: {msg}");
        assert!(msg.contains("fixed-column"), "missing the fixed-column rule: {msg}");
    }

    #[test]
    fn rejects_wrong_line_number() {
        // Pass line 2 where line 1 is expected.
        let err = Tle::from_lines(None, ISS_LINE2, ISS_LINE2)
            .expect_err("misordered lines must be rejected");
        assert!(matches!(
            err,
            AstroError::Tle(TleError::WrongLineNumber { expected: '1', found: '2' })
        ));
        assert!(err.to_string().contains("begin with '1'"));
    }

    #[test]
    fn rejects_non_numeric_field_naming_columns() {
        // Corrupt the inclination field (line 2, columns 9–16) with letters, then repair the
        // check digit so the failure is attributed to field parsing rather than the checksum.
        let mut chars: Vec<char> = ISS_LINE2.chars().collect();
        for c in chars.iter_mut().take(16).skip(8) {
            *c = 'X';
        }
        let body: String = chars[..68].iter().collect();
        let line2 = format!("{body}{}", tle_checksum(&body));

        let err = Tle::from_lines(None, ISS_LINE1, &line2)
            .expect_err("a non-numeric field must be rejected");
        assert!(
            matches!(&err, AstroError::Tle(TleError::Field { field, line: 2, start: 9, end: 16, .. }) if field == "inclination"),
            "unexpected error: {err:?}"
        );
        // The message must name the field and its column range.
        let msg = err.to_string();
        assert!(msg.contains("inclination"), "missing field name: {msg}");
        assert!(msg.contains("columns 9-16"), "missing column range: {msg}");
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
    fn propagates_sat5_epoch_to_reference_teme_state() {
        let tle = Tle::from_lines(None, SAT5_LINE1, SAT5_LINE2).expect("verification TLE parses");
        let state = propagate(&tle, tle.epoch).expect("propagation at epoch succeeds");

        // Vallado SGP4 verification output for satellite 00005 at tsince = 0.
        let expected_r = [7022.46529266, -1400.08296755, 0.03995155];
        let expected_v = [1.893841015, 6.405893759, 4.534807250];

        // `propagate` uses the crate's default model (WGS84 + IAU sidereal time), whereas the
        // published tcppver reference uses the WGS72 + AFSPC model. The two agree at the
        // tens-of-metres level — far tighter than the kilometres-per-day at which TLEs
        // themselves drift — so a 0.05 km / 1e-4 km/s tolerance confirms correct propagation
        // while accommodating the documented model difference.
        for i in 0..3 {
            assert!(
                (state.position_km[i] - expected_r[i]).abs() < 0.05,
                "position[{i}]: {} vs reference {}",
                state.position_km[i],
                expected_r[i]
            );
            assert!(
                (state.velocity_km_s[i] - expected_v[i]).abs() < 1e-4,
                "velocity[{i}]: {} vs reference {}",
                state.velocity_km_s[i],
                expected_v[i]
            );
        }
    }

    #[test]
    fn afspc_mode_reproduces_reference_to_sub_metre() {
        // Confirms the ~tens-of-metres gap in `propagates_sat5_epoch_to_reference_teme_state`
        // is purely the WGS84/IAU-vs-WGS72/AFSPC model choice: in AFSPC mode the engine
        // reproduces the published reference to sub-metre precision.
        let elements =
            sgp4::Elements::from_tle(None, SAT5_LINE1.as_bytes(), SAT5_LINE2.as_bytes()).unwrap();
        let constants = sgp4::Constants::from_elements_afspc_compatibility_mode(&elements).unwrap();
        let prediction = constants
            .propagate_afspc_compatibility_mode(sgp4::MinutesSinceEpoch(0.0))
            .unwrap();

        let expected_r = [7022.46529266, -1400.08296755, 0.03995155];
        for i in 0..3 {
            assert!(
                (prediction.position[i] - expected_r[i]).abs() < 1e-3,
                "AFSPC position[{i}]: {} vs reference {}",
                prediction.position[i],
                expected_r[i]
            );
        }
    }

    #[test]
    fn wrapper_matches_engine_across_offsets() {
        for (l1, l2) in [(ISS_LINE1, ISS_LINE2), (SAT5_LINE1, SAT5_LINE2)] {
            let tle = Tle::from_lines(None, l1, l2).unwrap();
            let elements = sgp4::Elements::from_tle(None, l1.as_bytes(), l2.as_bytes()).unwrap();
            let constants = sgp4::Constants::from_elements(&elements).unwrap();

            for offset_min in [0.0_f64, 30.0, 90.0, 540.0] {
                let target =
                    tle.epoch + chrono::Duration::nanoseconds((offset_min * 60.0 * 1e9) as i64);
                let state = propagate(&tle, target).expect("propagation succeeds");
                let reference = constants
                    .propagate(sgp4::MinutesSinceEpoch(offset_min))
                    .expect("engine propagation succeeds");

                for i in 0..3 {
                    assert!(
                        (state.position_km[i] - reference.position[i]).abs() < 1e-2,
                        "position mismatch at {offset_min} min, axis {i}"
                    );
                    assert!(
                        (state.velocity_km_s[i] - reference.velocity[i]).abs() < 1e-5,
                        "velocity mismatch at {offset_min} min, axis {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn propagation_at_unrepresentable_time_errors() {
        let tle = Tle::from_lines(None, SAT5_LINE1, SAT5_LINE2).unwrap();
        // ~400 years past epoch overflows the engine's minutes-since-epoch representation.
        let far_future = tle.epoch + chrono::Duration::days(365 * 400);
        let err = propagate(&tle, far_future).expect_err("an unrepresentable time must error");
        assert!(matches!(err, AstroError::SatelliteError(_)), "unexpected error: {err}");
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
