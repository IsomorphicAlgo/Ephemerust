//! Orbit Mean-Elements Message (OMM) ingestion.
//!
//! An OMM carries the same SGP4 mean elements as a Two-Line Element set, but as **named
//! keywords** instead of fixed columns. It is defined by the CCSDS Orbit Data Messages
//! standard (CCSDS 502.0-B-3), and it is the format element-set providers publish for every
//! object, including the ones a TLE cannot describe.
//!
//! # Why this module exists
//!
//! A TLE stores the satellite catalog number in five columns. The public catalog reached
//! 69999 on 2026-07-11 (the block 70000–99999 is reserved for temporary identifiers), so
//! objects cataloged since then are numbered 100000 and up. Some providers squeeze those
//! numbers into a TLE with the *Alpha-5* letter scheme, which [`Tle`] also reads, but that
//! scheme stops at 339999. The OMM has no such limit: `NORAD_CAT_ID` is an ordinary integer of
//! up to nine digits, and its `EPOCH` is a full ISO 8601 timestamp with a four-digit year
//! instead of the TLE's two-digit year and day-of-year.
//!
//! The physics is unchanged. An OMM written for SGP4 is the *same* set of Kozai mean elements
//! in the TEME frame, so it is propagated by the same engine: build a
//! [`Propagator`](crate::satellite::Propagator) with
//! [`Propagator::from_omm`](crate::satellite::Propagator::from_omm) and every downstream
//! computation (state, sub-point, look angles, passes, ground track, shadow) works as it does
//! for a TLE.
//!
//! # Serializations
//!
//! | [`OmmFormat`] | Where it comes from | Shape |
//! |---------------|---------------------|-------|
//! | [`Json`](OmmFormat::Json) | CelesTrak `FORMAT=json`, Space-Track | one object, or an array of objects; values may be numbers or strings |
//! | [`Kvn`](OmmFormat::Kvn) | CCSDS key-value notation, CelesTrak `FORMAT=kvn` | `KEYWORD = value` lines; messages are concatenated |
//! | [`Xml`](OmmFormat::Xml) | CCSDS XML, CelesTrak `FORMAT=xml` | one `<omm>` element per message, usually inside `<ndm>` |
//! | [`Csv`](OmmFormat::Csv) | CelesTrak `FORMAT=csv` | a header row of keywords, then one row per object |
//!
//! All four reduce to the same list of `(keyword, value)` pairs, which is then validated once
//! by a single code path. JSON and CSV are provider conveniences that reuse the OMM keywords:
//! they leave out the header and the metadata keywords whose value is fixed for SGP4 data
//! (`CENTER_NAME`, `REF_FRAME`, `TIME_SYSTEM`, `MEAN_ELEMENT_THEORY`). Those keywords are
//! therefore optional here, but when present they must say what SGP4 requires.
//!
//! # Keywords
//!
//! | Keyword | [`Omm`] field | Unit | Required |
//! |---------|---------------|------|----------|
//! | `OBJECT_NAME` | [`name`](Omm::name) | | no |
//! | `OBJECT_ID` | [`international_designator`](Omm::international_designator) | | no |
//! | `EPOCH` | [`epoch`](Omm::epoch) | UTC | **yes** |
//! | `MEAN_MOTION` | [`mean_motion`](Omm::mean_motion) | rev/day | **yes** |
//! | `ECCENTRICITY` | [`eccentricity`](Omm::eccentricity) | | **yes** |
//! | `INCLINATION` | [`inclination_deg`](Omm::inclination_deg) | deg | **yes** |
//! | `RA_OF_ASC_NODE` | [`raan_deg`](Omm::raan_deg) | deg | **yes** |
//! | `ARG_OF_PERICENTER` | [`arg_perigee_deg`](Omm::arg_perigee_deg) | deg | **yes** |
//! | `MEAN_ANOMALY` | [`mean_anomaly_deg`](Omm::mean_anomaly_deg) | deg | **yes** |
//! | `BSTAR` | [`bstar`](Omm::bstar) | 1/Earth radii | **yes** |
//! | `NORAD_CAT_ID` | [`catalog_number`](Omm::catalog_number) | | no |
//! | `CLASSIFICATION_TYPE` | [`classification`](Omm::classification) | | no (`U`) |
//! | `EPHEMERIS_TYPE` | [`ephemeris_type`](Omm::ephemeris_type) | | no (0) |
//! | `ELEMENT_SET_NO` | [`element_set_number`](Omm::element_set_number) | | no (0) |
//! | `REV_AT_EPOCH` | [`revolution_number`](Omm::revolution_number) | | no (0) |
//! | `MEAN_MOTION_DOT` | [`mean_motion_dot`](Omm::mean_motion_dot) | rev/day² | no (0) |
//! | `MEAN_MOTION_DDOT` | [`mean_motion_ddot`](Omm::mean_motion_ddot) | rev/day³ | no (0) |
//!
//! `BSTAR` is required because SGP4's drag model reads it: assuming zero would silently change
//! the predicted orbit. The two mean-motion derivatives default to zero because SGP4 does not
//! read them at all (they belong to the older SGP model). Unknown keywords, comments and
//! user-defined parameters are ignored.
//!
//! # Limits
//!
//! - Only SGP4 mean elements are accepted. A message that declares another theory, frame, time
//!   system or central body, or `EPHEMERIS_TYPE = 4` (SGP4-XP), is rejected with an
//!   explanation instead of being propagated with the wrong model.
//! - The XML reader is a small scanner for the flat OMM layout, not a general XML parser: it
//!   reads element text and the five predefined entities, and does not process DTDs or `CDATA`
//!   sections.
//! - A CSV field may be quoted, but may not contain a line break.
//!
//! # Example
//!
//! ```
//! use ephemerust::omm::Omm;
//! use ephemerust::satellite::Propagator;
//!
//! // CelesTrak-style JSON for an object numbered above the TLE's five-digit limit.
//! let omm = Omm::parse(
//!     r#"[{
//!         "OBJECT_NAME": "EXAMPLE-SAT",
//!         "OBJECT_ID": "2026-150A",
//!         "EPOCH": "2026-08-01T12:00:00.000000",
//!         "MEAN_MOTION": 15.2,
//!         "ECCENTRICITY": 0.0012,
//!         "INCLINATION": 97.5,
//!         "RA_OF_ASC_NODE": 120.0,
//!         "ARG_OF_PERICENTER": 45.0,
//!         "MEAN_ANOMALY": 315.0,
//!         "EPHEMERIS_TYPE": 0,
//!         "CLASSIFICATION_TYPE": "U",
//!         "NORAD_CAT_ID": 100123,
//!         "ELEMENT_SET_NO": 999,
//!         "REV_AT_EPOCH": 42,
//!         "BSTAR": 0.0001,
//!         "MEAN_MOTION_DOT": 0.00001,
//!         "MEAN_MOTION_DDOT": 0
//!     }]"#,
//! )?;
//! assert_eq!(omm.catalog_number, Some(100_123));
//!
//! let prop = Propagator::from_omm(&omm)?;
//! let state = prop.propagate(omm.epoch)?;
//! let r = state.position_km;
//! let radius = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
//! assert!((6_800.0..7_100.0).contains(&radius));
//! # Ok::<(), ephemerust::AstroError>(())
//! ```

use crate::satellite::Tle;
use crate::{AstroError, Result};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use thiserror::Error;

/// The serialization an OMM arrived in.
///
/// The four formats carry the same keywords; see the [module docs](self) for where each one
/// comes from. [`OmmFormat::detect`] recognises them from the text alone, so callers rarely
/// need to name a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OmmFormat {
    /// JSON: one object, or an array of objects, keyed by OMM keyword.
    Json,
    /// CCSDS key-value notation: one `KEYWORD = value` per line.
    Kvn,
    /// CCSDS XML: one `<omm>` element per message.
    Xml,
    /// Comma-separated values: a header row of OMM keywords, then one row per object.
    Csv,
}

impl OmmFormat {
    /// Recognises the serialization from the text, or returns `None` if it does not look like
    /// an OMM at all (a TLE, for example).
    ///
    /// The test is structural and cheap: JSON starts with `{` or `[`, XML with `<`, KVN has a
    /// `KEYWORD = value` first line, and CSV has a comma-separated header row that names
    /// `EPOCH`. Recognising a format says nothing about whether the content is valid.
    ///
    /// ```
    /// use ephemerust::omm::OmmFormat;
    ///
    /// assert_eq!(OmmFormat::detect("[{\"EPOCH\": \"2026-08-01T00:00:00\"}]"), Some(OmmFormat::Json));
    /// assert_eq!(OmmFormat::detect("CCSDS_OMM_VERS = 2.0\n"), Some(OmmFormat::Kvn));
    /// assert_eq!(OmmFormat::detect("<?xml version=\"1.0\"?><ndm/>"), Some(OmmFormat::Xml));
    /// assert_eq!(OmmFormat::detect("OBJECT_NAME,EPOCH,MEAN_MOTION\n"), Some(OmmFormat::Csv));
    ///
    /// // A TLE is not an OMM.
    /// let tle = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
    /// assert_eq!(OmmFormat::detect(tle), None);
    /// ```
    pub fn detect(text: &str) -> Option<Self> {
        let body = text.trim_start_matches('\u{feff}').trim_start();
        match body.chars().next()? {
            '{' | '[' => return Some(OmmFormat::Json),
            '<' => return Some(OmmFormat::Xml),
            _ => {}
        }
        let first = body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !is_kvn_comment(l))?;
        if let Some((key, _)) = first.split_once('=')
            && is_keyword(key.trim())
        {
            return Some(OmmFormat::Kvn);
        }
        let mut columns = first.split(',').map(|c| c.trim().trim_matches('"'));
        if first.contains(',') && columns.any(|c| c.eq_ignore_ascii_case("EPOCH")) {
            return Some(OmmFormat::Csv);
        }
        None
    }

    /// A short label for messages and logs: `"JSON"`, `"KVN"`, `"XML"` or `"CSV"`.
    pub fn name(&self) -> &'static str {
        match self {
            OmmFormat::Json => "JSON",
            OmmFormat::Kvn => "KVN",
            OmmFormat::Xml => "XML",
            OmmFormat::Csv => "CSV",
        }
    }
}

