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
//!   existing accuracy posture (see `docs/accuracy-and-limits.md`). Production propagation
//!   uses the `sgp4` crate's default **WGS84 + IAU** model; geodetic conversion uses the
//!   **WGS84** ellipsoid (aligned). Optional [`PropagationModel::AfspcCompatibility`] selects
//!   WGS72 + legacy AFSPC sidereal/epoch handling for Vallado reference reproduction.
//! - Positions are kilometres, velocities kilometres per second, angles degrees, and times
//!   UTC unless stated otherwise.
//!
//! The staged implementation plan lives in `docs/satellite-tracking-plan.md`. Companion theory
//! for the SGP4 layer is in `docs/sgp4.md`, with the non-operational [`crate::sgp4_teaching`]
//! module for Kepler / mean-motion pedagogy. Implemented so
//! far: [`Tle`] parses and validates 2- and 3-line element sets (with structured, educational
//! [`TleError`]s); [`propagate`] takes a parsed element set to a [`TemeState`] via the `sgp4`
//! engine; [`teme_to_ecef`] and [`ecef_to_geodetic`] bridge TEME to WGS84 geodetic coordinates
//! using Greenwich sidereal time (same Z-rotation convention as ECI→ECEF in [`crate::coordinates`]);
//! [`subpoint`] combines propagation with that chain for the sub-satellite point; and
//! [`look_angles`] completes the topocentric East–North–Up (ENU) pipeline for azimuth, elevation,
//! slant range, and range rate; [`predict_passes`] finds visible passes over a time window using
//! a coarse elevation scan with bisection refinement; [`ground_track`] samples the sub-satellite
//! path with [`ground_track_to_csv`] / [`ground_track_to_json`] for plotting pipelines.
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

use crate::celestial::ObserverLocation;
use crate::coordinates::{Ecef, Eci, ecef_to_geodetic_wgs84, eci_to_ecef, geodetic_wgs84_to_ecef};
use crate::{AstroError, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::Serialize;
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
    #[error(
        "the epoch does not resolve to a valid calendar date (year {year}, day-of-year {ordinal})"
    )]
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
/// engine can re-parse them directly, while the typed fields provide an
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TemeState {
    /// Position vector `[x, y, z]` in kilometres (TEME).
    pub position_km: [f64; 3],
    /// Velocity vector `[vx, vy, vz]` in kilometres per second (TEME).
    pub velocity_km_s: [f64; 3],
}

/// A sub-satellite (ground) point: the geodetic position directly beneath the satellite.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Subpoint {
    /// Geodetic latitude in degrees, positive north.
    pub latitude_deg: f64,
    /// Geodetic longitude in degrees, positive east.
    pub longitude_deg: f64,
    /// Height above the WGS84 ellipsoid in kilometres.
    pub altitude_km: f64,
}

/// Observer-relative look angles for pointing and visibility.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pass {
    /// Acquisition of signal: the satellite crosses the elevation mask while rising.
    pub aos: DateTime<Utc>,
    /// Culmination: the moment of maximum elevation during the pass.
    pub culmination: DateTime<Utc>,
    /// Loss of signal: the satellite drops below the elevation mask.
    pub los: DateTime<Utc>,
    /// Maximum elevation reached during the pass, in degrees.
    pub max_elevation_deg: f64,
    /// Azimuth at AOS (degrees, clockwise from true north).
    pub aos_azimuth_deg: f64,
    /// Azimuth at LOS (degrees, clockwise from true north).
    pub los_azimuth_deg: f64,
}

