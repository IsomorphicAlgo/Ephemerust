//! Criterion benchmarks for Ephemerust's hot paths.
//!
//! Run with `cargo bench`. Results land in `target/criterion/` with HTML-free
//! summary output on stdout. These benchmarks back the performance numbers
//! quoted in `docs/architecture.md` and the CHANGELOG.
//!
//! What is measured and why:
//!
//! - **`tle_parse`** — fixed-column TLE parsing (validation, checksums, field
//!   extraction). Pure string work, no propagation.
//! - **`planet_position_mars`** — full VSOP87 pipeline: heliocentric series for
//!   Mars and Earth, geocentric conversion, RA/Dec.
//! - **`propagate_single`** — one SGP4 state at a fixed time via the one-shot
//!   `propagate` API (includes element parsing and propagator initialization).
//! - **`ground_track_90min_60s`** — 90 samples over one ISS orbit; the classic
//!   "propagate many times from one TLE" workload that dominates real tracking.

use chrono::{Duration, TimeZone, Utc};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use ephemerust::satellite::{Propagator, Tle, ground_track, propagate};
use ephemerust::{Planet, calculate_planet_position, julian_date};

// Canonical ISS (ZARYA) element set, same fixture as the unit-test suite.
// Epoch: 2020-07-12 ~21:16 UTC (day 194.88612269 of 2020).
const ISS_LINE1: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
const ISS_LINE2: &str = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

fn iss_tle() -> Tle {
    Tle::parse(&format!("{ISS_LINE1}\n{ISS_LINE2}")).expect("ISS fixture must parse")
}

fn bench_tle_parse(c: &mut Criterion) {
    let text = format!("{ISS_LINE1}\n{ISS_LINE2}");
    c.bench_function("tle_parse", |b| {
        b.iter(|| Tle::parse(black_box(&text)).unwrap())
    });
}

fn bench_planet_position(c: &mut Criterion) {
    let epoch = Utc.with_ymd_and_hms(2026, 7, 28, 0, 0, 0).unwrap();
    let jd = julian_date(epoch);
    c.bench_function("planet_position_mars", |b| {
        b.iter(|| calculate_planet_position(black_box(Planet::Mars), black_box(jd)).unwrap())
    });
}

fn bench_propagate_single(c: &mut Criterion) {
    let tle = iss_tle();
    // 30 minutes past the element-set epoch.
    let t = Utc.with_ymd_and_hms(2020, 7, 12, 21, 46, 0).unwrap();
    c.bench_function("propagate_single", |b| {
        b.iter(|| propagate(black_box(&tle), black_box(t)).unwrap())
    });
}

/// One propagation step on an already-initialized `Propagator`. The difference between
/// this and `propagate_single` is the per-call SGP4 initialization cost that `Propagator`
/// lets callers pay only once.
fn bench_propagate_reused(c: &mut Criterion) {
    let tle = iss_tle();
    let prop = Propagator::new(&tle).unwrap();
    let t = Utc.with_ymd_and_hms(2020, 7, 12, 21, 46, 0).unwrap();
    c.bench_function("propagate_reused", |b| {
        b.iter(|| black_box(&prop).propagate(black_box(t)).unwrap())
    });
}

fn bench_ground_track(c: &mut Criterion) {
    let tle = iss_tle();
    let start = Utc.with_ymd_and_hms(2020, 7, 12, 21, 16, 0).unwrap();
    let end = start + Duration::minutes(90); // ~one ISS orbit
    let step = Duration::seconds(60); // 90 samples
    c.bench_function("ground_track_90min_60s", |b| {
        b.iter(|| ground_track(black_box(&tle), black_box(start), black_box(end), step).unwrap())
    });
}

criterion_group!(
    benches,
    bench_tle_parse,
    bench_planet_position,
    bench_propagate_single,
    bench_propagate_reused,
    bench_ground_track
);
criterion_main!(benches);