impl std::fmt::Display for OmmFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A structured, *educational* error describing why an OMM could not be read.
///
/// Like [`TleError`](crate::satellite::TleError), each variant states what was expected, what
/// was found and the rule behind it, and [`OmmError::hint`] offers a short next step. Where a
/// TLE fails by *position* (a shifted column), an OMM fails by *keyword*, so the errors name
/// the keyword instead of a column range.
#[derive(Error, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum OmmError {
    /// The text is not JSON, KVN, XML or CSV in any recognisable OMM shape.
    #[error(
        "the input is not a recognisable OMM. An OMM is JSON (starts with `{{` or `[`), XML \
         (starts with `<`), key-value notation (`KEYWORD = value` lines) or CSV (a header row \
         of keywords that includes EPOCH)"
    )]
    UnrecognizedFormat,

    /// The text was recognised as one of the formats but is not well formed.
    #[error("the {format} text is malformed: {detail}")]
    Syntax {
        /// The format the text was read as.
        format: OmmFormat,
        /// What is wrong, with a line number where one is known.
        detail: String,
    },

    /// The text is well formed but holds no OMM message.
    #[error(
        "the {format} input contains no OMM message. A message needs at least an EPOCH and \
         the six mean elements"
    )]
    NoMessages {
        /// The format the text was read as.
        format: OmmFormat,
    },

    /// [`Omm::parse`] expects exactly one message and found several.
    #[error(
        "expected exactly one OMM message, but found {found}. Providers usually answer a \
         group query with one message per object"
    )]
    WrongMessageCount {
        /// Number of messages found.
        found: usize,
    },

    /// A keyword that SGP4 cannot do without is absent (or present with an empty value).
    #[error("the keyword {keyword} is missing or empty. {why}")]
    MissingKeyword {
        /// The absent OMM keyword.
        keyword: &'static str,
        /// Why the keyword is needed.
        why: &'static str,
    },

    /// A value could not be read as the type its keyword requires.
    #[error("could not read {keyword} from '{found}': {reason}")]
    InvalidValue {
        /// The OMM keyword whose value failed to parse.
        keyword: &'static str,
        /// The raw text that was found.
        found: String,
        /// Why the parse failed, including the expected shape.
        reason: String,
    },

    /// A number parsed correctly but is outside the physically meaningful range.
    #[error("{keyword} = {value} is out of range: {expected}")]
    OutOfRange {
        /// The OMM keyword whose value is out of range.
        keyword: &'static str,
        /// The value that was found.
        value: f64,
        /// The accepted range and the reason for it.
        expected: &'static str,
    },

    /// The message describes something other than SGP4 mean elements in TEME/UTC about the
    /// Earth, which this crate cannot propagate.
    #[error("{keyword} = {found} is not supported: {reason}")]
    Unsupported {
        /// The OMM keyword carrying the unsupported value.
        keyword: &'static str,
        /// The value that was found.
        found: String,
        /// What is supported instead, and why.
        reason: &'static str,
    },

    /// Wraps an error with the position and name of the message it occurred in, when the
    /// input held more than one message.
    #[error("OMM message {index} ({object}): {source}")]
    InMessage {
        /// 1-based position of the message in the input.
        index: usize,
        /// The message's object name, or its catalog number when it has no name.
        object: String,
        /// The underlying error.
        #[source]
        source: Box<OmmError>,
    },
}

impl OmmError {
    /// Returns a short, actionable hint for this error, suitable for display on a dedicated
    /// `Hint:` line after the error message.
    pub fn hint(&self) -> Option<&'static str> {
        Some(match self {
            OmmError::UnrecognizedFormat => {
                "Request an explicit format from the provider (for CelesTrak: FORMAT=json, kvn, \
                 xml or csv). If the text is a two-line element set, parse it as a TLE instead."
            }
            OmmError::Syntax { .. } => {
                "Re-download the message; a truncated response or an HTML error page is the \
                 usual cause."
            }
            OmmError::NoMessages { .. } => {
                "Check that the query matched an object: a provider answers an unmatched query \
                 with an empty list or a short notice instead of an error status."
            }
            OmmError::WrongMessageCount { .. } => {
                "Use `Omm::parse_all` to read every message, or `select_omm` to pick one by \
                 name or catalog number."
            }
            OmmError::MissingKeyword { .. } => {
                "An SGP4 OMM needs EPOCH, MEAN_MOTION, ECCENTRICITY, INCLINATION, \
                 RA_OF_ASC_NODE, ARG_OF_PERICENTER, MEAN_ANOMALY and BSTAR."
            }
            OmmError::InvalidValue { .. } => {
                "OMM numbers are plain decimals (an exponent such as 1.5E-4 is allowed) and \
                 EPOCH is ISO 8601, e.g. 2026-08-01T12:00:00.000000."
            }
            OmmError::OutOfRange { .. } => {
                "Angles are in degrees, MEAN_MOTION is in revolutions per day and ECCENTRICITY \
                 is a plain fraction below 1, not the TLE's implied-decimal digits."
            }
            OmmError::Unsupported { .. } => {
                "Request general-perturbations (GP) data: SGP4 mean elements in the TEME frame, \
                 UTC, about the Earth."
            }
            OmmError::InMessage { source, .. } => return source.hint(),
        })
    }
}

/// A parsed and validated Orbit Mean-Elements Message for SGP4.
///
/// The fields mirror [`Tle`] so the two can be used interchangeably, with three differences
/// that follow from the format: the catalog number is optional (an operator may publish
/// elements for an object that has not been cataloged yet) and may exceed five digits, the
/// international designator is in its full form (`1998-067A`, not `98067A`), and there are no
/// retained text lines.
///
/// Angles are in degrees, the mean motion is in revolutions per day, and the epoch is UTC.
/// The elements are SGP4 *mean* elements in the TEME frame; they are meaningful only when
/// propagated with SGP4 (see [`Propagator::from_omm`](crate::satellite::Propagator::from_omm)),
/// not as osculating Keplerian elements.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Omm {
    /// Object name (`OBJECT_NAME`).
    pub name: Option<String>,
    /// International designator in full form, e.g. `1998-067A` (`OBJECT_ID`).
    pub international_designator: Option<String>,
    /// Satellite catalog number, up to nine digits (`NORAD_CAT_ID`).
    pub catalog_number: Option<u32>,
    /// Classification: `U` (unclassified), `C` (classified), or `S` (secret).
    pub classification: char,
    /// Epoch of the element set (UTC).
    pub epoch: DateTime<Utc>,

    /// First derivative of mean motion divided by two, in revolutions per day squared, as in a
    /// TLE. Not used by SGP4.
    pub mean_motion_dot: f64,
    /// Second derivative of mean motion divided by six, in revolutions per day cubed, as in a
    /// TLE. Not used by SGP4.
    pub mean_motion_ddot: f64,
    /// B* drag coefficient, in inverse Earth radii.
    pub bstar: f64,
    /// Element set number.
    pub element_set_number: u32,
    /// Ephemeris type: 0 in distributed data. 4 (SGP4-XP) is rejected at parse time.
    pub ephemeris_type: u8,

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
    /// Mean motion in revolutions per day (Kozai convention, as in a TLE).
    pub mean_motion: f64,
    /// Revolution number at epoch.
    pub revolution_number: u32,
}

