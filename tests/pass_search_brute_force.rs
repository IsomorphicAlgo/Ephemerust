//! Exhaustive check of the pass search against a one-second brute-force scan.
//!
//! `predict_passes` walks the search window in *safe steps* (see its documentation) and should
//! therefore find every pass a dense scan finds. This test verifies that claim the slow way:
//! it samples the elevation every second for ten days, takes each run of samples at or above
//! the mask as a pass, and requires the search to report the same passes.
//!
//! Ten element sets (low, eccentric, 12-hour, transfer and inclined-geosynchronous orbits, plus
//! one that is decaying) are checked from five stations at six elevation masks, 300 searches
//! in all. It propagates about 43 million states, so it is `#[ignore]`d by default. Run it with:
//!
//! ```text
//! cargo test --release --test pass_search_brute_force -- --ignored --nocapture
//! ```
//!
//! Measured at 1.2.1: 7074 passes, none missed, none spurious, every AOS and LOS inside the
//! one-second bracket the scan gives. The fixed-step search it replaced missed 231 of them
//! (3.3 %), all shorter than its sampling step: 26 at masks of 0–10°, 41 at 30° and 164 at
//! 60–80°, where every pass is short. `satellite::tests::finds_every_short_pass_a_one_second_scan_finds` is the
//! fast regression test that runs on every `cargo test`.

use chrono::{DateTime, Duration, Utc};
use ephemerust::omm::Omm;
use ephemerust::{ObserverLocation, Propagator, Tle};

const DAYS: i64 = 10;

/// A synthetic element set (not a real satellite) with the given mean elements.
fn synthetic(name: &str, mean_motion: f64, eccentricity: f64, inclination: f64) -> Omm {
    Omm::parse(&format!(
        r#"{{"OBJECT_NAME":"{name}","EPOCH":"2026-08-01T12:00:00.000000",
            "MEAN_MOTION":{mean_motion},"ECCENTRICITY":{eccentricity},
            "INCLINATION":{inclination},"RA_OF_ASC_NODE":120.0,"ARG_OF_PERICENTER":200.0,
            "MEAN_ANOMALY":50.0,"BSTAR":0.00005}}"#
    ))
    .expect("synthetic OMM")
}

fn element_sets() -> Vec<(String, Propagator, DateTime<Utc>)> {
    let iss = Tle::parse(
        "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
    )
    .expect("ISS TLE");
    // High drag: this orbit shrinks visibly over the ten days.
    let decaying = Tle::parse(
        "1 55123U 98067UQ  23115.44827133  .00787702  29408-3  15680-2 0  9999\n\
         2 55123  51.6242 216.2930 0014649 331.8976  28.1241 15.99081912 18396",
    )
    .expect("MARIO TLE");

    let mut sets = vec![
        ("ISS".to_string(), Propagator::new(&iss).unwrap(), iss.epoch),
        (
            "decaying".to_string(),
            Propagator::new(&decaying).unwrap(),
            decaying.epoch - Duration::days(2),
        ),
    ];
    for (name, n, e, i) in [
        ("sun-synchronous 500 km", 15.2, 0.0012, 97.5),
        ("polar 800 km", 14.3, 0.001, 98.7),
        ("1400 km", 12.62, 0.0005, 52.0),
        ("eccentric low orbit", 13.5, 0.05, 65.0),
        ("12-hour circular", 2.0057, 0.01, 55.0),
        ("Molniya", 2.00613, 0.7215, 63.43),
        ("transfer orbit", 2.27, 0.73, 27.0),
        ("inclined geosynchronous", 1.0027, 0.0003, 8.0),
    ] {
        let omm = synthetic(name, n, e, i);
        sets.push((
            name.to_string(),
            Propagator::from_omm(&omm).unwrap(),
            omm.epoch,
        ));
    }
    sets
}

/// Runs of one-second samples at or above the mask: `(first second above, first second below)`.
fn scan_passes(elevations: &[f64], mask: f64) -> Vec<(i64, i64)> {
    let mut runs = Vec::new();
    let mut open: Option<i64> = None;
    for (k, &el) in elevations.iter().enumerate() {
        match (open, el >= mask) {
            (None, true) => open = Some(k as i64),
            (Some(k0), false) => {
                runs.push((k0, k as i64));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(k0) = open {
        runs.push((k0, elevations.len() as i64));
    }
    runs
}

#[test]
#[ignore = "propagates ~43 million states; run with --release -- --ignored"]
fn search_finds_exactly_the_passes_a_one_second_scan_finds() {
    let stations = [
        ("Everett", 47.9088, -122.2503),
        ("equator", 0.0, 0.0),
        ("Svalbard", 78.2298, 15.4078),
        ("Sydney", -33.8688, 151.2093),
        ("Punta Arenas", -53.16, -70.91),
    ];
    let masks = [0.0, 5.0, 10.0, 30.0, 60.0, 80.0];
    let seconds = DAYS * 86_400;

    let (mut total, mut problems) = (0_usize, Vec::new());
    for (name, prop, start) in element_sets() {
        let end = start + Duration::seconds(seconds);
        let mut set_total = 0;
        for (station, latitude, longitude) in stations {
            let observer = ObserverLocation {
                latitude,
                longitude,
                elevation: 0.0,
            };
            let elevations: Vec<f64> = (0..seconds)
                .map(|k| {
                    prop.look_angles(start + Duration::seconds(k), observer)
                        .expect("look angles")
                        .elevation_deg
                })
                .collect();

            for mask in masks {
                let scan = scan_passes(&elevations, mask);
                let found = prop
                    .predict_passes(observer, start, end, mask)
                    .expect("pass search");
                set_total += scan.len();

                let label = format!("{name} / {station} / mask {mask}°");
                if found.len() != scan.len() {
                    problems.push(format!(
                        "{label}: the scan has {} passes, the search {}",
                        scan.len(),
                        found.len()
                    ));
                    continue;
                }
                for (pass, (first_above, first_below)) in found.iter().zip(&scan) {
                    // The true crossing lies in the second before the scan's sample, except
                    // where the pass is cut by the window itself.
                    let aos_lag = *first_above * 1000 - (pass.aos - start).num_milliseconds();
                    let los_lag = *first_below * 1000 - (pass.los - start).num_milliseconds();
                    if !(0..=1000).contains(&aos_lag) || !(0..=1000).contains(&los_lag) {
                        problems.push(format!(
                            "{label}: pass at +{first_above} s has AOS lag {aos_lag} ms, \
                             LOS lag {los_lag} ms"
                        ));
                    }
                }
            }
        }
        println!("{name:24} {set_total:5} passes");
        total += set_total;
    }

    println!("total: {total} passes, {} problems", problems.len());
    assert!(
        problems.is_empty(),
        "{} disagreements with the one-second scan:\n{}",
        problems.len(),
        problems.join("\n")
    );
    assert!(
        total > 6000,
        "the fixture should hold thousands of passes, got {total}"
    );
}
