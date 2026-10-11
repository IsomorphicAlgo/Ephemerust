//! Cross-checks the `frames` module against astropy/ERFA (the IAU SOFA library).
//!
//! `tests/data/astropy_frames.csv` holds 56 states (LEO, polar LEO, GEO and a high-eccentricity
//! apogee, at seven epochs from 2000 to 2026, including one 30 s before a leap second) rotated
//! from GCRF and from TEME to ITRF by astropy with IERS finals2000A Earth orientation. Each row
//! records the UT1−UTC and polar motion used, and Ephemerust is fed the same values.
//!
//! Bounds, with the measured worst case at 1.1.0:
//!
//! | Path | Position bound | Measured | Velocity bound | Measured |
//! |------|----------------|----------|----------------|----------|
//! | GCRF → ITRF (IAU 2006/2000B vs 2000A) | 0.25 m | 0.068 m | 1 mm/s | 0.25 mm/s |
//! | TEME → ITRF (GMST82 + polar motion) | 0.01 m | 0.0024 m | 1 mm/s | 0.33 mm/s |
//!
//! The velocity residual is dominated by astropy's finite-difference velocity transform.

use chrono::{DateTime, NaiveDateTime, Utc};
use ephemerust::frames::{EarthOrientation, InertialFrame, inertial_to_itrf};

const FIXTURE: &str = include_str!("data/astropy_frames.csv");

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn inertial_to_itrf_matches_astropy() {
    let mut worst = std::collections::BTreeMap::<&str, (f64, f64)>::new();
    let mut n = 0;
    for line in FIXTURE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        let time: DateTime<Utc> = NaiveDateTime::parse_from_str(f[0], "%Y-%m-%dT%H:%M:%S")
            .unwrap()
            .and_utc();
        let num = |i: usize| f[i].parse::<f64>().unwrap();
        let eop = EarthOrientation::new(num(1), num(2), num(3)).unwrap();
        let frame = match f[4] {
            "GCRF" => InertialFrame::Gcrf,
            "TEME" => InertialFrame::Teme,
            other => panic!("unknown frame {other}"),
        };
        let (r, v) = ([num(5), num(6), num(7)], [num(8), num(9), num(10)]);
        let (ri, vi) = inertial_to_itrf(frame, time, r, v, &eop);
        let dr_m = dist(ri, [num(11), num(12), num(13)]) * 1000.0;
        let dv_mm_s = dist(vi, [num(14), num(15), num(16)]) * 1e6;
        let w = worst.entry(f[4]).or_insert((0.0, 0.0));
        w.0 = w.0.max(dr_m);
        w.1 = w.1.max(dv_mm_s);
        n += 1;
    }
    assert_eq!(n, 56);
    eprintln!("worst-case vs astropy: {worst:?} (metres, mm/s)");
    let (gr, gv) = worst["GCRF"];
    let (tr, tv) = worst["TEME"];
    assert!(gr < 0.25 && gv < 1.0, "GCRF→ITRF: {gr:.4} m, {gv:.4} mm/s");
    assert!(tr < 0.01 && tv < 1.0, "TEME→ITRF: {tr:.4} m, {tv:.4} mm/s");
}