impl Omm {
    /// Parses text that holds **exactly one** OMM, in any [`OmmFormat`].
    ///
    /// A JSON array or a CSV table with a single entry counts as one message. For text with
    /// several objects use [`Omm::parse_all`] or [`select_omm`].
    ///
    /// # Errors
    ///
    /// [`AstroError::Omm`] wrapping the [`OmmError`] that explains the failure.
    ///
    /// # Example
    ///
    /// The same ISS element set as the [`Tle`] examples, in key-value notation:
    ///
    /// ```
    /// use ephemerust::omm::Omm;
    ///
    /// let omm = Omm::parse(
    ///     "CCSDS_OMM_VERS = 2.0\n\
    ///      OBJECT_NAME = ISS (ZARYA)\n\
    ///      OBJECT_ID = 1998-067A\n\
    ///      CENTER_NAME = EARTH\n\
    ///      REF_FRAME = TEME\n\
    ///      TIME_SYSTEM = UTC\n\
    ///      MEAN_ELEMENT_THEORY = SGP4\n\
    ///      EPOCH = 2020-07-12T21:16:01.000416\n\
    ///      MEAN_MOTION = 15.49507896\n\
    ///      ECCENTRICITY = .0001413\n\
    ///      INCLINATION = 51.6461\n\
    ///      RA_OF_ASC_NODE = 221.2784\n\
    ///      ARG_OF_PERICENTER = 89.1723\n\
    ///      MEAN_ANOMALY = 280.4612\n\
    ///      EPHEMERIS_TYPE = 0\n\
    ///      CLASSIFICATION_TYPE = U\n\
    ///      NORAD_CAT_ID = 25544\n\
    ///      ELEMENT_SET_NO = 999\n\
    ///      REV_AT_EPOCH = 23600\n\
    ///      BSTAR = -.31515E-4\n\
    ///      MEAN_MOTION_DOT = -.2218E-4\n\
    ///      MEAN_MOTION_DDOT = 0\n",
    /// )?;
    ///
    /// assert_eq!(omm.name.as_deref(), Some("ISS (ZARYA)"));
    /// assert_eq!(omm.catalog_number, Some(25544));
    /// assert!((omm.inclination_deg - 51.6461).abs() < 1e-12);
    /// # Ok::<(), ephemerust::AstroError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self> {
        let (_, records) = raw_records(text)?;
        match records.as_slice() {
            [one] => Ok(Omm::from_record(one)?),
            many => Err(OmmError::WrongMessageCount { found: many.len() }.into()),
        }
    }

    /// Parses **every** OMM in the text, in any [`OmmFormat`], preserving their order.
    ///
    /// This is the entry point for multi-object work (a provider's group query, a saved
    /// catalog). Every message is validated; the first invalid one stops the parse, and its
    /// error names the message by position and object so it can be found in a long file.
    ///
    /// # Errors
    ///
    /// [`AstroError::Omm`]. With more than one message in the input, a per-message failure is
    /// wrapped in [`OmmError::InMessage`].
    ///
    /// # Example
    ///
    /// ```
    /// use ephemerust::omm::Omm;
    ///
    /// let csv = "\
    /// OBJECT_NAME,OBJECT_ID,EPOCH,MEAN_MOTION,ECCENTRICITY,INCLINATION,RA_OF_ASC_NODE,ARG_OF_PERICENTER,MEAN_ANOMALY,EPHEMERIS_TYPE,CLASSIFICATION_TYPE,NORAD_CAT_ID,ELEMENT_SET_NO,REV_AT_EPOCH,BSTAR,MEAN_MOTION_DOT,MEAN_MOTION_DDOT
    /// ISS (ZARYA),1998-067A,2020-07-12T21:16:01.000416,15.49507896,.0001413,51.6461,221.2784,89.1723,280.4612,0,U,25544,999,23600,-.31515E-4,-.2218E-4,0
    /// EXAMPLE-SAT,2026-150A,2026-08-01T12:00:00.000000,15.2,.0012,97.5,120.0,45.0,315.0,0,U,100123,999,42,.1E-3,.1E-4,0
    /// ";
    /// let all = Omm::parse_all(csv)?;
    /// assert_eq!(all.len(), 2);
    /// assert_eq!(all[1].catalog_number, Some(100_123));
    /// # Ok::<(), ephemerust::AstroError>(())
    /// ```
    pub fn parse_all(text: &str) -> Result<Vec<Self>> {
        let (_, records) = raw_records(text)?;
        let single = records.len() == 1;
        let mut out = Vec::with_capacity(records.len());
        for (i, record) in records.iter().enumerate() {
            match Omm::from_record(record) {
                Ok(omm) => out.push(omm),
                Err(e) if single => return Err(e.into()),
                Err(e) => {
                    return Err(OmmError::InMessage {
                        index: i + 1,
                        object: record.display_name(),
                        source: Box::new(e),
                    }
                    .into());
                }
            }
        }
        Ok(out)
    }

    /// Reads and parses a single OMM from a file path (see [`Omm::parse`]).
    pub fn from_file(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Validates one message's keyword/value pairs and builds the typed view.
    fn from_record(record: &RawRecord) -> std::result::Result<Self, OmmError> {
        // Metadata first: a message for another theory, frame, time system or central body
        // must not be mistaken for SGP4 input just because its numbers parse.
        check_metadata(
            record,
            "CENTER_NAME",
            &["EARTH"],
            "SGP4 models orbits about the Earth, so only CENTER_NAME = EARTH can be propagated",
        )?;
        check_metadata(
            record,
            "REF_FRAME",
            &["TEME"],
            "SGP4 mean elements are defined in the TEME frame, so only REF_FRAME = TEME is \
             accepted",
        )?;
        check_metadata(
            record,
            "TIME_SYSTEM",
            &["UTC"],
            "SGP4 epochs are UTC, so only TIME_SYSTEM = UTC is accepted",
        )?;
        check_metadata(
            record,
            "MEAN_ELEMENT_THEORY",
            &["SGP4", "SGP/SGP4"],
            "only SGP4 mean elements can be propagated here; elements fitted to another theory \
             (SGP4-XP, DSST, USM, …) give wrong positions when run through SGP4",
        )?;

        let epoch_text = required(
            record,
            "EPOCH",
            "It is the instant the mean elements are valid for; propagation is measured from it",
        )?;
        let epoch = parse_epoch(epoch_text)?;

        let mean_motion = match record.get("MEAN_MOTION") {
            Some(text) => parse_real("MEAN_MOTION", text)?,
            None if record.get("SEMI_MAJOR_AXIS").is_some() => {
                return Err(OmmError::Unsupported {
                    keyword: "SEMI_MAJOR_AXIS",
                    found: record
                        .get("SEMI_MAJOR_AXIS")
                        .unwrap_or_default()
                        .to_string(),
                    reason: "SGP4 is initialised from the Kozai mean motion in revolutions per \
                             day, so the message must carry MEAN_MOTION",
                });
            }
            None => {
                return Err(OmmError::MissingKeyword {
                    keyword: "MEAN_MOTION",
                    why: "It sets the size of the orbit: SGP4 derives the semi-major axis from \
                          the mean motion in revolutions per day",
                });
            }
        };
        if mean_motion <= 0.0 {
            return Err(OmmError::OutOfRange {
                keyword: "MEAN_MOTION",
                value: mean_motion,
                expected: "it must be greater than 0 revolutions per day (about 1 for a \
                           geostationary orbit, 15 to 16 for low Earth orbit)",
            });
        }

        let eccentricity = parse_real(
            "ECCENTRICITY",
            required(record, "ECCENTRICITY", "It sets the shape of the orbit")?,
        )?;
        if !(0.0..1.0).contains(&eccentricity) {
            return Err(OmmError::OutOfRange {
                keyword: "ECCENTRICITY",
                value: eccentricity,
                expected: "it must be at least 0 and below 1, the range of closed (elliptical) \
                           orbits that SGP4 models",
            });
        }

        let inclination_deg = parse_real(
            "INCLINATION",
            required(
                record,
                "INCLINATION",
                "It is the tilt of the orbit plane against the equator",
            )?,
        )?;
        if !(0.0..=180.0).contains(&inclination_deg) {
            return Err(OmmError::OutOfRange {
                keyword: "INCLINATION",
                value: inclination_deg,
                expected: "it must be between 0 and 180 degrees (0 equatorial, 90 polar, above \
                           90 retrograde)",
            });
        }

        let raan_deg = parse_angle(
            record,
            "RA_OF_ASC_NODE",
            "It orients the orbit plane about the Earth's axis",
        )?;
        let arg_perigee_deg = parse_angle(
            record,
            "ARG_OF_PERICENTER",
            "It orients the ellipse within the orbit plane",
        )?;
        let mean_anomaly_deg = parse_angle(
            record,
            "MEAN_ANOMALY",
            "It places the satellite along the orbit at the epoch",
        )?;

        let bstar = parse_real(
            "BSTAR",
            required(
                record,
                "BSTAR",
                "It is SGP4's drag term; assuming zero would change the predicted orbit",
            )?,
        )?;

        let mean_motion_dot = optional_real(record, "MEAN_MOTION_DOT")?;
        let mean_motion_ddot = optional_real(record, "MEAN_MOTION_DDOT")?;

        let catalog_number = match record.get("NORAD_CAT_ID") {
            Some(text) => {
                let n = parse_count("NORAD_CAT_ID", text)?;
                if n > MAX_CATALOG_NUMBER {
                    return Err(OmmError::OutOfRange {
                        keyword: "NORAD_CAT_ID",
                        value: f64::from(n),
                        expected: "catalog numbers have at most nine digits",
                    });
                }
                Some(n)
            }
            None => None,
        };
        let element_set_number = optional_count(record, "ELEMENT_SET_NO")?;
        let revolution_number = optional_count(record, "REV_AT_EPOCH")?;

        let ephemeris_type = match record.get("EPHEMERIS_TYPE") {
            Some(text) => {
                let n = parse_count("EPHEMERIS_TYPE", text)?;
                if n == 4 {
                    return Err(OmmError::Unsupported {
                        keyword: "EPHEMERIS_TYPE",
                        found: text.to_string(),
                        reason: "type 4 marks SGP4-XP elements, which need the SGP4-XP \
                                 propagator; running them through SGP4 gives wrong positions",
                    });
                }
                u8::try_from(n).map_err(|_| OmmError::InvalidValue {
                    keyword: "EPHEMERIS_TYPE",
                    found: text.to_string(),
                    reason: "expected a small integer (0 in distributed data)".into(),
                })?
            }
            None => 0,
        };

        let classification = match record.get("CLASSIFICATION_TYPE") {
            None => 'U',
            Some(text) => match text.to_ascii_uppercase().as_str() {
                "U" => 'U',
                "C" => 'C',
                "S" => 'S',
                _ => {
                    return Err(OmmError::InvalidValue {
                        keyword: "CLASSIFICATION_TYPE",
                        found: text.to_string(),
                        reason: "expected U (unclassified), C (classified) or S (secret)".into(),
                    });
                }
            },
        };

        Ok(Omm {
            name: record.get("OBJECT_NAME").map(str::to_string),
            international_designator: record.get("OBJECT_ID").map(str::to_string),
            catalog_number,
            classification,
            epoch,
            mean_motion_dot,
            mean_motion_ddot,
            bstar,
            element_set_number,
            ephemeris_type,
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

/// Parses a single OMM from text, exactly like [`Omm::parse`].
///
/// ```
/// use ephemerust::omm::Omm;
///
/// let omm: Omm = r#"{"EPOCH":"2020-07-12T21:16:01.000416","MEAN_MOTION":15.49507896,
///     "ECCENTRICITY":0.0001413,"INCLINATION":51.6461,"RA_OF_ASC_NODE":221.2784,
///     "ARG_OF_PERICENTER":89.1723,"MEAN_ANOMALY":280.4612,"BSTAR":-0.31515E-4}"#
///     .parse()?;
/// assert_eq!(omm.catalog_number, None);
/// # Ok::<(), ephemerust::AstroError>(())
/// ```
impl std::str::FromStr for Omm {
    type Err = AstroError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Omm::parse(s)
    }
}

/// Re-expresses a TLE as an OMM, so code that works on [`Omm`] accepts both sources.
///
/// The orbital elements are copied unchanged. The international designator is expanded to its
/// full form (`98067A` → `1998-067A`) using the TLE's two-digit-year rule (57–99 → 1957–1999,
/// 00–56 → 2000–2056), and the ephemeris type is taken as 0.
///
/// A [`Propagator`](crate::satellite::Propagator) built from the converted value agrees with
/// one built from the TLE to well under a millimetre; the residual comes from the epoch,
/// which the two paths round independently.
///
/// ```
/// use ephemerust::omm::Omm;
/// use ephemerust::satellite::Tle;
///
/// let tle = Tle::parse(
///     "ISS (ZARYA)\n\
///      1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
///      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
/// )?;
/// let omm = Omm::from(&tle);
/// assert_eq!(omm.catalog_number, Some(25544));
/// assert_eq!(omm.international_designator.as_deref(), Some("1998-067A"));
/// assert_eq!(omm.epoch, tle.epoch);
/// # Ok::<(), ephemerust::AstroError>(())
/// ```
impl From<&Tle> for Omm {
    fn from(tle: &Tle) -> Self {
        Omm {
            name: tle.name.clone(),
            international_designator: expand_designator(&tle.international_designator),
            catalog_number: Some(tle.catalog_number),
            classification: tle.classification,
            epoch: tle.epoch,
            mean_motion_dot: tle.mean_motion_dot,
            mean_motion_ddot: tle.mean_motion_ddot,
            bstar: tle.bstar,
            element_set_number: tle.element_set_number,
            ephemeris_type: 0,
            inclination_deg: tle.inclination_deg,
            raan_deg: tle.raan_deg,
            eccentricity: tle.eccentricity,
            arg_perigee_deg: tle.arg_perigee_deg,
            mean_anomaly_deg: tle.mean_anomaly_deg,
            mean_motion: tle.mean_motion,
            revolution_number: tle.revolution_number,
        }
    }
}

/// Selects **one** OMM from text that may contain many, mirroring
/// [`select_tle`](crate::satellite::select_tle).
///
/// - **`selector: None`** — the text must contain exactly one message.
/// - **`selector: Some(s)`** — `s` is matched **case-insensitively as a substring** of each
///   `OBJECT_NAME`; a purely numeric `s` also matches the **exact `NORAD_CAT_ID`**. Exactly
///   one message must match.
///
/// Only the *selected* message is validated, so one bad entry elsewhere in a large file does
/// not prevent selection. Ambiguity and no-match errors list the available objects.
///
/// # Errors
///
/// [`AstroError::SatelliteError`] for an ambiguous or unmatched selector, and
/// [`AstroError::Omm`] if the text is not an OMM or the selected message fails validation.
///
/// # Example
///
/// ```
/// use ephemerust::omm::select_omm;
///
/// let json = r#"[
///   {"OBJECT_NAME":"ISS (ZARYA)","NORAD_CAT_ID":25544,"EPOCH":"2020-07-12T21:16:01.000416",
///    "MEAN_MOTION":15.49507896,"ECCENTRICITY":0.0001413,"INCLINATION":51.6461,
///    "RA_OF_ASC_NODE":221.2784,"ARG_OF_PERICENTER":89.1723,"MEAN_ANOMALY":280.4612,
///    "BSTAR":-0.31515E-4},
///   {"OBJECT_NAME":"EXAMPLE-SAT","NORAD_CAT_ID":100123,"EPOCH":"2026-08-01T12:00:00",
///    "MEAN_MOTION":15.2,"ECCENTRICITY":0.0012,"INCLINATION":97.5,"RA_OF_ASC_NODE":120.0,
///    "ARG_OF_PERICENTER":45.0,"MEAN_ANOMALY":315.0,"BSTAR":0.0001}
/// ]"#;
///
/// let iss = select_omm(json, Some("zarya"))?;
/// assert_eq!(iss.catalog_number, Some(25544));
///
/// // A numeric selector matches the catalog number exactly, at any length.
/// let sat = select_omm(json, Some("100123"))?;
/// assert_eq!(sat.name.as_deref(), Some("EXAMPLE-SAT"));
/// # Ok::<(), ephemerust::AstroError>(())
/// ```
pub fn select_omm(text: &str, selector: Option<&str>) -> Result<Omm> {
    let (_, records) = raw_records(text)?;

    /// Formats up to 12 object names for an error message.
    fn list_names(records: &[&RawRecord]) -> String {
        let mut names: Vec<String> = records.iter().take(12).map(|r| r.display_name()).collect();
        if records.len() > names.len() {
            names.push(format!("… and {} more", records.len() - names.len()));
        }
        names.join(", ")
    }

    let chosen = match selector {
        None => match records.as_slice() {
            [one] => one,
            many => {
                let all: Vec<&RawRecord> = many.iter().collect();
                return Err(AstroError::SatelliteError(format!(
                    "the input contains {} OMM messages; select one by name or catalog number \
                     (available: {})",
                    many.len(),
                    list_names(&all)
                )));
            }
        },
        Some(sel) => {
            let sel_trimmed = sel.trim();
            let sel_lower = sel_trimmed.to_lowercase();
            let sel_number =
                if !sel_trimmed.is_empty() && sel_trimmed.bytes().all(|b| b.is_ascii_digit()) {
                    sel_trimmed.parse::<u64>().ok()
                } else {
                    None
                };

            let matches: Vec<&RawRecord> = records
                .iter()
                .filter(|r| {
                    let name_hit = r
                        .get("OBJECT_NAME")
                        .is_some_and(|n| n.to_lowercase().contains(&sel_lower));
                    let catalog_hit = sel_number.is_some()
                        && r.get("NORAD_CAT_ID")
                            .and_then(|c| c.trim().parse::<u64>().ok())
                            == sel_number;
                    name_hit || catalog_hit
                })
                .collect();

            match matches.as_slice() {
                [one] => *one,
                [] => {
                    let all: Vec<&RawRecord> = records.iter().collect();
                    return Err(AstroError::SatelliteError(format!(
                        "no OMM message matches \"{sel_trimmed}\" (available: {})",
                        list_names(&all)
                    )));
                }
                many => {
                    return Err(AstroError::SatelliteError(format!(
                        "\"{sel_trimmed}\" matches {} OMM messages; be more specific \
                         (matches: {})",
                        many.len(),
                        list_names(many)
                    )));
                }
            }
        }
    };

    Ok(Omm::from_record(chosen)?)
}

// ---------------------------------------------------------------------------------------------
// Shared layer: every serialization is reduced to keyword/value pairs, then validated once.
// ---------------------------------------------------------------------------------------------

/// Largest catalog number an OMM can carry (nine digits).
const MAX_CATALOG_NUMBER: u32 = 999_999_999;

/// One message as it appears in the text: keywords (upper-cased) and their raw, trimmed,
/// non-empty values, before any validation.
#[derive(Debug, Default)]
struct RawRecord {
    pairs: Vec<(String, String)>,
}

impl RawRecord {
    /// Adds a pair. Empty values are dropped: providers send blank or null for "not known",
    /// which is the same as leaving the keyword out.
    fn push(&mut self, keyword: &str, value: &str) {
        let value = value.trim();
        if !value.is_empty() {
            self.pairs
                .push((keyword.trim().to_ascii_uppercase(), value.to_string()));
        }
    }

    /// The first value recorded for `keyword`, if any.
    fn get(&self, keyword: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == keyword)
            .map(|(_, v)| v.as_str())
    }

    fn contains(&self, keyword: &str) -> bool {
        self.get(keyword).is_some()
    }

    fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Name for error messages: the object name, else the catalog number, else a placeholder.
    fn display_name(&self) -> String {
        match (self.get("OBJECT_NAME"), self.get("NORAD_CAT_ID")) {
            (Some(name), _) => name.to_string(),
            (None, Some(id)) => format!("catalog {id}"),
            (None, None) => "unnamed object".to_string(),
        }
    }
}

/// Detects the format and splits the text into unvalidated messages.
fn raw_records(text: &str) -> std::result::Result<(OmmFormat, Vec<RawRecord>), OmmError> {
    let format = OmmFormat::detect(text).ok_or(OmmError::UnrecognizedFormat)?;
    let body = text.trim_start_matches('\u{feff}');
    let records = match format {
        OmmFormat::Json => records_from_json(body)?,
        OmmFormat::Kvn => records_from_kvn(body)?,
        OmmFormat::Xml => records_from_xml(body)?,
        OmmFormat::Csv => records_from_csv(body)?,
    };
    if records.is_empty() {
        return Err(OmmError::NoMessages { format });
    }
    Ok((format, records))
}

/// `true` for an OMM-style keyword: upper-case ASCII letters, digits and underscores.
fn is_keyword(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// `true` for a KVN comment line (`COMMENT`, alone or followed by free text).
fn is_kvn_comment(line: &str) -> bool {
    line == "COMMENT" || line.starts_with("COMMENT ") || line.starts_with("COMMENT\t")
}

fn syntax(format: OmmFormat, detail: impl Into<String>) -> OmmError {
    OmmError::Syntax {
        format,
        detail: detail.into(),
    }
}

// --- JSON -----------------------------------------------------------------------------------

fn records_from_json(text: &str) -> std::result::Result<Vec<RawRecord>, OmmError> {
    use serde_json::Value;

    let value: Value =
        serde_json::from_str(text).map_err(|e| syntax(OmmFormat::Json, e.to_string()))?;
    let objects = match value {
        Value::Array(items) => items,
        object @ Value::Object(_) => vec![object],
        _ => {
            return Err(syntax(
                OmmFormat::Json,
                "expected an object or an array of objects",
            ));
        }
    };

    let mut records = Vec::with_capacity(objects.len());
    for (i, item) in objects.into_iter().enumerate() {
        let Value::Object(map) = item else {
            return Err(syntax(
                OmmFormat::Json,
                format!("array entry {} is not an object", i + 1),
            ));
        };
        let mut record = RawRecord::default();
        for (key, value) in &map {
            match value {
                // Providers disagree on quoting: CelesTrak sends numbers, Space-Track sends
                // strings. Both become text here and are parsed by the shared validator.
                Value::String(s) => record.push(key, s),
                Value::Number(n) => record.push(key, &n.to_string()),
                // null means "not known"; nested values are not part of the flat OMM layout.
                _ => {}
            }
        }
        if !record.is_empty() {
            records.push(record);
        }
    }
    Ok(records)
}

// --- KVN ------------------------------------------------------------------------------------

fn records_from_kvn(text: &str) -> std::result::Result<Vec<RawRecord>, OmmError> {
    let mut records = Vec::new();
    let mut current = RawRecord::default();

    for (i, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || is_kvn_comment(line) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(syntax(
                OmmFormat::Kvn,
                format!(
                    "line {} is not `KEYWORD = value`: '{}'",
                    i + 1,
                    truncate(line, 60)
                ),
            ));
        };
        let key = key.trim().to_ascii_uppercase();
        if !is_keyword(&key) {
            return Err(syntax(
                OmmFormat::Kvn,
                format!(
                    "line {} has '{}' where a keyword was expected",
                    i + 1,
                    truncate(&key, 40)
                ),
            ));
        }
        // A new message starts at its version line. Files without version lines are split
        // where a keyword repeats.
        let starts_new = key == "CCSDS_OMM_VERS" || current.contains(&key);
        if starts_new && !current.is_empty() {
            records.push(std::mem::take(&mut current));
        }
        current.push(&key, value);
    }
    if !current.is_empty() {
        records.push(current);
    }
    // A version line on its own is a header, not a message.
    records.retain(|r| r.pairs.iter().any(|(k, _)| k != "CCSDS_OMM_VERS"));
    Ok(records)
}

// --- XML ------------------------------------------------------------------------------------

/// Scans the flat OMM XML layout: each `<omm>` element is one message, and every element
/// inside it that holds only text is a keyword/value pair. Container elements (`header`,
/// `body`, `segment`, `metadata`, `data`, `meanElements`, `tleParameters`, …) are walked
/// through without being interpreted, so their order and nesting do not matter.
fn records_from_xml(text: &str) -> std::result::Result<Vec<RawRecord>, OmmError> {
    let cleaned = strip_xml_comments(text)?;
    let mut records = Vec::new();
    let mut current: Option<RawRecord> = None;
    let mut pos = 0;

    while let Some(offset) = cleaned[pos..].find('<') {
        let start = pos + offset;
        let end = start
            + cleaned[start..]
                .find('>')
                .ok_or_else(|| syntax(OmmFormat::Xml, "a tag opened with `<` is never closed"))?;
        let tag = &cleaned[start + 1..end];
        pos = end + 1;

        // Declarations (`<?xml …?>`) and doctype/CDATA markers carry no OMM data.
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            if xml_local_name(name).eq_ignore_ascii_case("omm")
                && let Some(record) = current.take()
                && !record.is_empty()
            {
                records.push(record);
            }
            continue;
        }

        let name = xml_local_name(tag);
        if name.eq_ignore_ascii_case("omm") {
            if current.is_some() {
                return Err(syntax(
                    OmmFormat::Xml,
                    "an <omm> element starts before the previous one is closed",
                ));
            }
            if !tag.ends_with('/') {
                current = Some(RawRecord::default());
            }
            continue;
        }
        if tag.ends_with('/') {
            continue; // self-closing: an empty value
        }

        // A leaf element is one whose closing tag follows its text directly.
        let Some(next) = cleaned[pos..].find('<') else {
            return Err(syntax(
                OmmFormat::Xml,
                format!("the element <{name}> is never closed"),
            ));
        };
        let rest = &cleaned[pos + next..];
        if let Some(close) = rest.strip_prefix("</") {
            let close_end = close
                .find('>')
                .ok_or_else(|| syntax(OmmFormat::Xml, "a tag opened with `</` is never closed"))?;
            let close_name = xml_local_name(&close[..close_end]);
            if close_name != name {
                return Err(syntax(
                    OmmFormat::Xml,
                    format!("<{name}> is closed by </{close_name}>"),
                ));
            }
            if let Some(record) = current.as_mut() {
                let value = decode_xml_entities(&cleaned[pos..pos + next])?;
                record.push(name, &value);
            }
            pos = pos + next + 2 + close_end + 1;
        }
    }

    if current.is_some() {
        return Err(syntax(
            OmmFormat::Xml,
            "an <omm> element is never closed (the text may be truncated)",
        ));
    }
    Ok(records)
}

/// Removes `<!-- … -->` comments so the scanner never mistakes their content for markup.
fn strip_xml_comments(text: &str) -> std::result::Result<String, OmmError> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let end = after.find("-->").ok_or_else(|| {
            syntax(
                OmmFormat::Xml,
                "a comment opened with `<!--` is never closed",
            )
        })?;
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The element name of a tag's inner text, without attributes or a namespace prefix
/// (`ndm:omm id="…"` → `omm`).
fn xml_local_name(tag: &str) -> &str {
    let name = tag
        .trim()
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("");
    name.rsplit(':').next().unwrap_or(name)
}

/// Decodes the five predefined XML entities and numeric character references.
fn decode_xml_entities(text: &str) -> std::result::Result<String, OmmError> {
    if !text.contains('&') {
        return Ok(text.to_string());
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after.find(';').ok_or_else(|| {
            syntax(
                OmmFormat::Xml,
                "an `&` is not followed by an entity name and `;` (write a literal ampersand \
                 as `&amp;`)",
            )
        })?;
        let entity = &after[..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| entity.strip_prefix('#').map(str::parse::<u32>))
                .and_then(|code| code.ok())
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => {
                return Err(syntax(
                    OmmFormat::Xml,
                    format!("unknown entity `&{};`", truncate(entity, 20)),
                ));
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

// --- CSV ------------------------------------------------------------------------------------

fn records_from_csv(text: &str) -> std::result::Result<Vec<RawRecord>, OmmError> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());

    let Some((header_no, header_line)) = lines.next() else {
        return Ok(Vec::new());
    };
    let header = split_csv_line(header_line)
        .map_err(|e| syntax(OmmFormat::Csv, format!("line {}: {e}", header_no + 1)))?;

    let mut records = Vec::new();
    for (i, line) in lines {
        let fields = split_csv_line(line)
            .map_err(|e| syntax(OmmFormat::Csv, format!("line {}: {e}", i + 1)))?;
        if fields.len() != header.len() {
            return Err(syntax(
                OmmFormat::Csv,
                format!(
                    "line {} has {} fields, but the header row names {} columns",
                    i + 1,
                    fields.len(),
                    header.len()
                ),
            ));
        }
        let mut record = RawRecord::default();
        for (key, value) in header.iter().zip(&fields) {
            record.push(key, value);
        }
        if !record.is_empty() {
            records.push(record);
        }
    }
    Ok(records)
}

/// Splits one CSV line into fields, honouring double-quoted fields with `""` as an escaped
/// quote. Line breaks inside a quoted field are not supported.
fn split_csv_line(line: &str) -> std::result::Result<Vec<String>, &'static str> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        match (c, in_quotes) {
            ('"', true) => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            }
            ('"', false) if field.trim().is_empty() => {
                field.clear();
                in_quotes = true;
            }
            (',', false) => fields.push(std::mem::take(&mut field)),
            (other, _) => field.push(other),
        }
    }
    if in_quotes {
        return Err("a quoted field is never closed");
    }
    fields.push(field);
    Ok(fields)
}

