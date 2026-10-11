//! Cross-checks satellite look angles against an **independent** implementation.
//!
//! `tests/data/skyfield_iss_everett.csv` holds azimuth, elevation, slant range and range rate
//! for the crate's reference ISS element set, generated with Python's Skyfield (full IAU 2000
//! TEME → GCRS → ITRS chain with UT1). Because both sides propagate the *same* TLE, the
//! differences isolate Ephemerust's frame and velocity handling from TLE error.
//!
//! Tolerances, with the measured worst case at 1.0.0 over all 360 samples (24 h, above and
//! below the horizon):
//!
//! | Quantity | Bound | Measured | Why it is not zero |
//! |----------|-------|----------|--------------------|
//! | Line-of-sight direction | 2 arcmin | 0.34 arcmin | GMST-only TEME→ECEF (no polar motion), UT1 taken as UTC, WGS84 vs WGS72 propagation constants |
//! | Slant range | 0.25 km | 0.075 km | Same model differences, mostly along-track |
//! | Range rate | 2 m/s | 0.41 m/s | Same; at 437.5 MHz this is under 3 Hz of Doppler |
//!
//! See `docs/accuracy-and-limits.md` for how these feed a Doppler / pointing error budget.

use chrono::Duration;
use ephemerust::{ObserverLocation, Propagator, Tle};

const ISS: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
                   2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

const FIXTURE: &str = include_str!("data/skyfield_iss_everett.csv");

fn great_circle_deg(az1: f64, el1: f64, az2: f64, el2: f64) -> f64 {
    let (a1, e1, a2, e2) = (
        az1.to_radians(),
        el1.to_radians(),
        az2.to_radians(),
        el2.to_radians(),
    );
    let c = e1.sin() * e2.sin() + e1.cos() * e2.cos() * (a1 - a2).cos();
    c.clamp(-1.0, 1.0).acos().to_degrees()
}

#[test]
fn look_angles_match_skyfield_reference() {
    let tle = Tle::parse(ISS).expect("reference TLE parses");
    let prop = Propagator::new(&tle).expect("propagator");
    let obs = ObserverLocation {
        latitude: 47.9088,
        longitude: -122.2503,
        elevation: 0.0,
    };

    let (mut worst_dir_arcmin, mut worst_range_km, mut worst_rr_m_s) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut n = 0;
    for line in FIXTURE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let f: Vec<f64> = line.split(',').map(|v| v.trim().parse().unwrap()).collect();
        let t = tle.epoch + Duration::seconds(f[0] as i64);
        let la = prop.look_angles(t, obs).expect("look angles");
        worst_dir_arcmin = worst_dir_arcmin
            .max(great_circle_deg(la.azimuth_deg, la.elevation_deg, f[1], f[2]) * 60.0);
        worst_range_km = worst_range_km.max((la.range_km - f[3]).abs());
        worst_rr_m_s = worst_rr_m_s.max((la.range_rate_km_s - f[4]).abs() * 1000.0);
        n += 1;
    }

    assert_eq!(n, 360, "fixture row count");
    assert!(
        worst_dir_arcmin < 2.0,
        "direction error {worst_dir_arcmin:.3} arcmin"
    );
    assert!(worst_range_km < 0.25, "range error {worst_range_km:.4} km");
    assert!(worst_rr_m_s < 2.0, "range-rate error {worst_rr_m_s:.3} m/s");
}
