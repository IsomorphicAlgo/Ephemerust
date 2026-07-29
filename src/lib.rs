#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
//! # Ephemerust
//!
//! An accessible, **teaching-grade** astronomy, orbital-mechanics, and satellite-tracking
//! library for Rust — in spirit, the Rust counterpart to Python's
//! [Skyfield](https://rhodesmill.org/skyfield/). It deliberately occupies the middle ground
//! between raw numerical engines (such as the [`sgp4`](https://crates.io/crates/sgp4) crate)
//! and ultra-high-fidelity mission toolkits (such as `nyx-space`): it wraps the messy parts —
//! time systems, coordinate-frame conversions, and planetary theory — behind an ergonomic,
//! thoroughly documented API.
//!
//! ## Design philosophy
//!
//! - **Don't reinvent the wheel.** Heavy numerical work is delegated to established crates
//!   (the `sgp4` propagator, `chrono` for time); Ephemerust supplies the conversion and
//!   convenience layer around them.
//! - **Document the physics, not just the code.** Each public item explains the physical
//!   reasoning and the conventions (frames, units, epochs) it assumes.
//! - **Errors are teaching moments.** Failures explain *what* was expected and *why*, so the
//!   library is instructive even when input is wrong (see [`TleError`]).
//!
//! ## Module map
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`time`] | Julian Date, Greenwich/Local sidereal time |
//! | [`coordinates`] | RA/Dec ↔ Alt/Az, ECEF ↔ ECI, and WGS84 ECEF ↔ geodetic |
//! | [`celestial`] | Sun/Moon position and rise/set; dispatch to planets |
//! | [`eclipse`] | Earth-shadow geometry: sunlit / penumbra / umbra classification and entry/exit search |
//! | [`orbital`] | Kepler's equation, orbital period, elements → state vectors |
//! | [`planets`] | VSOP87 planetary ephemeris |
//! | [`satellite`] | TLE / SGP4 propagation, TEME → ECEF → geodetic, look angles, passes, ground track |
//! | [`sgp4_teaching`] | Educational two-body / Kepler scaffolding vs the real `sgp4` model (`docs/sgp4.md`) |
//!
//! ## Conventions
//!
//! Angles are in degrees and times in UTC unless a function states otherwise; right ascension
//! and sidereal time are in hours; distances follow each module's stated unit (kilometres for
//! orbital/satellite state, metres for ECEF/ECI). No precession or nutation is applied, which
//! bounds accuracy at roughly the arcminute level — see `docs/accuracy-and-limits.md`.
//!
//! ## API stability, MSRV, and Cargo features
//!
//! The project is in the **`0.x`** semver range: minor releases may include breaking API
//! changes (see [`CHANGELOG.md`](https://github.com/IsomorphicAlgo/ephemerust/blob/master/CHANGELOG.md)).
//! The **minimum supported Rust version (MSRV)** is declared in the crate `Cargo.toml`
//! (`package.rust-version`) and summarized in the top-level readme.
//!
//! Optional Cargo features:
//!
//! | Feature | Purpose |
//! |---------|---------|
//! | *(none by default)* | Default build: no HTTP client or TLS dependencies. |
//! | `network` | Enables the `net` module (bounded HTTPS fetch of CelesTrak-style element-set
//!   bulletins via `ureq`/rustls) and the `track` CLI flag `--tle-url`. See `http_plan.md`
//!   for the operational requirements this implementation follows. |
//!
//! ## Example
//!
//! Compute the Julian Date and Greenwich Mean Sidereal Time of the J2000.0 epoch:
//!
//! ```
//! use chrono::{TimeZone, Utc};
//! use ephemerust::{julian_date, greenwich_mean_sidereal_time};
//!
//! // J2000.0 is defined as 2000-01-01 12:00:00 UTC.
//! let epoch = Utc.with_ymd_and_hms(2000, 1, 1, 12, 0, 0).unwrap();
//! let jd = julian_date(epoch);
//! assert!((jd - 2451545.0).abs() < 1e-6);
//!
//! // GMST is the Earth's rotation angle expressed in hours of right ascension.
//! let gmst = greenwich_mean_sidereal_time(jd);
//! assert!((0.0..24.0).contains(&gmst));
//! ```