// --- Value parsing --------------------------------------------------------------------------

/// Returns the value of a keyword SGP4 cannot do without.
fn required<'a>(
    record: &'a RawRecord,
    keyword: &'static str,
    why: &'static str,
) -> std::result::Result<&'a str, OmmError> {
    record
        .get(keyword)
        .ok_or(OmmError::MissingKeyword { keyword, why })
}

/// Checks a metadata keyword that has exactly one meaningful value for SGP4. An absent
/// keyword is fine: JSON and CSV leave these out because the value is implied.
fn check_metadata(
    record: &RawRecord,
    keyword: &'static str,
    accepted: &[&str],
    reason: &'static str,
) -> std::result::Result<(), OmmError> {
    match record.get(keyword) {
        Some(found) if !accepted.iter().any(|a| found.eq_ignore_ascii_case(a)) => {
            Err(OmmError::Unsupported {
                keyword,
                found: found.to_string(),
                reason,
            })
        }
        _ => Ok(()),
    }
}

/// Removes an optional KVN unit suffix (`15.5 [rev/day]` → `15.5`).
fn strip_units(text: &str) -> &str {
    match text.rfind('[') {
        Some(open) if text.ends_with(']') => text[..open].trim_end(),
        _ => text,
    }
}

/// Parses a finite decimal number. Accepts the forms providers actually emit: a leading
/// decimal point (`.0001413`), an explicit sign, and an `E` exponent (`-.31515E-4`).
fn parse_real(keyword: &'static str, text: &str) -> std::result::Result<f64, OmmError> {
    let body = strip_units(text);
    let invalid = |reason: &str| OmmError::InvalidValue {
        keyword,
        found: text.to_string(),
        reason: reason.to_string(),
    };
    // `f64::from_str` also accepts "inf" and "nan"; neither is a physical value, so only
    // digits, sign, point and exponent are let through.
    if body.is_empty()
        || !body
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return Err(invalid(
            "expected a decimal number such as 51.6461, .0001413 or -.31515E-4",
        ));
    }
    match body.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        Ok(_) => Err(invalid("the number is too large to represent")),
        Err(_) => Err(invalid(
            "expected a decimal number such as 51.6461, .0001413 or -.31515E-4",
        )),
    }
}

