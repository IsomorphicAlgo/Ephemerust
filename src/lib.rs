// Core modules
pub mod coordinates;
pub mod celestial;
pub mod time;
pub mod orbital;
pub mod planets;
pub mod satellite;

// Error handling
pub mod error {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum AstroError {
        #[error("Invalid coordinate: {0}")]
        InvalidCoordinate(String),
        
        #[error("Invalid time: {0}")]
        InvalidTime(String),
        
        #[error("Calculation error: {0}")]
        CalculationError(String),
        
        #[error("Satellite error: {0}")]
        SatelliteError(String),
        
        #[error("IO error: {0}")]
        IoError(#[from] std::io::Error),
    }

    pub type Result<T> = std::result::Result<T, AstroError>;
}

// Re-export commonly used types and functions for convenience
pub use error::{AstroError, Result};

// Re-export coordinate types
pub use coordinates::{RaDec, AltAz, Ecef, Eci};

// Re-export celestial object types
pub use celestial::{CelestialObject, ObserverLocation, RiseSetTimes};

// Re-export planet types
pub use planets::{Planet, calculate_planet_position};

// Re-export satellite types
pub use satellite::{Tle, TemeState, Subpoint, LookAngles, Pass};

// Re-export time functions
pub use time::{julian_date, greenwich_mean_sidereal_time, local_sidereal_time};