// Core modules
pub mod celestial;
pub mod coordinates;
pub mod eclipse;
#[cfg(feature = "network")]
pub mod net;
pub mod orbital;
pub mod planets;
pub mod satellite;
/// **Educational** SGP4-related mechanics (two-body skeleton vs production `sgp4`).
///
/// This is **not** used by [`satellite::propagate`]; it exists for learning and regression
/// checks documented in `docs/sgp4.md`.
pub mod sgp4_teaching;
pub mod time;

/// Error handling: the crate-wide [`error::AstroError`] type and [`error::Result`] alias.
pub mod error {
    use thiserror::Error;

    /// The error type returned across the Ephemerust API.
    ///
    /// Most variants carry a human-readable message describing the offending value; the
    /// [`AstroError::Tle`] variant instead wraps the structured, educational
    /// [`crate::satellite::TleError`], and [`AstroError::hint`] surfaces a corrective hint
    /// where one is available.
    #[derive(Error, Debug)]
    pub enum AstroError {
        /// A coordinate input was invalid (e.g. NaN, infinite, or out of range).
        #[error("Invalid coordinate: {0}")]
        InvalidCoordinate(String),

        /// A time or date input could not be parsed or was out of range.
        #[error("Invalid time: {0}")]
        InvalidTime(String),

        /// A numerical calculation failed or could not converge.
        #[error("Calculation error: {0}")]
        CalculationError(String),

        /// A structured, educational TLE-parsing failure. The detailed, teaching-oriented
        /// message comes from the wrapped [`crate::satellite::TleError`].
        #[error(transparent)]
        Tle(#[from] crate::satellite::TleError),

        /// A satellite operation failed at runtime (e.g. propagation diverged), as opposed to
        /// a TLE-format problem (which uses [`AstroError::Tle`]).
        #[error("Satellite error: {0}")]
        SatelliteError(String),

        /// An underlying I/O error (for example, while reading a TLE from a file).
        #[error("IO error: {0}")]
        IoError(#[from] std::io::Error),
    }

    impl AstroError {
        /// Returns a short, actionable formatting hint suitable for display on a dedicated
        /// `Hint:` line after the error. Currently provided for TLE-parsing errors, where the
        /// hint explains how to correct the input; other variants return `None`.
        pub fn hint(&self) -> Option<&'static str> {
            match self {
                AstroError::Tle(e) => e.hint(),
                _ => None,
            }
        }
    }

    /// A convenience alias for `Result<T, AstroError>` used throughout the crate.
    pub type Result<T> = std::result::Result<T, AstroError>;
}

// Re-export commonly used types and functions for convenience
pub use error::{AstroError, Result};

// Re-export coordinate types and WGS84 geodetic helpers
pub use coordinates::{
    AltAz, Ecef, Eci, Geodetic, RaDec, ecef_to_geodetic_wgs84, geodetic_wgs84_to_ecef,
};

// Re-export celestial object types
pub use celestial::{CelestialObject, ObserverLocation, RiseSetTimes, sun_vector_km};

// Re-export Earth-shadow (eclipse) types
pub use eclipse::{ShadowState, ShadowTransition, shadow_state, shadow_transitions};

// Re-export planet types
pub use planets::{Planet, calculate_planet_position};

// Re-export satellite types and propagation / ground-point pipeline
pub use satellite::{
    GroundTrackSample, LookAngles, Pass, PropagationModel, Propagator, Subpoint, TemeState, Tle,
    TleError, ecef_to_geodetic, ground_track, ground_track_to_csv, ground_track_to_json,
    look_angles, look_angles_with_model, predict_passes, predict_passes_with_model, propagate,
    propagate_with_model, select_tle, subpoint, subpoint_with_model, teme_to_ecef,
};

// Re-export time functions
pub use time::{greenwich_mean_sidereal_time, julian_date, local_sidereal_time};