/// One sample along a satellite **ground track**: UTC time plus the corresponding [`Subpoint`].
///
/// Produced by [`ground_track`]; serialize with [`ground_track_to_csv`] or [`ground_track_to_json`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GroundTrackSample {
    /// Sample time (UTC).
    pub time: DateTime<Utc>,
    /// Geodetic sub-satellite point at `time`.
    pub subpoint: Subpoint,
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

    // (See also the `FromStr` impl below, which makes `text.parse::<Tle>()` work.)

    /// Builds a [`Tle`] from an optional name and the two data lines, validating structure,
    /// checksums, and each field.
    pub fn from_lines(name: Option<&str>, line1: &str, line2: &str) -> Result<Self> {
        let l1 = validate_line(line1, 1)?;
        let l2 = validate_line(line2, 2)?;

        let catalog_1 = parse_u32(&FieldSpec::new("satellite catalog number", 1, 3, 7), l1)?;
        let catalog_2 = parse_u32(&FieldSpec::new("satellite catalog number", 2, 3, 7), l2)?;
        if catalog_1 != catalog_2 {
            return Err(TleError::CatalogMismatch {
                line1: catalog_1,
                line2: catalog_2,
            }
            .into());
        }

        let classification = col(l1, 8, 8).chars().next().unwrap_or('U');
        let international_designator = col(l1, 10, 17).trim().to_string();

        let epoch_year = parse_u32(&FieldSpec::new("epoch year", 1, 19, 20), l1)?;
        let epoch_day = parse_f64_loose(&FieldSpec::new("epoch day-of-year", 1, 21, 32), l1)?;
        let epoch = parse_epoch(epoch_year, epoch_day)?;

        let mean_motion_dot = parse_f64_loose(
            &FieldSpec::new("first derivative of mean motion", 1, 34, 43),
            l1,
        )?;
        let mean_motion_ddot = parse_assumed_exp(
            &FieldSpec::new("second derivative of mean motion", 1, 45, 52),
            l1,
        )?;
        let bstar = parse_assumed_exp(&FieldSpec::new("B* drag term", 1, 54, 61), l1)?;
        let element_set_number = parse_u32(&FieldSpec::new("element set number", 1, 65, 68), l1)?;

        let inclination_deg = parse_f64_loose(&FieldSpec::new("inclination", 2, 9, 16), l2)?;
        let raan_deg = parse_f64_loose(
            &FieldSpec::new("right ascension of ascending node", 2, 18, 25),
            l2,
        )?;
        let eccentricity = parse_eccentricity(&FieldSpec::new("eccentricity", 2, 27, 33), l2)?;
        let arg_perigee_deg =
            parse_f64_loose(&FieldSpec::new("argument of perigee", 2, 35, 42), l2)?;
        let mean_anomaly_deg = parse_f64_loose(&FieldSpec::new("mean anomaly", 2, 44, 51), l2)?;
        let mean_motion = parse_f64_loose(&FieldSpec::new("mean motion", 2, 53, 63), l2)?;
        let revolution_number = parse_u32(&FieldSpec::new("revolution number", 2, 64, 68), l2)?;

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

/// Parses a TLE from text, exactly like [`Tle::parse`].
///
/// Implementing [`std::str::FromStr`] means a TLE can be produced by the standard
/// `str::parse` machinery — `text.parse::<Tle>()` — and by any generic code written
/// against the trait. The rich, teaching-oriented [`TleError`] diagnostics are preserved
/// (wrapped in [`AstroError`]):
///
/// ```
/// use ephemerust::satellite::Tle;
///
/// let tle: Tle = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
///                 2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008"
///     .parse()
///     .unwrap();
/// assert_eq!(tle.catalog_number, 25544);
/// ```
impl std::str::FromStr for Tle {
    type Err = AstroError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Tle::parse(s)
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
        return Err(TleError::LineTooShort {
            line: line_no,
            found: trimmed.len(),
        });
    }
    let line = &trimmed[..69];

    let actual_number = line.chars().next().unwrap();
    if actual_number != expected_number {
        return Err(TleError::WrongLineNumber {
            expected: expected_number,
            found: actual_number,
        });
    }

    let computed = tle_checksum(line);
    let check_text = col(line, 69, 69);
    let stated = check_text
        .parse::<u32>()
        .map_err(|_| TleError::ChecksumNotDigit {
            line: line_no,
            found: check_text.to_string(),
        })?;
    if computed != stated {
        return Err(TleError::ChecksumMismatch {
            line: line_no,
            computed,
            stated,
        });
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
        FieldSpec {
            name,
            line,
            start,
            end,
        }
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
    normalized.parse::<f64>().map_err(|_| {
        spec.error(
            s,
            "expected a decimal number (an implied leading point is allowed, e.g. `-.00002218`)",
        )
    })
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
        spec.error(
            s,
            "missing exponent sign in assumed-decimal notation (e.g. `-31515-4` means -0.31515e-4)",
        )
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
    format!("0.{s}").parse::<f64>().map_err(|_| {
        spec.error(
            s,
            "expected an implied-decimal fraction of digits only (e.g. `0001413` → 0.0001413)",
        )
    })
}

/// Converts a 2-digit TLE epoch year and fractional day-of-year into a UTC datetime.
///
/// Per TLE convention, years 57–99 map to 1957–1999 and 00–56 map to 2000–2056.
fn parse_epoch(
    two_digit_year: u32,
    day_of_year: f64,
) -> std::result::Result<DateTime<Utc>, TleError> {
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

/// Which SGP4 constant set and propagate path the [`sgp4`] crate uses.
///
/// The default [`PropagationModel::Modern`] path matches the crate's recommended
/// `Constants::from_elements` / `propagate` (WGS84 geopotential, IAU sidereal time and
/// epoch). [`PropagationModel::AfspcCompatibility`] selects WGS72 + AFSPC sidereal/epoch
/// handling to reproduce legacy NORAD / Vallado reference outputs (typically within metres
/// of the default path for near-Earth orbits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PropagationModel {
    /// WGS84 + IAU (`sgp4` crate default). Recommended for general use and WGS84 geodetic output.
    #[default]
    Modern,
    /// WGS72 + AFSPC sidereal/epoch (Vallado / legacy NORAD reference compatibility).
    #[serde(rename = "afspc")]
    AfspcCompatibility,
}

/// A reusable, initialized SGP4 propagator for one element set.
///
/// SGP4 has an expensive part and a cheap part: **initialization** (parsing the element
/// set and deriving the secular/periodic coefficients in [`sgp4::Constants`]) costs far
/// more than a single **propagation step**. The one-shot free functions ([`propagate`],
/// [`subpoint`], [`look_angles`]) pay the initialization cost on *every call*, which is
/// fine for a single lookup but wasteful in loops.
///
/// `Propagator` moves that distinction into the type system — a core Rust idiom:
/// construction (`Propagator::new`) does the expensive work **once** and can fail, so it
/// returns `Result<Self>`; the borrowing methods (`&self`) are then cheap, infallible to
/// borrow, and safe to call thousands of times. Ownership makes the contract explicit:
/// as long as you hold a `Propagator`, its initialized constants are valid — there is no
/// "did I remember to initialize?" state to check at runtime.
///
/// [`ground_track`] and [`predict_passes`] use a `Propagator` internally, so they pay
/// initialization once per call rather than once per sample.
///
/// # Example
///
/// ```
/// use chrono::Duration;
/// use ephemerust::satellite::{Propagator, Tle};
///
/// let tle = Tle::parse(
///     "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
///      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
/// )
/// .unwrap();
///
/// // Initialize once...
/// let prop = Propagator::new(&tle).unwrap();
///
/// // ...then propagate many times cheaply.
/// for minutes in 0..90 {
///     let t = tle.epoch + Duration::minutes(minutes);
///     let state = prop.propagate(t).unwrap();
///     assert!(state.position_km.iter().all(|c| c.is_finite()));
/// }
/// ```
pub struct Propagator {
    elements: sgp4::Elements,
    constants: sgp4::Constants,
    model: PropagationModel,
}

impl Propagator {
    /// Initializes a propagator with [`PropagationModel::Modern`] (WGS84 + IAU).
    ///
    /// # Errors
    ///
    /// Returns [`AstroError::SatelliteError`] if the element set cannot be parsed or
    /// yields invalid epoch constants.
    pub fn new(tle: &Tle) -> Result<Self> {
        Self::with_model(tle, PropagationModel::default())
    }

    /// Initializes a propagator with an explicit [`PropagationModel`].
    ///
    /// # Errors
    ///
    /// Same as [`Propagator::new`].
    pub fn with_model(tle: &Tle, model: PropagationModel) -> Result<Self> {
        let elements =
            sgp4::Elements::from_tle(tle.name.clone(), tle.line1.as_bytes(), tle.line2.as_bytes())
                .map_err(|e| {
                    AstroError::SatelliteError(format!(
                        "could not parse element set for propagation: {e}"
                    ))
                })?;

        let constants = match model {
            PropagationModel::Modern => sgp4::Constants::from_elements(&elements).map_err(|e| {
                AstroError::SatelliteError(format!("invalid orbital elements for propagation: {e}"))
            })?,
            PropagationModel::AfspcCompatibility => {
                sgp4::Constants::from_elements_afspc_compatibility_mode(&elements).map_err(|e| {
                    AstroError::SatelliteError(format!(
                        "invalid orbital elements for AFSPC propagation: {e}"
                    ))
                })?
            }
        };

        Ok(Self {
            elements,
            constants,
            model,
        })
    }

    /// The [`PropagationModel`] this propagator was initialized with.
    pub fn model(&self) -> PropagationModel {
        self.model
    }

    /// Propagates to `time` and returns the [`TemeState`] (position km, velocity km/s).
    ///
    /// # Errors
    ///
    /// Returns [`AstroError::SatelliteError`] if the target time is too far from the
    /// element-set epoch to represent, or if the propagation diverges (for example, a
    /// decayed orbit or an eccentricity driven out of range).
    pub fn propagate(&self, time: DateTime<Utc>) -> Result<TemeState> {
        let minutes = self
            .elements
            .datetime_to_minutes_since_epoch(&time.naive_utc())
            .map_err(|e| {
                AstroError::SatelliteError(format!(
                    "target time is too far from the element-set epoch to represent: {e}"
                ))
            })?;

        let prediction = match self.model {
            PropagationModel::Modern => self.constants.propagate(minutes),
            PropagationModel::AfspcCompatibility => {
                self.constants.propagate_afspc_compatibility_mode(minutes)
            }
        }
        .map_err(|e| AstroError::SatelliteError(format!("SGP4 propagation diverged: {e}")))?;

        Ok(TemeState {
            position_km: prediction.position,
            velocity_km_s: prediction.velocity,
        })
    }

    /// Propagates to `time` and returns the **sub-satellite point** (geodetic latitude,
    /// longitude, height above the WGS84 ellipsoid).
    ///
    /// Equivalent to [`Propagator::propagate`] followed by [`teme_to_ecef`] and
    /// [`ecef_to_geodetic`].
    pub fn subpoint(&self, time: DateTime<Utc>) -> Result<Subpoint> {
        let state = self.propagate(time)?;
        let ecef = teme_to_ecef(&state, time)?;
        ecef_to_geodetic(ecef)
    }

    /// Topocentric look angles from this element set to `observer` at `time`.
    ///
    /// See [`look_angles`] for conventions (azimuth clockwise from true north, elevation
    /// above the local horizon, observer elevation as metres above the WGS84 ellipsoid).
    ///
    /// # Errors
    ///
    /// Returns [`AstroError::CalculationError`] if the slant range is below ~10 m
    /// (degenerate pointing); otherwise propagation or coordinate errors.
    pub fn look_angles(
        &self,
        time: DateTime<Utc>,
        observer: ObserverLocation,
    ) -> Result<LookAngles> {
        let state = self.propagate(time)?;
        let r_sat = teme_to_ecef(&state, time)?;
        let r_obs =
            geodetic_wgs84_to_ecef(observer.latitude, observer.longitude, observer.elevation)?;

        let rho_x = r_sat.x - r_obs.x;
        let rho_y = r_sat.y - r_obs.y;
        let rho_z = r_sat.z - r_obs.z;

        let v_sat = teme_velocity_to_ecef_m_s(&state, time, &r_sat)?;
        let v_obs = omega_cross_r_ecef(&r_obs);
        let rdx = v_sat[0] - v_obs[0];
        let rdy = v_sat[1] - v_obs[1];
        let rdz = v_sat[2] - v_obs[2];

        let (e, n, u) =
            ecef_delta_to_enu(observer.latitude, observer.longitude, rho_x, rho_y, rho_z);
        let range_m = (e * e + n * n + u * u).sqrt();
        const MIN_RANGE_M: f64 = 10.0;
        if !range_m.is_finite() || range_m < MIN_RANGE_M {
            return Err(AstroError::CalculationError(format!(
                "observer-to-satellite range ({range_m:.3} m) is too small for stable azimuth/elevation"
            )));
        }

        let horizontal = (e * e + n * n).sqrt();
        let elevation_deg = u.atan2(horizontal).to_degrees();
        let mut azimuth_deg = e.atan2(n).to_degrees();
        if azimuth_deg < 0.0 {
            azimuth_deg += 360.0;
        }

        let range_rate_m_s = (rho_x * rdx + rho_y * rdy + rho_z * rdz) / range_m;

        Ok(LookAngles {
            azimuth_deg,
            elevation_deg,
            range_km: range_m / 1000.0,
            range_rate_km_s: range_rate_m_s / 1000.0,
        })
    }
}

/// Propagates a TLE to the given UTC time and returns the [`TemeState`].
///
/// Uses [`PropagationModel::Modern`] (WGS84 + IAU). For legacy reference reproduction see
/// [`propagate_with_model`].
///
/// # Errors
///
/// Returns [`AstroError::SatelliteError`] if the element set cannot be parsed or yields
/// invalid epoch constants, if the target time is too far from the epoch to represent
/// (nanosecond overflow), or if the propagation itself diverges (for example, a decayed
/// orbit or an eccentricity driven out of range).
///
/// # Example
///
/// Propagate the ISS to its own element-set epoch and confirm a plausible low-Earth-orbit
/// radius (~6,800 km from the geocenter):
///
/// ```
/// use ephemerust::satellite::{Tle, propagate};
///
/// let tle = Tle::parse(
///     "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
///      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
/// )
/// .unwrap();
///
/// let state = propagate(&tle, tle.epoch).unwrap();
/// let r = state.position_km;
/// let radius = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
/// assert!((6_600.0..=7_100.0).contains(&radius));
/// ```
pub fn propagate(tle: &Tle, time: DateTime<Utc>) -> Result<TemeState> {
    propagate_with_model(tle, time, PropagationModel::default())
}

/// Propagates a TLE with an explicit [`PropagationModel`].
///
/// This is a one-shot convenience: it initializes a [`Propagator`] (parsing the element
/// set and deriving the SGP4 constants) and performs a single propagation. When
/// propagating the **same element set at many times**, construct a [`Propagator`] once
/// and reuse it — initialization dominates the cost of an individual step.
pub fn propagate_with_model(
    tle: &Tle,
    time: DateTime<Utc>,
    model: PropagationModel,
) -> Result<TemeState> {
    Propagator::with_model(tle, model)?.propagate(time)
}

/// Converts a TEME position to **ECEF** coordinates at the given UTC time.
///
/// The position part of [`TemeState`] is converted from kilometres to metres and rotated
/// from TEME to ECEF using the same Z-axis Greenwich sidereal-time model as
/// [`crate::coordinates::eci_to_ecef`], treating TEME like ECI for this step (the usual
/// simplified SGP4 pipeline). To transform velocity to ECEF, apply the same rotation to
/// `[vx, vy, vz]` expressed in metres per second.
///
/// # Errors
///
/// Delegates to [`crate::coordinates::eci_to_ecef`] (invalid coordinates or GMST).
pub fn teme_to_ecef(state: &TemeState, time: DateTime<Utc>) -> Result<Ecef> {
    let jd = crate::time::julian_date(time);
    let gmst = crate::time::greenwich_mean_sidereal_time(jd);
    let eci = Eci {
        x: state.position_km[0] * 1000.0,
        y: state.position_km[1] * 1000.0,
        z: state.position_km[2] * 1000.0,
    };
    eci_to_ecef(eci, gmst)
}

/// Converts **WGS84** [`Ecef`] coordinates (metres) to a [`Subpoint`] with altitude in kilometres.
///
/// This is a thin wrapper around [`crate::coordinates::ecef_to_geodetic_wgs84`] so satellite
/// callers work directly in [`Subpoint`] without juggling unit conversions.
pub fn ecef_to_geodetic(ecef: Ecef) -> Result<Subpoint> {
    let g = ecef_to_geodetic_wgs84(ecef)?;
    Ok(Subpoint {
        latitude_deg: g.latitude_deg,
        longitude_deg: g.longitude_deg,
        altitude_km: g.height_m / 1000.0,
    })
}

/// Propagates the element set to `time` and returns the **sub-satellite point** (geodetic
/// latitude, longitude, and height above the WGS84 ellipsoid).
///
/// This is equivalent to [`propagate`] followed by [`teme_to_ecef`] and [`ecef_to_geodetic`].
pub fn subpoint(tle: &Tle, time: DateTime<Utc>) -> Result<Subpoint> {
    subpoint_with_model(tle, time, PropagationModel::default())
}

/// Like [`subpoint`] with an explicit [`PropagationModel`].
///
/// One-shot convenience over [`Propagator::subpoint`]; for repeated sampling of the same
/// element set, build one [`Propagator`] and reuse it.
pub fn subpoint_with_model(
    tle: &Tle,
    time: DateTime<Utc>,
    model: PropagationModel,
) -> Result<Subpoint> {
    Propagator::with_model(tle, model)?.subpoint(time)
}

/// Samples the satellite **ground track** as WGS84 sub-satellite points from `window_start`
/// (inclusive) through `window_end` (**exclusive**), every `step`.
///
/// Each sample is [`subpoint`] at `window_start`, `window_start + step`, … while strictly
/// before `window_end`. If `window_end <= window_start`, returns an empty vector. `step` must
/// be strictly positive.
///
/// # Errors
///
/// Propagation or geodetic conversion errors surface from [`subpoint`].
pub fn ground_track(
    tle: &Tle,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    step: Duration,
) -> Result<Vec<GroundTrackSample>> {
    ground_track_with_model(
        tle,
        window_start,
        window_end,
        step,
        PropagationModel::default(),
    )
}

/// Like [`ground_track`] with an explicit [`PropagationModel`].
pub fn ground_track_with_model(
    tle: &Tle,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    step: Duration,
    model: PropagationModel,
) -> Result<Vec<GroundTrackSample>> {
    if step <= Duration::zero() {
        return Err(AstroError::SatelliteError(
            "ground_track step must be strictly positive".into(),
        ));
    }
    if window_end <= window_start {
        return Ok(Vec::new());
    }

    let span_ns = (window_end - window_start)
        .num_nanoseconds()
        .unwrap_or(i64::MAX)
        .max(0);
    let step_ns = step.num_nanoseconds().unwrap_or(i64::MAX).max(1);
    let est = ((span_ns / step_ns) as usize).min(500_000);
    let mut out = Vec::with_capacity(est);

    // Initialize the SGP4 constants once; each sample is then a cheap propagation step.
    let prop = Propagator::with_model(tle, model)?;

    let mut t = window_start;
    while t < window_end {
        let s = prop.subpoint(t)?;
        out.push(GroundTrackSample {
            time: t,
            subpoint: s,
        });
        t += step;
    }
    Ok(out)
}

/// Serializes ground-track samples as **CSV** (RFC 4180–style: header row, comma-separated).
///
/// The `time_utc` column uses RFC 3339 / ISO-8601 with fractional seconds. Angles are in
/// degrees; altitude is ellipsoidal height in kilometres (same as [`Subpoint`]).
pub fn ground_track_to_csv(samples: &[GroundTrackSample]) -> String {
    use std::fmt::Write;
    let mut buf = String::new();
    buf.push_str("time_utc,latitude_deg,longitude_deg,altitude_km\n");
    for row in samples {
        let _ = writeln!(
            &mut buf,
            "{},{:.9},{:.9},{:.6}",
            row.time
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            row.subpoint.latitude_deg,
            row.subpoint.longitude_deg,
            row.subpoint.altitude_km
        );
    }
    buf
}

/// Serializes ground-track samples as a **JSON** array (pretty-printed).
pub fn ground_track_to_json(samples: &[GroundTrackSample]) -> Result<String> {
    serde_json::to_string_pretty(samples)
        .map_err(|e| AstroError::SatelliteError(format!("JSON serialization failed: {e}")))
}

/// Earth rotation rate about ECEF +Z (rad/s), conventional sidereal value used with GMST.
const OMEGA_EARTH_RAD_S: f64 = 7.2921150e-5;

/// ω × **r** for ω aligned with +Z (ECEF), in metres per second when **r** is in metres.
fn omega_cross_r_ecef(r: &Ecef) -> [f64; 3] {
    let w = OMEGA_EARTH_RAD_S;
    [-w * r.y, w * r.x, 0.0]
}

/// TEME velocity to ECEF velocity (m/s), including the transport term **ω** × **r** for the
/// rotating Earth-fixed frame (Vallado-style bridge used with SGP4/TEME pipelines).
fn teme_velocity_to_ecef_m_s(
    state: &TemeState,
    time: DateTime<Utc>,
    r_ecef: &Ecef,
) -> Result<[f64; 3]> {
    let jd = crate::time::julian_date(time);
    let gmst = crate::time::greenwich_mean_sidereal_time(jd);
    let v_eci = Eci {
        x: state.velocity_km_s[0] * 1000.0,
        y: state.velocity_km_s[1] * 1000.0,
        z: state.velocity_km_s[2] * 1000.0,
    };
    let v_rot = eci_to_ecef(v_eci, gmst)?;
    let wxr = omega_cross_r_ecef(r_ecef);
    Ok([v_rot.x + wxr[0], v_rot.y + wxr[1], v_rot.z + wxr[2]])
}

/// Rotates a geocentric ECEF difference vector into **ENU** (east, north, up) at the observer.
///
/// The rotation is the standard WGS84 local tangent plane: east along the parallel, north
/// along the meridian, up along the ellipsoid outward normal. This is equivalent to the
/// usual **NED**/**SEZ** family up to axis renaming (ENU up = zenith of SEZ).
fn ecef_delta_to_enu(
    latitude_deg: f64,
    longitude_deg: f64,
    dx: f64,
    dy: f64,
    dz: f64,
) -> (f64, f64, f64) {
    let φ = latitude_deg.to_radians();
    let λ = longitude_deg.to_radians();
    let sin_φ = φ.sin();
    let cos_φ = φ.cos();
    let sin_λ = λ.sin();
    let cos_λ = λ.cos();
    let east = -sin_λ * dx + cos_λ * dy;
    let north = -sin_φ * cos_λ * dx - sin_φ * sin_λ * dy + cos_φ * dz;
    let up = cos_φ * cos_λ * dx + cos_φ * sin_λ * dy + sin_φ * dz;
    (east, north, up)
}

/// Topocentric look angles from a TLE, time, and observer location.
///
/// Pipeline: [`propagate`] → TEME→ECEF position and velocity (including **ω** × **r** for
/// Earth rotation), minus the observer's ECEF position and **ω** × **r_obs** velocity, then
/// a local **ENU** (east–north–up) rotation. Azimuth is measured **clockwise from true north**
/// (0° = north, 90° = east), matching [`crate::coordinates::AltAz`] conventions. Elevation is
/// above the local astronomical horizon (the plane perpendicular to ellipsoid up).
///
/// `observer.elevation` is taken as **metres above the WGS84 ellipsoid** for the satellite
/// pipeline (the same numeric field is documented as sea level elsewhere in the crate; for
/// slant range it is treated as ellipsoidal height here).
///
/// # Errors
///
/// Returns [`AstroError::CalculationError`] if the slant range is below ~10 m (degenerate
/// pointing). Other failures are propagation or coordinate errors from the underlying calls.
///
/// # Example
///
/// ```
/// use ephemerust::celestial::ObserverLocation;
/// use ephemerust::satellite::{Tle, look_angles};
///
/// let tle = Tle::parse(
///     "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
///      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
/// )
/// .unwrap();
/// let obs = ObserverLocation {
///     latitude: 47.9088,
///     longitude: -122.2503,
///     elevation: 0.0,
/// };
/// let la = look_angles(&tle, tle.epoch, obs).unwrap();
/// assert!(la.range_km > 50.0 && la.range_km < 20_000.0);
/// assert!((0.0..360.0).contains(&la.azimuth_deg));
/// ```
pub fn look_angles(
    tle: &Tle,
    time: DateTime<Utc>,
    observer: ObserverLocation,
) -> Result<LookAngles> {
    look_angles_with_model(tle, time, observer, PropagationModel::default())
}

/// Like [`look_angles`] with an explicit [`PropagationModel`].
///
/// One-shot convenience over [`Propagator::look_angles`]; for repeated lookups against the
/// same element set (tracking loops, pass searches), build one [`Propagator`] and reuse it.
pub fn look_angles_with_model(
    tle: &Tle,
    time: DateTime<Utc>,
    observer: ObserverLocation,
    model: PropagationModel,
) -> Result<LookAngles> {
    Propagator::with_model(tle, model)?.look_angles(time, observer)
}

/// Seconds per solar day (used only to convert mean motion from rev/day to an orbital period).
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Coarse sampling step for pass finding: a fraction of the orbital period, clamped.
fn coarse_step_for_mean_motion(mean_motion_rev_per_day: f64) -> Duration {
    if mean_motion_rev_per_day <= 0.0 || !mean_motion_rev_per_day.is_finite() {
        return Duration::seconds(60);
    }
    let period_s = SECONDS_PER_DAY / mean_motion_rev_per_day;
    let step_s = (period_s / 48.0).round().clamp(15.0, 600.0);
    Duration::seconds(step_s as i64)
}

fn elevation_deg(prop: &Propagator, t: DateTime<Utc>, observer: ObserverLocation) -> Result<f64> {
    Ok(prop.look_angles(t, observer)?.elevation_deg)
}

/// Heuristic: near-geosynchronous element sets (≈1 rev/day, equatorial) use a simplified
/// all-or-nothing visibility over the search window.
fn is_geo_heuristic(tle: &Tle) -> bool {
    tle.mean_motion > 0.98
        && tle.mean_motion < 1.04
        && tle.inclination_deg.abs() < 3.0
        && tle.eccentricity < 0.02
}

fn predict_passes_geo(
    prop: &Propagator,
    observer: ObserverLocation,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    min_elevation_deg: f64,
) -> Result<Vec<Pass>> {
    if window_end <= window_start {
        return Ok(Vec::new());
    }
    let mid = window_start + (window_end - window_start) / 2;
    let el_start = elevation_deg(prop, window_start, observer)?;
    let el_mid = elevation_deg(prop, mid, observer)?;
    let el_end = elevation_deg(prop, window_end - Duration::milliseconds(1), observer)?;
    let above =
        el_start >= min_elevation_deg && el_mid >= min_elevation_deg && el_end >= min_elevation_deg;
    if !above {
        return Ok(Vec::new());
    }
    let la_a = prop.look_angles(window_start, observer)?;
    let la_b = prop.look_angles(window_end - Duration::milliseconds(1), observer)?;
    Ok(vec![Pass {
        aos: window_start,
        culmination: mid,
        los: window_end,
        max_elevation_deg: el_start.max(el_mid).max(el_end),
        aos_azimuth_deg: la_a.azimuth_deg,
        los_azimuth_deg: la_b.azimuth_deg,
    }])
}

/// Refines a rising threshold crossing: `el(lo) < mask`, `el(hi) >= mask`.
fn bisect_elevation_rising(
    prop: &Propagator,
    observer: ObserverLocation,
    mask: f64,
    mut lo: DateTime<Utc>,
    mut hi: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    for _ in 0..56 {
        if hi.signed_duration_since(lo) <= Duration::milliseconds(1) {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let el = elevation_deg(prop, mid, observer)?;
        if el >= mask {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

/// Refines a falling threshold crossing: `el(lo) >= mask`, `el(hi) < mask`.
fn bisect_elevation_falling(
    prop: &Propagator,
    observer: ObserverLocation,
    mask: f64,
    mut lo: DateTime<Utc>,
    mut hi: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    for _ in 0..56 {
        if hi.signed_duration_since(lo) <= Duration::milliseconds(1) {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let el = elevation_deg(prop, mid, observer)?;
        if el < mask {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

/// Finds culmination time and maximum elevation between AOS and LOS (ternary search on time).
fn culmination_time_and_max_el(
    prop: &Propagator,
    observer: ObserverLocation,
    aos: DateTime<Utc>,
    los: DateTime<Utc>,
) -> Result<(DateTime<Utc>, f64)> {
    let mut lo = aos;
    let mut hi = los;
    if hi <= lo {
        let el = elevation_deg(prop, lo, observer)?;
        return Ok((lo, el));
    }
    for _ in 0..48 {
        if hi.signed_duration_since(lo) <= Duration::milliseconds(2) {
            break;
        }
        let third = (hi - lo) / 3;
        let m1 = lo + third;
        let m2 = hi - third;
        let e1 = elevation_deg(prop, m1, observer)?;
        let e2 = elevation_deg(prop, m2, observer)?;
        if e1 > e2 {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let t_peak = lo + (hi - lo) / 2;
    let max_el = elevation_deg(prop, t_peak, observer)?;
    Ok((t_peak, max_el))
}

/// Predicts visible passes of a satellite over an observer in a UTC time window.
///
/// The search samples elevation at a coarse step derived from the element set’s mean motion,
/// detects crossings of `min_elevation_deg`, then refines AOS and LOS with bisection and
/// locates culmination by ternary search on the elevation profile. Near-geosynchronous
/// sets (≈1 rev/day, low inclination) use a simplified model: if the satellite is above the
/// mask at the start, middle, and end of the window, a single pass spanning the full window
/// is returned; otherwise none.
///
/// `window_end` is **exclusive** (samples use `t < window_end`).
///
/// `observer.elevation` is interpreted like [`look_angles`]: metres above the WGS84 ellipsoid.
pub fn predict_passes(
    tle: &Tle,
    observer: ObserverLocation,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    min_elevation_deg: f64,
) -> Result<Vec<Pass>> {
    predict_passes_with_model(
        tle,
        observer,
        window_start,
        window_end,
        min_elevation_deg,
        PropagationModel::default(),
    )
}

/// Like [`predict_passes`] with an explicit [`PropagationModel`].
pub fn predict_passes_with_model(
    tle: &Tle,
    observer: ObserverLocation,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    min_elevation_deg: f64,
    model: PropagationModel,
) -> Result<Vec<Pass>> {
    if window_end <= window_start {
        return Ok(Vec::new());
    }

    // Initialize the SGP4 constants once for the entire search: the coarse scan,
    // bisection refinements, and culmination search below share this propagator.
    let prop = Propagator::with_model(tle, model)?;

    if is_geo_heuristic(tle) {
        return predict_passes_geo(&prop, observer, window_start, window_end, min_elevation_deg);
    }

    let dt = coarse_step_for_mean_motion(tle.mean_motion);
    let mut passes = Vec::new();

    let mut prev_t = window_start;
    let mut prev_el = elevation_deg(&prop, prev_t, observer)?;
    let mut pending_aos: Option<DateTime<Utc>> = if prev_el >= min_elevation_deg {
        Some(window_start)
    } else {
        None
    };

    let mut t = window_start + dt;
    while t < window_end {
        let el = elevation_deg(&prop, t, observer)?;

        if pending_aos.is_none() && prev_el < min_elevation_deg && el >= min_elevation_deg {
            let aos = bisect_elevation_rising(&prop, observer, min_elevation_deg, prev_t, t)?;
            pending_aos = Some(aos);
        } else if pending_aos.is_some() && prev_el >= min_elevation_deg && el < min_elevation_deg {
            let aos = pending_aos.take().expect("aos");
            let los = bisect_elevation_falling(&prop, observer, min_elevation_deg, prev_t, t)?;
            if los > aos {
                let (culm, max_el) = culmination_time_and_max_el(&prop, observer, aos, los)?;
                let la_aos = prop.look_angles(aos, observer)?;
                let la_los = prop.look_angles(los, observer)?;
                passes.push(Pass {
                    aos,
                    culmination: culm,
                    los,
                    max_elevation_deg: max_el,
                    aos_azimuth_deg: la_aos.azimuth_deg,
                    los_azimuth_deg: la_los.azimuth_deg,
                });
            }
        }

        prev_t = t;
        prev_el = el;
        t += dt;
    }

    // Final segment up to exclusive window end.
    let last_t = (window_end - Duration::milliseconds(1)).max(window_start);
    if last_t > prev_t {
        let el = elevation_deg(&prop, last_t, observer)?;
        let opened_in_tail =
            if pending_aos.is_none() && prev_el < min_elevation_deg && el >= min_elevation_deg {
                pending_aos = Some(bisect_elevation_rising(
                    &prop,
                    observer,
                    min_elevation_deg,
                    prev_t,
                    last_t,
                )?);
                true
            } else {
                false
            };

        if let Some(aos) = pending_aos {
            if !opened_in_tail && prev_el >= min_elevation_deg && el < min_elevation_deg {
                let los =
                    bisect_elevation_falling(&prop, observer, min_elevation_deg, prev_t, last_t)?;
                if los > aos {
                    let (culm, max_el) = culmination_time_and_max_el(&prop, observer, aos, los)?;
                    let la_aos = prop.look_angles(aos, observer)?;
                    let la_los = prop.look_angles(los, observer)?;
                    passes.push(Pass {
                        aos,
                        culmination: culm,
                        los,
                        max_elevation_deg: max_el,
                        aos_azimuth_deg: la_aos.azimuth_deg,
                        los_azimuth_deg: la_los.azimuth_deg,
                    });
                }
            } else if el >= min_elevation_deg {
                let los = window_end;
                let los_sample = last_t;
                let (culm, max_el) = culmination_time_and_max_el(&prop, observer, aos, los_sample)?;
                let la_aos = prop.look_angles(aos, observer)?;
                let la_los = prop.look_angles(los_sample, observer)?;
                passes.push(Pass {
                    aos,
                    culmination: culm,
                    los,
                    max_elevation_deg: max_el,
                    aos_azimuth_deg: la_aos.azimuth_deg,
                    los_azimuth_deg: la_los.azimuth_deg,
                });
            }
        }
    }

    Ok(passes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celestial::ObserverLocation;
    use crate::coordinates::ecef_to_eci;
    use chrono::{Datelike, Duration, TimeZone, Timelike};

    // Canonical ISS (ZARYA) element set, reused as a fixed reference across the suite.
    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_LINE1: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
    const ISS_LINE2: &str = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

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
        let tle =
            Tle::parse(&format!("{ISS_LINE1}\n{ISS_LINE2}")).expect("2-line set should parse");
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
            matches!(
                err,
                AstroError::Tle(TleError::ChecksumMismatch { line: 1, .. })
            ),
            "unexpected error: {err:?}"
        );
        // The message must teach the rule, not merely report a mismatch.
        let msg = err.to_string();
        assert!(
            msg.contains("modulo-10"),
            "missing the checksum rule: {msg}"
        );
        assert!(err.hint().is_some(), "checksum errors should carry a hint");
    }

    #[test]
    fn rejects_wrong_line_count() {
        let err = Tle::parse(ISS_LINE1).expect_err("single line must be rejected");
        assert!(matches!(
            err,
            AstroError::Tle(TleError::WrongLineCount { found: 1 })
        ));
        assert!(err.to_string().contains("2-line or 3-line"));
    }

    #[test]
    fn rejects_short_line() {
        let err = Tle::from_lines(None, "1 25544U", ISS_LINE2)
            .expect_err("truncated line must be rejected");
        assert!(matches!(
            err,
            AstroError::Tle(TleError::LineTooShort { line: 1, .. })
        ));
        // The message must state both the requirement and the underlying fixed-column rule.
        let msg = err.to_string();
        assert!(
            msg.contains("69 are required"),
            "missing the column requirement: {msg}"
        );
        assert!(
            msg.contains("fixed-column"),
            "missing the fixed-column rule: {msg}"
        );
    }

    #[test]
    fn rejects_wrong_line_number() {
        // Pass line 2 where line 1 is expected.
        let err = Tle::from_lines(None, ISS_LINE2, ISS_LINE2)
            .expect_err("misordered lines must be rejected");
        assert!(matches!(
            err,
            AstroError::Tle(TleError::WrongLineNumber {
                expected: '1',
                found: '2'
            })
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
                delta
                    .num_microseconds()
                    .map(|us| us.abs() < 1_000)
                    .unwrap_or(false),
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
    fn propagate_afspc_wrapper_matches_reference_at_epoch() {
        let tle = Tle::from_lines(None, SAT5_LINE1, SAT5_LINE2).unwrap();
        let state = propagate_with_model(&tle, tle.epoch, PropagationModel::AfspcCompatibility)
            .expect("AFSPC propagation at epoch succeeds");
        let expected_r = [7022.46529266, -1400.08296755, 0.03995155];
        for (i, expected) in expected_r.iter().enumerate() {
            assert!(
                (state.position_km[i] - expected).abs() < 1e-3,
                "AFSPC position[{i}]: {} vs reference {expected}",
                state.position_km[i]
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
        for (axis, (actual, expected)) in prediction
            .position
            .iter()
            .zip(expected_r.iter())
            .enumerate()
        {
            assert!(
                (actual - expected).abs() < 1e-3,
                "AFSPC position[{axis}]: {actual} vs reference {expected}"
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
        assert!(
            matches!(err, AstroError::SatelliteError(_)),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn propagation_diverges_at_epoch_for_degenerate_element_set() {
        // From the `sgp4` crate's official regression set: perturbed eccentricity diverges at t = 0.
        let tle = Tle::from_lines(None, DIVERGE_AT_EPOCH_L1, DIVERGE_AT_EPOCH_L2).unwrap();
        let err = propagate(&tle, tle.epoch).expect_err("epoch propagation must diverge");
        assert!(
            matches!(err, AstroError::SatelliteError(_)),
            "unexpected error: {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("diverged"),
            "expected SGP4 divergence message, got: {msg}"
        );
    }

    #[test]
    fn propagation_diverges_after_drag_decay_fixture() {
        // Same regression corpus: negative semi-latus rectum 25 minutes after epoch.
        let tle = Tle::from_lines(None, DIVERGE_25MIN_L1, DIVERGE_25MIN_L2).unwrap();
        let t = tle.epoch + Duration::minutes(25);
        let err = propagate(&tle, t).expect_err("propagation must diverge after decay");
        assert!(
            matches!(err, AstroError::SatelliteError(_)),
            "unexpected error: {err:?}"
        );
        assert!(err.to_string().contains("diverged"));
        // Epoch itself is still valid for this element set.
        propagate(&tle, tle.epoch).expect("propagation at epoch should succeed for this TLE");
    }

    #[test]
    fn teme_to_ecef_inverts_ecef_to_eci_bridge() {
        let time = Utc.with_ymd_and_hms(2000, 1, 1, 12, 0, 0).unwrap();
        let jd = crate::time::julian_date(time);
        let gmst = crate::time::greenwich_mean_sidereal_time(jd);

        let ecef = crate::coordinates::Ecef {
            x: 1_200_000.0,
            y: -4_500_000.0,
            z: 4_800_000.0,
        };
        let eci = ecef_to_eci(ecef, gmst).unwrap();
        let state = TemeState {
            position_km: [eci.x / 1000.0, eci.y / 1000.0, eci.z / 1000.0],
            velocity_km_s: [0.0; 3],
        };
        let back = teme_to_ecef(&state, time).unwrap();
        const MM: f64 = 0.001;
        assert!((back.x - ecef.x).abs() < MM);
        assert!((back.y - ecef.y).abs() < MM);
        assert!((back.z - ecef.z).abs() < MM);
    }

    #[test]
    fn subpoint_iss_epoch_is_plausible_leo() {
        let tle = Tle::parse(&iss_3line()).expect("ISS TLE parses");
        let p = subpoint(&tle, tle.epoch).expect("subpoint at epoch succeeds");
        assert!(
            p.altitude_km > 200.0 && p.altitude_km < 500.0,
            "altitude {} km",
            p.altitude_km
        );
        assert!(p.latitude_deg.abs() < 55.0, "latitude {}°", p.latitude_deg);
        assert!(p.longitude_deg.abs() <= 180.0);
    }

    #[test]
    fn subpoint_iss_crosses_equator_within_one_orbit() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        let end = start + orbit_period(&tle);
        let min_abs_lat = min_abs_subpoint_latitude_deg(&tle, start, end, Duration::seconds(30));
        assert!(
            min_abs_lat < 1.0,
            "ISS should cross the equator within one orbit; min |lat| = {min_abs_lat}°"
        );
    }

    #[test]
    fn subpoint_high_inclination_reaches_polar_latitudes() {
        // Near-polar element set (i ≈ 96.5°) from the `sgp4` regression corpus.
        let tle = Tle::from_lines(None, DIVERGE_25MIN_L1, DIVERGE_25MIN_L2).unwrap();
        let start = tle.epoch;
        let end = start + orbit_period(&tle);
        let max_abs_lat = max_abs_subpoint_latitude_deg(&tle, start, end, Duration::seconds(30));
        assert!(
            max_abs_lat > 85.0,
            "96° inclination should reach polar latitudes; max |lat| = {max_abs_lat}°"
        );
    }

    #[test]
    fn subpoint_longitude_stays_in_range_and_crosses_antimeridian() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        // Two orbits: ISS ground track crosses the ±180° meridian routinely.
        let end = start + orbit_period(&tle) * 2;
        assert!(
            subpoint_track_crosses_antimeridian(&tle, start, end, Duration::seconds(45)),
            "expected a ±180° longitude wrap on the ISS ground track within two orbits"
        );
    }

    #[test]
    fn ecef_to_geodetic_matches_wgs84_engine() {
        let ecef = crate::coordinates::Ecef {
            x: 652_954.0,
            y: 4_774_619.0,
            z: 4_202_104.0,
        };
        let g = crate::coordinates::ecef_to_geodetic_wgs84(ecef).unwrap();
        let sp = ecef_to_geodetic(ecef).unwrap();
        assert!((sp.latitude_deg - g.latitude_deg).abs() < 1e-12);
        assert!((sp.longitude_deg - g.longitude_deg).abs() < 1e-12);
        assert!((sp.altitude_km * 1000.0 - g.height_m).abs() < 1e-9);
    }

    #[test]
    fn look_angles_near_zenith_when_observer_at_subpoint() {
        let tle = Tle::parse(&iss_3line()).expect("ISS TLE parses");
        let t = tle.epoch;
        let sub = subpoint(&tle, t).expect("subpoint");
        let obs = ObserverLocation {
            latitude: sub.latitude_deg,
            longitude: sub.longitude_deg,
            elevation: 0.0,
        };
        let la = look_angles(&tle, t, obs).expect("look angles at subpoint");
        assert!(
            la.elevation_deg > 85.0,
            "expected near-zenith; got el={}° az={}° range={} km",
            la.elevation_deg,
            la.azimuth_deg,
            la.range_km
        );
    }

    #[test]
    fn look_angles_range_rate_matches_central_difference() {
        let tle = Tle::parse(&iss_3line()).expect("ISS TLE parses");
        let obs = ObserverLocation {
            latitude: 47.9088,
            longitude: -122.2503,
            elevation: 0.0,
        };
        let t0 = tle.epoch;
        let la0 = look_angles(&tle, t0, obs).expect("look angles at epoch");
        let dt_s = 1.0_f64;
        let dt = chrono::Duration::milliseconds((dt_s * 1000.0) as i64);
        let r_plus = look_angles(&tle, t0 + dt, obs).expect("look+").range_km;
        let r_minus = look_angles(&tle, t0 - dt, obs).expect("look-").range_km;
        let num_km_s = (r_plus - r_minus) / (2.0 * dt_s);
        assert!(
            (num_km_s - la0.range_rate_km_s).abs() < 0.25,
            "range_rate {} vs numerical {} km/s (1 s central difference)",
            la0.range_rate_km_s,
            num_km_s
        );
    }

    #[test]
    fn look_angles_approaching_has_negative_range_rate() {
        let tle = Tle::parse(&iss_3line()).expect("ISS TLE parses");
        let obs = ObserverLocation {
            latitude: 47.9088,
            longitude: -122.2503,
            elevation: 0.0,
        };
        let t0 = tle.epoch;
        let mut found = false;
        for i in 1..3600 {
            let t1 = t0 + chrono::Duration::seconds(i);
            let t2 = t0 + chrono::Duration::seconds(i + 1);
            let r1 = look_angles(&tle, t1, obs).unwrap().range_km;
            let r2 = look_angles(&tle, t2, obs).unwrap().range_km;
            if r2 < r1 - 0.002 {
                let la = look_angles(&tle, t1, obs).unwrap();
                assert!(
                    la.range_rate_km_s < 0.0,
                    "when range drops over 1 s, range_rate should be negative; got {} (r1={} r2={})",
                    la.range_rate_km_s,
                    r1,
                    r2
                );
                found = true;
                break;
            }
        }
        assert!(found, "no approaching 1 s window found within 1 h of epoch");
    }

    #[test]
    fn ground_track_rejects_nonpositive_step() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        let end = start + Duration::hours(1);
        let err = ground_track(&tle, start, end, Duration::zero()).unwrap_err();
        assert!(matches!(err, AstroError::SatelliteError(_)));
    }

    #[test]
    fn ground_track_empty_for_reversed_window() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        let v = ground_track(&tle, start, start, Duration::seconds(60)).unwrap();
        assert!(v.is_empty());
        let v2 = ground_track(
            &tle,
            start,
            start - Duration::hours(1),
            Duration::seconds(60),
        )
        .unwrap();
        assert!(v2.is_empty());
    }

    #[test]
    fn ground_track_sample_count_matches_span() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        let end = start + Duration::hours(1);
        let step = Duration::minutes(10);
        let v = ground_track(&tle, start, end, step).unwrap();
        // Inclusive start, exclusive end: 0,10,…,50 minutes → 6 samples
        assert_eq!(v.len(), 6);
        assert_eq!(v[0].time, start);
        assert_eq!(v.last().unwrap().time, start + Duration::minutes(50));
    }

    // Vallado / sgp4 crate regression TLEs that must fail propagation (not merely parse).
    const DIVERGE_AT_EPOCH_L1: &str =
        "1 33334U 78066F   06174.85818871  .00000620  00000-0  10000-3 0  6806";
    const DIVERGE_AT_EPOCH_L2: &str =
        "2 33334  68.4714 236.1303 5602877 123.7484 302.5767  0.00001000 67521";
    const DIVERGE_25MIN_L1: &str =
        "1 33333U 05037B   05333.02012661  .25992681  00000-0  24476-3 0  1532";
    const DIVERGE_25MIN_L2: &str =
        "2 33333  96.4736 157.9986 9950000 244.0492 110.6523  4.00004038 10700";

    fn orbit_period(tle: &Tle) -> Duration {
        let period_s = 86400.0 / tle.mean_motion;
        Duration::milliseconds((period_s * 1000.0).round() as i64)
    }

    /// Minimum |latitude| reached by [`subpoint`] samples in `[start, end)` every `step`.
    fn min_abs_subpoint_latitude_deg(
        tle: &Tle,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: Duration,
    ) -> f64 {
        let mut min = f64::INFINITY;
        let mut t = start;
        while t < end {
            if let Ok(p) = subpoint(tle, t) {
                min = min.min(p.latitude_deg.abs());
            }
            t += step;
        }
        min
    }

    /// Maximum |latitude| reached by [`subpoint`] samples in `[start, end)` every `step`.
    fn max_abs_subpoint_latitude_deg(
        tle: &Tle,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: Duration,
    ) -> f64 {
        let mut max = 0.0_f64;
        let mut t = start;
        while t < end {
            if let Ok(p) = subpoint(tle, t) {
                max = max.max(p.latitude_deg.abs());
            }
            t += step;
        }
        max
    }

    /// True if consecutive samples show a raw longitude jump > 300° (±180° wrap) with both
    /// endpoints still in [-180°, 180°].
    fn subpoint_track_crosses_antimeridian(
        tle: &Tle,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        step: Duration,
    ) -> bool {
        let mut prev: Option<Subpoint> = None;
        let mut t = start;
        while t < end {
            if let Ok(p) = subpoint(tle, t) {
                assert!(
                    p.longitude_deg >= -180.0 && p.longitude_deg <= 180.0,
                    "longitude out of range: {}",
                    p.longitude_deg
                );
                if let Some(p0) = prev {
                    let raw_jump = (p.longitude_deg - p0.longitude_deg).abs();
                    if raw_jump > 300.0 {
                        return true;
                    }
                }
                prev = Some(p);
            }
            t += step;
        }
        false
    }

    fn lon_unwrapped_delta_deg(prev_lon: f64, next_lon: f64) -> f64 {
        let mut d = next_lon - prev_lon;
        while d > 180.0 {
            d -= 360.0;
        }
        while d < -180.0 {
            d += 360.0;
        }
        d
    }

    #[test]
    fn ground_track_longitude_shift_per_orbit_matches_earth_rotation() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let t0 = tle.epoch;
        let n = tle.mean_motion;
        assert!(n > 0.0);
        let period_sec = 86400.0 / n;
        let dt_ms = (period_sec * 1000.0).round() as i64;
        let p0 = subpoint(&tle, t0).unwrap();
        let p1 = subpoint(&tle, t0 + Duration::milliseconds(dt_ms)).unwrap();
        let dlon = lon_unwrapped_delta_deg(p0.longitude_deg, p1.longitude_deg);
        let expected = -360.0 * (period_sec / 86164.0905);
        assert!(
            (dlon - expected).abs() < 8.0,
            "Δlon {dlon}° vs expected ~{expected}° (one orbit, ISS TLE)"
        );
    }

    #[test]
    fn ground_track_continuity_small_lon_steps_for_60s_sampling() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        let end = start + Duration::minutes(45);
        let step = Duration::seconds(60);
        let v = ground_track(&tle, start, end, step).unwrap();
        assert!(v.len() >= 2);
        let mut max_dlon = 0.0_f64;
        for w in v.windows(2) {
            let d =
                lon_unwrapped_delta_deg(w[0].subpoint.longitude_deg, w[1].subpoint.longitude_deg)
                    .abs();
            max_dlon = max_dlon.max(d);
        }
        assert!(
            max_dlon < 22.0,
            "60 s steps should not produce huge unwrapped longitude hops; max Δlon {max_dlon}°"
        );
    }

    #[test]
    fn ground_track_csv_and_json_round_trip_shape() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let start = tle.epoch;
        let end = start + Duration::minutes(30);
        let v = ground_track(&tle, start, end, Duration::minutes(10)).unwrap();
        let csv = ground_track_to_csv(&v);
        assert!(csv.starts_with("time_utc,latitude_deg,longitude_deg,altitude_km\n"));
        assert!(csv.lines().count() >= 2);
        let json = ground_track_to_json(&v).unwrap();
        assert!(json.contains("\"latitude_deg\"") && json.contains("\"time\""));
    }

    #[test]
    fn predict_passes_is_deterministic() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let obs = ObserverLocation {
            latitude: 47.9088,
            longitude: -122.2503,
            elevation: 0.0,
        };
        let start = tle.epoch;
        let end = start + Duration::hours(72);
        let a = predict_passes(&tle, obs, start, end, 10.0).unwrap();
        let b = predict_passes(&tle, obs, start, end, 10.0).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn predict_passes_iss_finds_visible_pass_over_everett() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let obs = ObserverLocation {
            latitude: 47.9088,
            longitude: -122.2503,
            elevation: 0.0,
        };
        let start = tle.epoch;
        let end = start + Duration::hours(72);
        let passes = predict_passes(&tle, obs, start, end, 10.0).expect("predict");
        assert!(
            !passes.is_empty(),
            "expected at least one pass over 72 h for ISS / Everett"
        );
        let p = passes
            .iter()
            .max_by(|x, y| {
                x.max_elevation_deg
                    .partial_cmp(&y.max_elevation_deg)
                    .unwrap()
            })
            .unwrap();
        assert!(p.max_elevation_deg > 12.0, "max el {}", p.max_elevation_deg);
        assert!(p.aos <= p.culmination && p.culmination <= p.los);
        assert!(p.aos >= start && p.los <= end);
        assert!((0.0..360.0).contains(&p.aos_azimuth_deg));
        assert!((0.0..360.0).contains(&p.los_azimuth_deg));
    }

    #[test]
    fn predict_passes_empty_for_extreme_mask() {
        let tle = Tle::parse(&iss_3line()).unwrap();
        let obs = ObserverLocation {
            latitude: -89.5,
            longitude: 0.0,
            elevation: 0.0,
        };
        let start = tle.epoch;
        let end = start + Duration::hours(6);
        let passes = predict_passes(&tle, obs, start, end, 89.0).unwrap();
        assert!(
            passes.is_empty(),
            "ISS should not reach 89° from deep south pole in 6 h"
        );
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