fn optional_real(record: &RawRecord, keyword: &'static str) -> std::result::Result<f64, OmmError> {
    record
        .get(keyword)
        .map_or(Ok(0.0), |text| parse_real(keyword, text))
}

/// Parses a non-negative whole number (catalog number, revolution count, …).
fn parse_count(keyword: &'static str, text: &str) -> std::result::Result<u32, OmmError> {
    let body = strip_units(text);
    let digits = body.strip_prefix('+').unwrap_or(body);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(OmmError::InvalidValue {
            keyword,
            found: text.to_string(),
            reason: "expected a whole number written with digits only".into(),
        });
    }
    digits.parse::<u32>().map_err(|_| OmmError::InvalidValue {
        keyword,
        found: text.to_string(),
        reason: "the number is too large".into(),
    })
}

fn optional_count(record: &RawRecord, keyword: &'static str) -> std::result::Result<u32, OmmError> {
    record
        .get(keyword)
        .map_or(Ok(0), |text| parse_count(keyword, text))
}

/// Parses one of the three orientation angles, which a provider writes in `[0, 360)` degrees.
/// A full negative turn is tolerated for operator-generated messages.
fn parse_angle(
    record: &RawRecord,
    keyword: &'static str,
    why: &'static str,
) -> std::result::Result<f64, OmmError> {
    let value = parse_real(keyword, required(record, keyword, why)?)?;
    if !(-360.0..=360.0).contains(&value) {
        return Err(OmmError::OutOfRange {
            keyword,
            value,
            expected: "it must be an angle in degrees, between 0 and 360",
        });
    }
    Ok(value)
}

/// Parses an OMM `EPOCH`: ISO 8601 in UTC, as a calendar date (`2026-08-01T12:00:00.000000`)
/// or the CCSDS day-of-year form (`2026-213T12:00:00`). A trailing `Z` is accepted, and so is
/// a space in place of the `T`.
fn parse_epoch(text: &str) -> std::result::Result<DateTime<Utc>, OmmError> {
    let invalid = |reason: &str| OmmError::InvalidValue {
        keyword: "EPOCH",
        found: text.to_string(),
        reason: reason.to_string(),
    };
    let body = text.trim();
    let body = body
        .strip_suffix('Z')
        .or_else(|| body.strip_suffix('z'))
        .unwrap_or(body);
    let normalized = body.replacen(' ', "T", 1);

    let naive = NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(&normalized, "%Y-%jT%H:%M:%S%.f"))
        .map_err(|_| {
            invalid(
                "expected an ISO 8601 UTC timestamp, YYYY-MM-DDThh:mm:ss[.ffffff] or \
                 YYYY-DDDThh:mm:ss[.ffffff]",
            )
        })?;
    Ok(Utc.from_utc_datetime(&naive))
}

/// Expands a TLE international designator (`98067A`) to the OMM form (`1998-067A`).
/// Anything that is not two digits, three digits and a piece is passed through unchanged.
fn expand_designator(tle_designator: &str) -> Option<String> {
    let d = tle_designator.trim();
    if d.is_empty() {
        return None;
    }
    let bytes = d.as_bytes();
    if d.is_ascii() && bytes.len() >= 5 && bytes[..5].iter().all(u8::is_ascii_digit) {
        let yy: i32 = d[..2].parse().ok()?;
        let year = if yy < 57 { 2000 + yy } else { 1900 + yy };
        return Some(format!("{year}-{}{}", &d[2..5], d[5..].trim()));
    }
    Some(d.to_string())
}

/// Shortens text quoted in an error message so one bad line cannot flood the output.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    // The canonical ISS (ZARYA) element set used across the crate's TLE tests, expressed in
    // each OMM serialization. Every fixture must parse to the same `Omm`.
    const ISS_TLE: &str = "ISS (ZARYA)\n\
        1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
        2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

    /// CelesTrak-style JSON: an array, numeric values, redundant metadata left out.
    const ISS_JSON: &str = r#"[{
        "OBJECT_NAME": "ISS (ZARYA)",
        "OBJECT_ID": "1998-067A",
        "EPOCH": "2020-07-12T21:16:01.000416",
        "MEAN_MOTION": 15.49507896,
        "ECCENTRICITY": 0.0001413,
        "INCLINATION": 51.6461,
        "RA_OF_ASC_NODE": 221.2784,
        "ARG_OF_PERICENTER": 89.1723,
        "MEAN_ANOMALY": 280.4612,
        "EPHEMERIS_TYPE": 0,
        "CLASSIFICATION_TYPE": "U",
        "NORAD_CAT_ID": 25544,
        "ELEMENT_SET_NO": 999,
        "REV_AT_EPOCH": 23600,
        "BSTAR": -3.1515e-5,
        "MEAN_MOTION_DOT": -2.218e-5,
        "MEAN_MOTION_DDOT": 0
    }]"#;

    /// Space-Track-style JSON: every value a string, extra provider keys, nulls.
    const ISS_JSON_STRINGS: &str = r#"[{
        "CCSDS_OMM_VERS": "3.0",
        "COMMENT": "GENERATED VIA SPACE-TRACK.ORG API",
        "CREATION_DATE": "2020-07-13T02:16:10",
        "ORIGINATOR": "18 SPCS",
        "OBJECT_NAME": "ISS (ZARYA)",
        "OBJECT_ID": "1998-067A",
        "CENTER_NAME": "EARTH",
        "REF_FRAME": "TEME",
        "TIME_SYSTEM": "UTC",
        "MEAN_ELEMENT_THEORY": "SGP4",
        "EPOCH": "2020-07-12T21:16:01.000416",
        "MEAN_MOTION": "15.49507896",
        "ECCENTRICITY": "0.00014130",
        "INCLINATION": "51.6461",
        "RA_OF_ASC_NODE": "221.2784",
        "ARG_OF_PERICENTER": "89.1723",
        "MEAN_ANOMALY": "280.4612",
        "EPHEMERIS_TYPE": "0",
        "CLASSIFICATION_TYPE": "U",
        "NORAD_CAT_ID": "25544",
        "ELEMENT_SET_NO": "999",
        "REV_AT_EPOCH": "23600",
        "BSTAR": "-0.00003151500000",
        "MEAN_MOTION_DOT": "-0.00002218",
        "MEAN_MOTION_DDOT": "0.0000000000000",
        "SEMIMAJOR_AXIS": "6797.435",
        "PERIOD": "92.932",
        "DECAY_DATE": null,
        "TLE_LINE0": "0 ISS (ZARYA)"
    }]"#;

    /// CCSDS key-value notation with a header, comments and unit suffixes.
    const ISS_KVN: &str = "\
CCSDS_OMM_VERS = 2.0
COMMENT GENERATED FOR THE EPHEMERUST TEST SUITE
CREATION_DATE = 2020-07-13T02:16:10
ORIGINATOR = 18 SPCS

OBJECT_NAME = ISS (ZARYA)
OBJECT_ID = 1998-067A
CENTER_NAME = EARTH
REF_FRAME = TEME
TIME_SYSTEM = UTC
MEAN_ELEMENT_THEORY = SGP4

COMMENT mean elements
EPOCH = 2020-07-12T21:16:01.000416
MEAN_MOTION = 15.49507896 [rev/day]
ECCENTRICITY = .0001413
INCLINATION = 51.6461 [deg]
RA_OF_ASC_NODE = 221.2784 [deg]
ARG_OF_PERICENTER = 89.1723 [deg]
MEAN_ANOMALY = 280.4612 [deg]

EPHEMERIS_TYPE = 0
CLASSIFICATION_TYPE = U
NORAD_CAT_ID = 25544
ELEMENT_SET_NO = 999
REV_AT_EPOCH = 23600
BSTAR = -.31515E-4
MEAN_MOTION_DOT = -.2218E-4
MEAN_MOTION_DDOT = 0
";

    /// CCSDS XML inside an `<ndm>` wrapper, with unit attributes, a comment and empty
    /// header elements.
    const ISS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ndm xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<!-- one message per <omm> element -->
<omm id="CCSDS_OMM_VERS" version="2.0">
<header><CREATION_DATE/><ORIGINATOR>18 SPCS</ORIGINATOR></header>
<body><segment>
<metadata>
  <OBJECT_NAME>ISS (ZARYA)</OBJECT_NAME>
  <OBJECT_ID>1998-067A</OBJECT_ID>
  <CENTER_NAME>EARTH</CENTER_NAME>
  <REF_FRAME>TEME</REF_FRAME>
  <TIME_SYSTEM>UTC</TIME_SYSTEM>
  <MEAN_ELEMENT_THEORY>SGP4</MEAN_ELEMENT_THEORY>
</metadata>
<data>
  <meanElements>
    <EPOCH>2020-07-12T21:16:01.000416</EPOCH>
    <MEAN_MOTION units="rev/day">15.49507896</MEAN_MOTION>
    <ECCENTRICITY>.0001413</ECCENTRICITY>
    <INCLINATION units="deg">51.6461</INCLINATION>
    <RA_OF_ASC_NODE units="deg">221.2784</RA_OF_ASC_NODE>
    <ARG_OF_PERICENTER units="deg">89.1723</ARG_OF_PERICENTER>
    <MEAN_ANOMALY units="deg">280.4612</MEAN_ANOMALY>
  </meanElements>
  <tleParameters>
    <EPHEMERIS_TYPE>0</EPHEMERIS_TYPE>
    <CLASSIFICATION_TYPE>U</CLASSIFICATION_TYPE>
    <NORAD_CAT_ID>25544</NORAD_CAT_ID>
    <ELEMENT_SET_NO>999</ELEMENT_SET_NO>
    <REV_AT_EPOCH>23600</REV_AT_EPOCH>
    <BSTAR>-.31515E-4</BSTAR>
    <MEAN_MOTION_DOT>-.2218E-4</MEAN_MOTION_DOT>
    <MEAN_MOTION_DDOT>0</MEAN_MOTION_DDOT>
  </tleParameters>
