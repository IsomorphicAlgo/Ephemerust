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
//! currently at **Milestone 0** (foundation): the public types are defined as stubs and the
//! propagation engine is wired in and exercised by a smoke test.
//!
//! # Example
//!
//! ```
//! use cli_astro_calc::satellite::TemeState;
//!
//! let state = TemeState {
//!     position_km: [6778.0, 0.0, 0.0],
//!     velocity_km_s: [0.0, 7.5, 0.0],
//! };
//! assert_eq!(state.position_km[0], 6778.0);
//! ```

use crate::Result;
use chrono::{DateTime, Utc};

/// A parsed Two-Line Element set.
///
/// Stub for Milestone 0. Milestone 1 will replace the raw fields with a typed, validated
/// representation (catalog number, epoch, and orbital elements).
#[derive(Debug, Clone)]
pub struct Tle {
    /// Optional object name (from the leading line of a 3-line element set).
    pub name: Option<String>,
    /// First data line of the element set.
    pub line1: String,
    /// Second data line of the element set.
    pub line2: String,
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

/// Propagates a TLE to the given UTC time and returns the TEME state.
///
/// Stub for Milestone 0. The implementation is delivered in Milestone 2, which wraps the
/// `sgp4` propagator behind this signature.
pub fn propagate(_tle: &Tle, _time: DateTime<Utc>) -> Result<TemeState> {
    Err(crate::AstroError::SatelliteError(
        "propagation is not yet implemented (planned for Milestone 2)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    //! Milestone 0 smoke test: confirm the `sgp4` engine is wired in and produces a
    //! physically plausible state for a canonical low-Earth-orbit element set.

    /// Canonical ISS (ZARYA) element set used as a fixed reference across the test suite.
    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_LINE1: &str =
        "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
    const ISS_LINE2: &str =
        "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

    #[test]
    fn iss_propagation_yields_plausible_leo_radius() {
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
