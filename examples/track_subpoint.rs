//! Sub-satellite geodetic point at the element-set epoch for a canonical ISS TLE.
//!
//! Run from the crate root:
//!
//! ```text
//! cargo run --example track_subpoint
//! ```

use ephemerust::satellite::{Tle, subpoint};

fn main() -> ephemerust::Result<()> {
    let tle = Tle::parse(
        "ISS (ZARYA)\n\
         1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
    )?;
    let sp = subpoint(&tle, tle.epoch)?;
    println!("epoch (UTC): {}", tle.epoch);
    println!("latitude_deg:  {:.6}", sp.latitude_deg);
    println!("longitude_deg: {:.6}", sp.longitude_deg);
    println!("altitude_km:   {:.3}", sp.altitude_km);
    Ok(())
}