</data>
</segment></body>
</omm>
</ndm>"#;

    const CSV_HEADER: &str = "OBJECT_NAME,OBJECT_ID,EPOCH,MEAN_MOTION,ECCENTRICITY,INCLINATION,\
RA_OF_ASC_NODE,ARG_OF_PERICENTER,MEAN_ANOMALY,EPHEMERIS_TYPE,CLASSIFICATION_TYPE,NORAD_CAT_ID,\
ELEMENT_SET_NO,REV_AT_EPOCH,BSTAR,MEAN_MOTION_DOT,MEAN_MOTION_DDOT";
    const ISS_CSV_ROW: &str = "ISS (ZARYA),1998-067A,2020-07-12T21:16:01.000416,15.49507896,\
.0001413,51.6461,221.2784,89.1723,280.4612,0,U,25544,999,23600,-.31515E-4,-.2218E-4,0";
    /// A second object, numbered above the TLE's five-digit limit.
    const HIGH_CSV_ROW: &str = "EXAMPLE-SAT,2026-150A,2026-08-01T12:00:00.000000,15.2,.0012,\
97.5,120.0,45.0,315.0,0,U,100123,999,42,.1E-3,.1E-4,0";

    fn iss_csv() -> String {
        format!("{CSV_HEADER}\n{ISS_CSV_ROW}\n")
    }

    fn two_object_csv() -> String {
        format!("{CSV_HEADER}\n{ISS_CSV_ROW}\n{HIGH_CSV_ROW}\n")
    }

    /// Replaces one `KEYWORD = value` line of the KVN fixture (or appends it).
    fn kvn_with(keyword: &str, value: &str) -> String {
        let mut found = false;
        let mut out: Vec<String> = ISS_KVN
            .lines()
            .map(|l| {
                if l.split('=').next().map(str::trim) == Some(keyword) {
                    found = true;
                    format!("{keyword} = {value}")
                } else {
                    l.to_string()
                }
            })
            .collect();
        if !found {
            out.push(format!("{keyword} = {value}"));
        }
        out.join("\n")
    }

    /// Removes one keyword line from the KVN fixture.
    fn kvn_without(keyword: &str) -> String {
        ISS_KVN
            .lines()
            .filter(|l| l.split('=').next().map(str::trim) != Some(keyword))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn omm_error(result: Result<Omm>) -> OmmError {
        match result {
            Err(AstroError::Omm(e)) => e,
            other => panic!("expected an OMM error, got {other:?}"),
        }
    }

    // --- format detection ---------------------------------------------------------------

    #[test]
    fn detects_every_serialization() {
        assert_eq!(OmmFormat::detect(ISS_JSON), Some(OmmFormat::Json));
        assert_eq!(OmmFormat::detect(ISS_JSON_STRINGS), Some(OmmFormat::Json));
        assert_eq!(OmmFormat::detect(ISS_KVN), Some(OmmFormat::Kvn));
        assert_eq!(OmmFormat::detect(ISS_XML), Some(OmmFormat::Xml));
        assert_eq!(OmmFormat::detect(&iss_csv()), Some(OmmFormat::Csv));
    }

    #[test]
    fn detection_tolerates_a_byte_order_mark_and_leading_blank_lines() {
        assert_eq!(
            OmmFormat::detect(&format!("\u{feff}\n\n{ISS_JSON}")),
            Some(OmmFormat::Json)
        );
        assert_eq!(
            OmmFormat::detect(&format!("\n  \n{ISS_KVN}")),
            Some(OmmFormat::Kvn)
        );
        assert!(Omm::parse(&format!("\u{feff}{ISS_KVN}")).is_ok());
    }

    #[test]
    fn a_tle_and_arbitrary_text_are_not_detected_as_omm() {
        assert_eq!(OmmFormat::detect(ISS_TLE), None);
        assert_eq!(OmmFormat::detect(""), None);
        assert_eq!(OmmFormat::detect("   \n"), None);
        assert_eq!(OmmFormat::detect("No GP data found"), None);
        // A comma-separated line that is not an OMM header.
        assert_eq!(OmmFormat::detect("name,value\nfoo,1\n"), None);
        assert_eq!(
            omm_error(Omm::parse("No GP data found")),
            OmmError::UnrecognizedFormat
        );
    }

    // --- the four serializations agree --------------------------------------------------

    #[test]
    fn parses_all_iss_fields_from_json() {
        let omm = Omm::parse(ISS_JSON).expect("fixture should parse");
        assert_eq!(omm.name.as_deref(), Some("ISS (ZARYA)"));
        assert_eq!(omm.international_designator.as_deref(), Some("1998-067A"));
        assert_eq!(omm.catalog_number, Some(25544));
        assert_eq!(omm.classification, 'U');
        assert_eq!(omm.ephemeris_type, 0);
        assert_eq!(omm.element_set_number, 999);
        assert_eq!(omm.revolution_number, 23600);
        assert_eq!(omm.inclination_deg, 51.6461);
        assert_eq!(omm.raan_deg, 221.2784);
        assert_eq!(omm.eccentricity, 0.0001413);
        assert_eq!(omm.arg_perigee_deg, 89.1723);
        assert_eq!(omm.mean_anomaly_deg, 280.4612);
        assert_eq!(omm.mean_motion, 15.49507896);
        assert_eq!(omm.bstar, -0.31515e-4);
        assert_eq!(omm.mean_motion_dot, -0.2218e-4);
        assert_eq!(omm.mean_motion_ddot, 0.0);

        assert_eq!(
            (omm.epoch.year(), omm.epoch.month(), omm.epoch.day()),
            (2020, 7, 12)
        );
        assert_eq!(
            (omm.epoch.hour(), omm.epoch.minute(), omm.epoch.second()),
            (21, 16, 1)
        );
        assert_eq!(omm.epoch.nanosecond(), 416_000);
    }

    #[test]
    fn every_serialization_yields_the_same_omm() {
        let reference = Omm::parse(ISS_JSON).expect("JSON fixture");
        for (label, text) in [
            ("JSON with string values", ISS_JSON_STRINGS.to_string()),
            ("KVN", ISS_KVN.to_string()),
            ("XML", ISS_XML.to_string()),
            ("CSV", iss_csv()),
        ] {
            let omm = Omm::parse(&text).unwrap_or_else(|e| panic!("{label} fixture: {e}"));
            assert_eq!(omm, reference, "{label} disagrees with JSON");
        }
    }

    #[test]
    fn omm_matches_the_tle_it_was_published_with() {
        let tle = Tle::parse(ISS_TLE).expect("reference TLE");
        let from_tle = Omm::from(&tle);
        let parsed = Omm::parse(ISS_KVN).expect("KVN fixture");

        // The epoch is the one field the two formats encode differently: a day-of-year
        // fraction with eight decimals (0.864 ms steps) against a microsecond timestamp.
        let delta = (parsed.epoch - from_tle.epoch)
            .num_microseconds()
            .expect("small difference");
        assert!(delta.abs() < 1, "epochs differ by {delta} µs");

        let mut aligned = parsed.clone();
        aligned.epoch = from_tle.epoch;
        assert_eq!(aligned, from_tle);
    }

    #[test]
    fn from_str_matches_parse() {
        let a: Omm = ISS_XML.parse().expect("FromStr");
        assert_eq!(a, Omm::parse(ISS_XML).unwrap());
    }

    #[test]
    fn from_file_reads_a_single_message() {
        let path = std::env::temp_dir().join(format!(
            "ephemerust-omm-test-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, ISS_JSON).expect("write fixture");
        let omm = Omm::from_file(path.to_str().expect("utf-8 path"));
        let _ = std::fs::remove_file(&path);
        assert_eq!(omm.expect("read back").catalog_number, Some(25544));

        assert!(matches!(
            Omm::from_file("/nonexistent/ephemerust/omm.json"),
            Err(AstroError::IoError(_))
        ));
    }

    // --- catalog numbers beyond the TLE ---------------------------------------------------

    #[test]
    fn six_and_nine_digit_catalog_numbers_are_read() {
        let six = Omm::parse(&kvn_with("NORAD_CAT_ID", "100000")).unwrap();
        assert_eq!(six.catalog_number, Some(100_000));

        let nine = Omm::parse(&kvn_with("NORAD_CAT_ID", "799500123")).unwrap();
        assert_eq!(nine.catalog_number, Some(799_500_123));

        // Leading zeros, as some providers pad the field.
        let padded = Omm::parse(&kvn_with("NORAD_CAT_ID", "000025544")).unwrap();
        assert_eq!(padded.catalog_number, Some(25544));
    }

    #[test]
    fn ten_digit_catalog_number_is_out_of_range() {
        let err = omm_error(Omm::parse(&kvn_with("NORAD_CAT_ID", "1000000000")));
        assert!(
            matches!(
                err,
                OmmError::OutOfRange {
                    keyword: "NORAD_CAT_ID",
                    ..
                }
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn catalog_number_is_optional() {
        let omm = Omm::parse(&kvn_without("NORAD_CAT_ID")).unwrap();
        assert_eq!(omm.catalog_number, None);
    }

    // --- required and optional keywords ---------------------------------------------------

    #[test]
    fn each_required_keyword_is_reported_when_missing() {
        for keyword in [
            "EPOCH",
            "MEAN_MOTION",
            "ECCENTRICITY",
            "INCLINATION",
            "RA_OF_ASC_NODE",
            "ARG_OF_PERICENTER",
            "MEAN_ANOMALY",
            "BSTAR",
        ] {
            let err = omm_error(Omm::parse(&kvn_without(keyword)));
            assert!(
                matches!(&err, OmmError::MissingKeyword { keyword: k, .. } if *k == keyword),
                "{keyword}: unexpected error {err:?}"
            );
            let msg = err.to_string();
            assert!(
                msg.contains(keyword),
                "message should name {keyword}: {msg}"
            );
            assert!(err.hint().is_some());
        }
    }

    #[test]
    fn an_empty_value_counts_as_missing() {
        let err = omm_error(Omm::parse(&kvn_with("BSTAR", "")));
        assert!(matches!(
            err,
            OmmError::MissingKeyword {
                keyword: "BSTAR",
                ..
            }
        ));
    }

    #[test]
    fn optional_keywords_take_their_documented_defaults() {
        let mut text = ISS_KVN.to_string();
        for keyword in [
            "OBJECT_NAME",
            "OBJECT_ID",
            "CENTER_NAME",
            "REF_FRAME",
            "TIME_SYSTEM",
            "MEAN_ELEMENT_THEORY",
            "EPHEMERIS_TYPE",
            "CLASSIFICATION_TYPE",
            "NORAD_CAT_ID",
            "ELEMENT_SET_NO",
            "REV_AT_EPOCH",
            "MEAN_MOTION_DOT",
            "MEAN_MOTION_DDOT",
        ] {
            text = text
                .lines()
                .filter(|l| l.split('=').next().map(str::trim) != Some(keyword))
                .collect::<Vec<_>>()
                .join("\n");
        }
        let omm = Omm::parse(&text).expect("the eight required keywords are enough");
        assert_eq!(omm.name, None);
        assert_eq!(omm.international_designator, None);
        assert_eq!(omm.catalog_number, None);
        assert_eq!(omm.classification, 'U');
        assert_eq!(omm.ephemeris_type, 0);
        assert_eq!(omm.element_set_number, 0);
        assert_eq!(omm.revolution_number, 0);
        assert_eq!(omm.mean_motion_dot, 0.0);
        assert_eq!(omm.mean_motion_ddot, 0.0);
        assert_eq!(omm.mean_motion, 15.49507896);
    }

    #[test]
    fn semi_major_axis_without_mean_motion_is_explained() {
        let text = format!("{}\nSEMI_MAJOR_AXIS = 6797.4", kvn_without("MEAN_MOTION"));
        let err = omm_error(Omm::parse(&text));
        assert!(
            matches!(
                err,
                OmmError::Unsupported {
                    keyword: "SEMI_MAJOR_AXIS",
                    ..
                }
            ),
            "unexpected error: {err:?}"
        );
    }

    // --- value validation -----------------------------------------------------------------

    #[test]
    fn out_of_range_elements_are_rejected() {
        for (keyword, value) in [
            ("ECCENTRICITY", "1.0"),
            ("ECCENTRICITY", "-0.1"),
            // The TLE's implied-decimal digits pasted into an OMM: 1413, not 0.0001413.
            ("ECCENTRICITY", "0001413"),
            ("INCLINATION", "180.5"),
            ("INCLINATION", "-1"),
            ("MEAN_MOTION", "0"),
            ("MEAN_MOTION", "-15.5"),
            ("RA_OF_ASC_NODE", "400"),
            ("ARG_OF_PERICENTER", "-361"),
            ("MEAN_ANOMALY", "1e9"),
        ] {
            let err = omm_error(Omm::parse(&kvn_with(keyword, value)));
            assert!(
                matches!(&err, OmmError::OutOfRange { keyword: k, .. } if *k == keyword),
                "{keyword} = {value}: unexpected error {err:?}"
            );
        }
    }

    #[test]
    fn boundary_values_are_accepted() {
        assert!(Omm::parse(&kvn_with("ECCENTRICITY", "0")).is_ok());
        assert!(Omm::parse(&kvn_with("INCLINATION", "0")).is_ok());
        assert!(Omm::parse(&kvn_with("INCLINATION", "180")).is_ok());
        assert!(Omm::parse(&kvn_with("MEAN_ANOMALY", "360")).is_ok());
        assert!(Omm::parse(&kvn_with("RA_OF_ASC_NODE", "0.0000")).is_ok());
    }

    #[test]
    fn non_numeric_and_non_finite_numbers_are_rejected() {
        for value in [
            "abc", "nan", "NaN", "inf", "-inf", "infinity", "1.2.3", "--5", "0x10", "1e999",
            "51,6461",
        ] {
            let err = omm_error(Omm::parse(&kvn_with("INCLINATION", value)));
            assert!(
                matches!(
                    &err,
                    OmmError::InvalidValue {
                        keyword: "INCLINATION",
                        ..
                    }
                ),
                "INCLINATION = {value}: unexpected error {err:?}"
            );
        }
        for value in ["-1", "25544.0", "2.5e4", "A0001", "99999999999"] {
            let err = omm_error(Omm::parse(&kvn_with("NORAD_CAT_ID", value)));
            assert!(
                matches!(
                    &err,
                    OmmError::InvalidValue {
                        keyword: "NORAD_CAT_ID",
                        ..
                    }
                ),
                "NORAD_CAT_ID = {value}: unexpected error {err:?}"
            );
        }
    }

    #[test]
    fn number_forms_emitted_by_providers_are_accepted() {
        for (value, expected) in [
            (".0001413", 0.0001413),
            ("0.0001413", 0.0001413),
            ("+0.0001413", 0.0001413),
            ("1.413E-4", 0.0001413),
            ("1.413e-04", 0.0001413),
            ("0.00014130", 0.0001413),
        ] {
            let omm = Omm::parse(&kvn_with("ECCENTRICITY", value))
                .unwrap_or_else(|e| panic!("ECCENTRICITY = {value}: {e}"));
            assert_eq!(omm.eccentricity, expected, "ECCENTRICITY = {value}");
        }
    }

    #[test]
    fn epoch_forms() {
        let reference = Omm::parse(ISS_KVN).unwrap().epoch;
        for value in [
            "2020-07-12T21:16:01.000416Z",
            "2020-07-12 21:16:01.000416",
            // CCSDS day-of-year form: 12 July 2020 is day 194.
            "2020-194T21:16:01.000416",
            "2020-194T21:16:01.000416Z",
        ] {
            let omm =
                Omm::parse(&kvn_with("EPOCH", value)).unwrap_or_else(|e| panic!("{value}: {e}"));
            assert_eq!(omm.epoch, reference, "EPOCH = {value}");
        }

        // Whole seconds, and nanosecond precision.
        let whole = Omm::parse(&kvn_with("EPOCH", "2026-08-01T12:00:00")).unwrap();
        assert_eq!(whole.epoch.nanosecond(), 0);
        let fine = Omm::parse(&kvn_with("EPOCH", "2026-08-01T12:00:00.123456789")).unwrap();
        assert_eq!(fine.epoch.nanosecond(), 123_456_789);
    }

    #[test]
    fn malformed_epochs_are_rejected() {
        for value in [
            "20194.88612269",            // the TLE epoch form
            "2020-07-12",                // date only
            "2020-13-01T00:00:00",       // month 13
            "2020-02-30T00:00:00",       // not a calendar date
            "2021-366T00:00:00",         // 2021 has 365 days
            "2020-07-12T25:00:00",       // hour 25
            "2020-07-12T21:16:01+02:00", // a UTC offset: the OMM time system is UTC
            "yesterday",
        ] {
            let err = omm_error(Omm::parse(&kvn_with("EPOCH", value)));
            assert!(
                matches!(
                    &err,
                    OmmError::InvalidValue {
                        keyword: "EPOCH",
                        ..
                    }
                ),
                "EPOCH = {value}: unexpected error {err:?}"
            );
        }
    }

    #[test]
    fn metadata_for_another_model_is_rejected() {
        for (keyword, value) in [
            ("CENTER_NAME", "MARS"),
            ("REF_FRAME", "GCRF"),
            ("REF_FRAME", "EME2000"),
            ("TIME_SYSTEM", "GPS"),
            ("TIME_SYSTEM", "TAI"),
            ("MEAN_ELEMENT_THEORY", "DSST"),
            ("MEAN_ELEMENT_THEORY", "SGP4-XP"),
            ("EPHEMERIS_TYPE", "4"),
        ] {
            let err = omm_error(Omm::parse(&kvn_with(keyword, value)));
            assert!(
                matches!(&err, OmmError::Unsupported { keyword: k, .. } if *k == keyword),
                "{keyword} = {value}: unexpected error {err:?}"
            );
            assert!(err.to_string().contains(value));
        }
    }

    #[test]
    fn accepted_metadata_spellings() {
        assert!(Omm::parse(&kvn_with("MEAN_ELEMENT_THEORY", "SGP/SGP4")).is_ok());
        assert!(Omm::parse(&kvn_with("MEAN_ELEMENT_THEORY", "sgp4")).is_ok());
        assert!(Omm::parse(&kvn_with("CENTER_NAME", "Earth")).is_ok());
        assert_eq!(
            Omm::parse(&kvn_with("EPHEMERIS_TYPE", "2"))
                .unwrap()
                .ephemeris_type,
            2
        );
        assert_eq!(
            Omm::parse(&kvn_with("CLASSIFICATION_TYPE", "c"))
                .unwrap()
                .classification,
            'C'
        );
    }

    #[test]
    fn unknown_classification_is_rejected() {
        let err = omm_error(Omm::parse(&kvn_with("CLASSIFICATION_TYPE", "X")));
        assert!(matches!(
            err,
            OmmError::InvalidValue {
                keyword: "CLASSIFICATION_TYPE",
                ..
            }
        ));
    }

    // --- several messages -----------------------------------------------------------------

    #[test]
    fn parse_rejects_several_messages_and_points_at_the_alternatives() {
        let err = omm_error(Omm::parse(&two_object_csv()));
        assert_eq!(err, OmmError::WrongMessageCount { found: 2 });
        let hint = err.hint().expect("hint");
        assert!(hint.contains("parse_all") && hint.contains("select_omm"));
    }

    #[test]
    fn parse_all_reads_every_message_in_order() {
        let all = Omm::parse_all(&two_object_csv()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].catalog_number, Some(25544));
        assert_eq!(all[1].catalog_number, Some(100_123));
        assert_eq!(all[1].name.as_deref(), Some("EXAMPLE-SAT"));
        assert_eq!(all[1].bstar, 1e-4);

        assert_eq!(Omm::parse_all(ISS_XML).unwrap().len(), 1);
    }

    #[test]
    fn parse_all_names_the_message_that_failed() {
        let bad_row = HIGH_CSV_ROW.replace(",.0012,", ",1.5,");
        let text = format!("{CSV_HEADER}\n{ISS_CSV_ROW}\n{bad_row}\n");
        let err = omm_error(Omm::parse_all(&text).map(|mut v| v.remove(0)));
        match &err {
            OmmError::InMessage {
                index,
                object,
                source,
            } => {
                assert_eq!(*index, 2);
                assert_eq!(object, "EXAMPLE-SAT");
                assert!(matches!(
                    **source,
                    OmmError::OutOfRange {
                        keyword: "ECCENTRICITY",
                        ..
                    }
                ));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        let msg = err.to_string();
        assert!(msg.contains("message 2") && msg.contains("EXAMPLE-SAT"));
        // The hint is the underlying error's hint.
        assert!(err.hint().is_some());
    }

    #[test]
    fn parse_all_does_not_wrap_a_single_message_error() {
        let err =
            omm_error(Omm::parse_all(&kvn_with("ECCENTRICITY", "2")).map(|mut v| v.remove(0)));
        assert!(matches!(err, OmmError::OutOfRange { .. }));
    }

    #[test]
    fn select_by_name_and_by_catalog_number() {
        let text = two_object_csv();
        assert_eq!(
            select_omm(&text, Some("zarya")).unwrap().catalog_number,
            Some(25544)
        );
        assert_eq!(
            select_omm(&text, Some("100123")).unwrap().name.as_deref(),
            Some("EXAMPLE-SAT")
        );
        // Leading zeros in the selector are tolerated.
        assert_eq!(
            select_omm(&text, Some("025544")).unwrap().catalog_number,
            Some(25544)
        );
        // A numeric selector matches the whole number, not a substring of it.
        assert!(select_omm(&text, Some("2554")).is_err());
    }

    #[test]
    fn select_reports_ambiguity_no_match_and_missing_selector() {
        let text = two_object_csv();

        let err = select_omm(&text, None).unwrap_err().to_string();
        assert!(err.contains("2 OMM messages"), "{err}");
        assert!(
            err.contains("ISS (ZARYA)") && err.contains("EXAMPLE-SAT"),
            "{err}"
        );

        // "s" appears in both names.
        let err = select_omm(&text, Some("s")).unwrap_err().to_string();
        assert!(err.contains("matches 2"), "{err}");

        let err = select_omm(&text, Some("hubble")).unwrap_err().to_string();
        assert!(err.contains("no OMM message matches"), "{err}");
        assert!(err.contains("available"), "{err}");
    }

    #[test]
    fn select_without_selector_accepts_a_single_message() {
        assert_eq!(
            select_omm(ISS_KVN, None).unwrap().catalog_number,
            Some(25544)
        );
    }

    #[test]
    fn select_only_validates_the_selected_message() {
        let bad_row = HIGH_CSV_ROW.replace(",.0012,", ",1.5,");
        let text = format!("{CSV_HEADER}\n{ISS_CSV_ROW}\n{bad_row}\n");
        assert!(select_omm(&text, Some("zarya")).is_ok());
        assert!(matches!(
            select_omm(&text, Some("example")),
            Err(AstroError::Omm(OmmError::OutOfRange { .. }))
        ));
    }

    // --- JSON specifics -------------------------------------------------------------------

    #[test]
    fn json_single_object_and_array_are_equivalent() {
        let object = ISS_JSON
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']');
        assert_eq!(Omm::parse(object).unwrap(), Omm::parse(ISS_JSON).unwrap());
    }

    #[test]
    fn json_errors() {
        assert_eq!(
            omm_error(Omm::parse("[]")),
            OmmError::NoMessages {
                format: OmmFormat::Json
            }
        );
        assert!(matches!(
            omm_error(Omm::parse("[{\"EPOCH\": ")),
            OmmError::Syntax {
                format: OmmFormat::Json,
                ..
            }
        ));
        assert!(matches!(
            omm_error(Omm::parse("[1, 2, 3]")),
            OmmError::Syntax {
                format: OmmFormat::Json,
                ..
            }
        ));
        // A null where a required number belongs is "missing", not a type error.
        let with_null = ISS_JSON.replace("\"BSTAR\": -3.1515e-5", "\"BSTAR\": null");
        assert!(matches!(
            omm_error(Omm::parse(&with_null)),
            OmmError::MissingKeyword {
                keyword: "BSTAR",
                ..
            }
        ));
    }

    // --- KVN specifics --------------------------------------------------------------------

    #[test]
    fn kvn_messages_are_split_at_version_lines() {
        let second = kvn_with("NORAD_CAT_ID", "100123").replace("ISS (ZARYA)", "EXAMPLE-SAT");
        let all = Omm::parse_all(&format!("{ISS_KVN}\n{second}\n")).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name.as_deref(), Some("ISS (ZARYA)"));
        assert_eq!(all[1].catalog_number, Some(100_123));
    }

    #[test]
    fn kvn_messages_without_version_lines_are_split_where_a_keyword_repeats() {
        let first = kvn_without("CCSDS_OMM_VERS");
        let second = first
            .replace("ISS (ZARYA)", "EXAMPLE-SAT")
            .replace("NORAD_CAT_ID = 25544", "NORAD_CAT_ID = 100123");
        let all = Omm::parse_all(&format!("{first}\n{second}\n")).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].name.as_deref(), Some("EXAMPLE-SAT"));
    }

    #[test]
    fn kvn_tolerates_windows_line_endings_and_loose_spacing() {
        let text = ISS_KVN.replace('\n', "\r\n").replace(" = ", "=");
        assert_eq!(Omm::parse(&text).unwrap(), Omm::parse(ISS_KVN).unwrap());
    }

    #[test]
    fn kvn_malformed_line_is_reported_with_its_line_number() {
        let text = "CCSDS_OMM_VERS = 2.0\nOBJECT_NAME = X\nthis line has no separator\n";
        match omm_error(Omm::parse(text)) {
            OmmError::Syntax {
                format: OmmFormat::Kvn,
                detail,
            } => assert!(detail.contains("line 3"), "{detail}"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn kvn_header_alone_is_no_message() {
        assert_eq!(
            omm_error(Omm::parse("CCSDS_OMM_VERS = 2.0\nCOMMENT nothing here\n")),
            OmmError::NoMessages {
                format: OmmFormat::Kvn
            }
        );
    }

    #[test]
    fn kvn_object_name_keeps_its_brackets() {
        // Unit suffixes are stripped from numbers only; a name may end in brackets.
        let omm = Omm::parse(&kvn_with("OBJECT_NAME", "DEBRIS [TEST]")).unwrap();
        assert_eq!(omm.name.as_deref(), Some("DEBRIS [TEST]"));
    }

    // --- XML specifics --------------------------------------------------------------------

    #[test]
    fn xml_several_messages_namespaces_and_entities() {
        let second = ISS_XML
            .replace("<ndm ", "<ndm:ndm ")
            .replace("</ndm>", "</ndm:ndm>")
            .replace("<omm ", "<ndm:omm ")
            .replace("</omm>", "</ndm:omm>")
            .replace("ISS (ZARYA)", "R&amp;D SAT &#x41;&#66; &lt;1&gt;")
            .replace("<NORAD_CAT_ID>25544", "<NORAD_CAT_ID>100123");
        let omm = Omm::parse(&second).unwrap();
        assert_eq!(omm.name.as_deref(), Some("R&D SAT AB <1>"));
        assert_eq!(omm.catalog_number, Some(100_123));

        // Two <omm> elements in one <ndm>.
        let body_start = ISS_XML.find("<omm ").unwrap();
        let body_end = ISS_XML.find("</omm>").unwrap() + "</omm>".len();
        let message = &ISS_XML[body_start..body_end];
        let two = format!(
            "<?xml version=\"1.0\"?>\n<ndm>\n{message}\n{}\n</ndm>",
            message.replace("25544", "100123")
        );
        let all = Omm::parse_all(&two).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].catalog_number, Some(100_123));
    }

    #[test]
    fn xml_compact_single_line_document() {
        let compact: String = ISS_XML.lines().map(str::trim).collect::<Vec<_>>().join("");
        assert_eq!(Omm::parse(&compact).unwrap(), Omm::parse(ISS_XML).unwrap());
    }

    #[test]
    fn xml_errors() {
        // Truncated mid-message.
        let cut = &ISS_XML[..ISS_XML.find("<tleParameters>").unwrap()];
        assert!(matches!(
            omm_error(Omm::parse(cut)),
            OmmError::Syntax {
                format: OmmFormat::Xml,
                ..
            }
        ));
        // Mismatched closing tag.
        let mismatched = ISS_XML.replace("</EPOCH>", "</EPOCHS>");
        assert!(matches!(
            omm_error(Omm::parse(&mismatched)),
            OmmError::Syntax {
                format: OmmFormat::Xml,
                ..
            }
        ));
        // Unknown entity.
        let entity = ISS_XML.replace("ISS (ZARYA)", "ISS &nbsp; ZARYA");
        assert!(matches!(
            omm_error(Omm::parse(&entity)),
            OmmError::Syntax {
                format: OmmFormat::Xml,
                ..
            }
        ));
        // Well-formed XML with no <omm> element (an HTML error page, for example).
        assert_eq!(
            omm_error(Omm::parse("<html><body><p>Not found</p></body></html>")),
            OmmError::NoMessages {
                format: OmmFormat::Xml
            }
        );
        // Unterminated comment.
        assert!(matches!(
            omm_error(Omm::parse("<ndm><!-- never closed <omm></omm></ndm>")),
            OmmError::Syntax {
                format: OmmFormat::Xml,
                ..
            }
        ));
    }

    #[test]
    fn xml_commented_out_values_are_ignored() {
        let text = ISS_XML.replace(
            "<NORAD_CAT_ID>25544</NORAD_CAT_ID>",
            "<!-- <NORAD_CAT_ID>1</NORAD_CAT_ID> --><NORAD_CAT_ID>25544</NORAD_CAT_ID>",
        );
        assert_eq!(Omm::parse(&text).unwrap().catalog_number, Some(25544));
    }

    // --- CSV specifics --------------------------------------------------------------------

    #[test]
    fn csv_quoted_fields_and_windows_line_endings() {
        let row = ISS_CSV_ROW.replace("ISS (ZARYA)", "\"SAT, WITH \"\"QUOTES\"\"\"");
        let text = format!("{CSV_HEADER}\r\n{row}\r\n");
        let omm = Omm::parse(&text).unwrap();
        assert_eq!(omm.name.as_deref(), Some("SAT, WITH \"QUOTES\""));
        assert_eq!(omm.mean_motion_ddot, 0.0);
    }

    #[test]
    fn csv_column_order_does_not_matter() {
        let text = "EPOCH,NORAD_CAT_ID,BSTAR,MEAN_ANOMALY,ARG_OF_PERICENTER,RA_OF_ASC_NODE,\
INCLINATION,ECCENTRICITY,MEAN_MOTION\n\
2020-07-12T21:16:01.000416,25544,-.31515E-4,280.4612,89.1723,221.2784,51.6461,.0001413,\
15.49507896\n";
        let omm = Omm::parse(text).unwrap();
        assert_eq!(omm.catalog_number, Some(25544));
        assert_eq!(omm.inclination_deg, 51.6461);
    }

    #[test]
    fn csv_errors() {
        // A row with the wrong number of fields.
        let text = format!("{CSV_HEADER}\n{ISS_CSV_ROW},EXTRA\n");
        match omm_error(Omm::parse(&text)) {
            OmmError::Syntax {
                format: OmmFormat::Csv,
                detail,
            } => assert!(detail.contains("line 2"), "{detail}"),
            other => panic!("unexpected error: {other:?}"),
        }
        // Header only.
        assert_eq!(
            omm_error(Omm::parse(&format!("{CSV_HEADER}\n"))),
            OmmError::NoMessages {
                format: OmmFormat::Csv
            }
        );
        // Unterminated quote.
        let text = format!("{CSV_HEADER}\n\"{ISS_CSV_ROW}\n");
        assert!(matches!(
            omm_error(Omm::parse(&text)),
            OmmError::Syntax {
                format: OmmFormat::Csv,
                ..
            }
        ));
    }

    // --- TLE conversion -------------------------------------------------------------------

    #[test]
    fn international_designator_expansion() {
        assert_eq!(expand_designator("98067A").as_deref(), Some("1998-067A"));
        assert_eq!(expand_designator("57001A").as_deref(), Some("1957-001A"));
        assert_eq!(expand_designator("56001A").as_deref(), Some("2056-001A"));
        assert_eq!(expand_designator("26150AB").as_deref(), Some("2026-150AB"));
        assert_eq!(expand_designator("00001A  ").as_deref(), Some("2000-001A"));
        assert_eq!(expand_designator(""), None);
        assert_eq!(expand_designator("   "), None);
        // Not the TLE shape: passed through untouched.
        assert_eq!(expand_designator("1998-067A").as_deref(), Some("1998-067A"));
        assert_eq!(expand_designator("ABC").as_deref(), Some("ABC"));
    }

    // --- error presentation ---------------------------------------------------------------

    #[test]
    fn every_error_has_a_hint_and_flows_through_astro_error() {
        let errors = [
            OmmError::UnrecognizedFormat,
            OmmError::Syntax {
                format: OmmFormat::Json,
                detail: "x".into(),
            },
            OmmError::NoMessages {
                format: OmmFormat::Csv,
            },
            OmmError::WrongMessageCount { found: 3 },
            OmmError::MissingKeyword {
                keyword: "EPOCH",
                why: "x",
            },
            OmmError::InvalidValue {
                keyword: "EPOCH",
                found: "x".into(),
                reason: "y".into(),
            },
            OmmError::OutOfRange {
                keyword: "ECCENTRICITY",
                value: 2.0,
                expected: "x",
            },
            OmmError::Unsupported {
                keyword: "REF_FRAME",
                found: "GCRF".into(),
                reason: "x",
            },
        ];
        for e in errors {
            assert!(e.hint().is_some(), "{e:?} has no hint");
            let astro: AstroError = e.clone().into();
            assert_eq!(astro.hint(), e.hint());
            assert_eq!(astro.to_string(), e.to_string());
        }
    }

    #[test]
    fn long_garbage_is_truncated_in_messages() {
        let text = format!("CCSDS_OMM_VERS = 2.0\n{}\n", "x".repeat(5_000));
        let msg = omm_error(Omm::parse(&text)).to_string();
        assert!(msg.len() < 400, "message is {} bytes", msg.len());
    }
}
