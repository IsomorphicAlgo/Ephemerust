//! Earth-shadow (eclipse) geometry: is a satellite in sunlight, penumbra, or umbra?
//!
//! A satellite behind the Earth (relative to the Sun) flies through two nested shadow
//! cones. Because the Sun is a disk, not a point, the transition is gradual:
//!
//! - **Umbra** — the inner cone where the Earth blocks the *entire* solar disk. Total
//!   eclipse: no direct sunlight, solar panels produce nothing.
//! - **Penumbra** — the surrounding region where the Earth blocks *part* of the solar
//!   disk. Partial eclipse: illumination ramps between full and zero. For a low-Earth
//!   orbit the crossing lasts on the order of **ten seconds** — brief, but exactly the
//!   window where naive on/off models and real telemetry disagree.
//! - **Sunlit** — everywhere else.
//!
//! ## The model (conical, apparent-disk overlap)
//!
//! This module implements the standard **conical shadow model** (Vallado, *Fundamentals of
//! Astrodynamics and Applications*, §5.3; equivalently Montenbruck & Gill §3.4). From the
//! satellite's point of view, compare three angles:
//!
//! - `a` — apparent angular radius of the **Sun**: `asin(R_sun / |sat→sun|)`
//! - `b` — apparent angular radius of the **Earth**: `asin(R_earth / |sat→earth|)`
//! - `c` — angular separation between the Sun's center and the Earth's center
//!
//! Then classify by disk overlap:
//!
//! | Condition | Meaning | State |
//! |-----------|---------|-------|
//! | `c >= a + b` | disks don't touch | [`ShadowState::Sunlit`] |
//! | `c < b - a`  | Earth's disk fully covers the Sun's | [`ShadowState::Umbra`] |
//! | otherwise    | partial overlap | [`ShadowState::Penumbra`] |
//!
//! A simpler **cylindrical** model (shadow = infinite cylinder of Earth radius) gets entry
//! and exit wrong by roughly the penumbra width and cannot distinguish partial from total
//! eclipse; the conical model costs only a few extra trig calls.
//!
//! ## Assumptions and accuracy
//!
//! - **Spherical Earth** of WGS84 *equatorial* radius. Ignoring oblateness (~0.34%) shifts
//!   shadow-crossing times by a few seconds at LEO; using the equatorial (largest) radius
//!   makes the model err slightly toward "in shadow", the conservative direction for power
//!   budgeting. No atmospheric refraction of sunlight into the shadow cone.
//! - **Low-precision solar ephemeris** ([`crate::celestial::sun_vector_km`], ~0.01° in
//!   longitude) in the equator-of-date frame, compared directly against the satellite's TEME
//!   position. Both approximations are far below the ~0.5° angular scale of the geometry.
//! - The rare **annular** case (satellite so far away that the Earth's disk fits inside the
//!   Sun's, `b < a`) is classified as [`ShadowState::Penumbra`]: some sunlight is blocked but
//!   not all. For Earth orbiters below ~1.3 million km, `b > a` always holds.
//!
//! ## Example
//!
//! ```
//! use ephemerust::eclipse::{shadow_state, ShadowState};
//! use ephemerust::satellite::{Propagator, Tle};
//!
//! let tle = Tle::parse(
//!     "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
//!      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
//! )?;
//! let prop = Propagator::new(&tle)?;
//!
//! let state = shadow_state(&prop, tle.epoch)?;
//! // Every state is one of the three regimes; ShadowState also prints nicely.
//! assert!(matches!(
//!     state,
//!     ShadowState::Sunlit | ShadowState::Penumbra | ShadowState::Umbra
//! ));
//! println!("ISS at epoch: {state}");
//! # Ok::<(), ephemerust::AstroError>(())
//! ```

use crate::Result;
use crate::celestial::sun_vector_km;
use crate::satellite::Propagator;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

/// IAU nominal solar radius in kilometres.
pub const SUN_RADIUS_KM: f64 = 695_700.0;

/// Spherical-Earth radius used for shadow geometry: the WGS84 equatorial radius, in km.
const EARTH_RADIUS_KM: f64 = crate::coordinates::WGS84_A / 1000.0;

/// The illumination regime of a satellite with respect to the Earth's shadow.
///
/// Ordered by "shadow depth": `Sunlit < Penumbra < Umbra`. See the [module docs](self) for
/// the geometry that distinguishes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowState {
    /// Full direct sunlight: the Earth does not block any part of the solar disk.
    Sunlit,
    /// Partial eclipse: the Earth blocks part (not all) of the solar disk.
    Penumbra,
    /// Total eclipse: the Earth blocks the entire solar disk.
    Umbra,
}

impl ShadowState {
    /// `true` when the satellite receives any direct sunlight (i.e. not in umbra).
    ///
    /// In penumbra the illumination is partial but nonzero; power-critical consumers that
    /// need "any light at all" (rather than "full light") should use this instead of
    /// comparing against [`ShadowState::Sunlit`].
    #[must_use]
    pub fn is_illuminated(self) -> bool {
        self != ShadowState::Umbra
    }
}

impl std::fmt::Display for ShadowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ShadowState::Sunlit => "sunlit",
            ShadowState::Penumbra => "penumbra",
            ShadowState::Umbra => "umbra",
        })
    }
}

/// A refined instant at which a satellite crosses a shadow boundary.
///
/// Produced by [`shadow_transitions`]. `from` and `to` are always **adjacent** regimes
/// (sunlit ↔ penumbra or penumbra ↔ umbra): even when a coarse scan jumps straight from
/// sunlit to umbra, the search emits the two underlying boundary crossings separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ShadowTransition {
    /// UTC instant of the boundary crossing, refined to ~1 ms.
    pub time: DateTime<Utc>,
    /// Regime before the crossing.
    pub from: ShadowState,
    /// Regime after the crossing.
    pub to: ShadowState,
}

/// Classifies the shadow state from explicit **geocentric** position vectors (km).
///
/// This is the pure geometric core of the module — no time, no ephemeris, no propagation —
/// which makes it directly unit-testable and reusable with positions from any source, as
/// long as satellite and Sun share one geocentric equatorial frame.
///
/// Degenerate inputs (satellite at or below the Earth's surface, or a zero Sun vector) are
/// classified as [`ShadowState::Umbra`]: no direct sunlight is the safe answer.
///
/// # Example
///
/// A satellite 7,000 km from the geocenter, directly on the anti-Sun side, is in umbra:
///
/// ```
/// use ephemerust::eclipse::{shadow_state_from_vectors, ShadowState};
///
/// let sun = [1.496e8, 0.0, 0.0]; // Sun 1 AU away along +x
/// let sat = [-7.0e3, 0.0, 0.0]; // satellite exactly behind the Earth
/// assert_eq!(shadow_state_from_vectors(sat, sun), ShadowState::Umbra);
/// ```
#[must_use]
pub fn shadow_state_from_vectors(r_sat_km: [f64; 3], r_sun_km: [f64; 3]) -> ShadowState {
    // Vector from the satellite to the Sun, and to the Earth's center.
    let to_sun = [
        r_sun_km[0] - r_sat_km[0],
        r_sun_km[1] - r_sat_km[1],
        r_sun_km[2] - r_sat_km[2],
    ];
    let to_earth = [-r_sat_km[0], -r_sat_km[1], -r_sat_km[2]];

    let d_sun = norm(to_sun);
    let d_earth = norm(to_earth);
    // Degenerate geometry: satellite at/below the Earth's surface, "inside" the Sun, or
    // non-finite input. No direct sunlight is the safe classification.
    if !(d_sun.is_finite() && d_earth.is_finite())
        || d_earth <= EARTH_RADIUS_KM
        || d_sun <= SUN_RADIUS_KM
    {
        return ShadowState::Umbra;
    }

    // Apparent angular radii of the two disks, and the angle between their centers,
    // all as seen from the satellite.
    let a = (SUN_RADIUS_KM / d_sun).min(1.0).asin();
    let b = (EARTH_RADIUS_KM / d_earth).min(1.0).asin();
    let cos_c = dot(to_sun, to_earth) / (d_sun * d_earth);
    let c = cos_c.clamp(-1.0, 1.0).acos();

    if c >= a + b {
        ShadowState::Sunlit
    } else if c < b - a {
        ShadowState::Umbra
    } else {
        // Partial overlap — including the annular case (b < a), where some light always
        // leaks around the Earth's disk.
        ShadowState::Penumbra
    }
}

/// Computes the shadow state of a satellite at a UTC instant.
///
/// Propagates with the given [`Propagator`], obtains the Sun's geocentric position from
/// [`sun_vector_km`], and classifies with [`shadow_state_from_vectors`]. Equivalent to the
/// [`Propagator::shadow_state`] method.
///
/// # Errors
///
/// Propagation errors from [`Propagator::propagate`] (time too far from epoch, or a
/// diverging orbit).
pub fn shadow_state(prop: &Propagator, time: DateTime<Utc>) -> Result<ShadowState> {
    let state = prop.propagate(time)?;
    Ok(shadow_state_from_vectors(
        state.position_km,
        sun_vector_km(time)?,
    ))
}

impl Propagator {
    /// The satellite's [`ShadowState`] (sunlit / penumbra / umbra) at `time`.
    ///
    /// Method form of [`shadow_state`]; see the [`crate::eclipse`] module docs for the
    /// conical shadow model and its assumptions.
    ///
    /// # Errors
    ///
    /// Same as [`Propagator::propagate`].
    pub fn shadow_state(&self, time: DateTime<Utc>) -> Result<ShadowState> {
        shadow_state(self, time)
    }
}

/// Numeric shadow depth used to order boundary crossings during refinement.
fn depth(s: ShadowState) -> u8 {
    match s {
        ShadowState::Sunlit => 0,
        ShadowState::Penumbra => 1,
        ShadowState::Umbra => 2,
    }
}

fn state_at_depth(d: u8) -> ShadowState {
    match d {
        0 => ShadowState::Sunlit,
        1 => ShadowState::Penumbra,
        _ => ShadowState::Umbra,
    }
}

/// Finds every shadow-boundary crossing in `[window_start, window_end)`.
///
/// The search samples the shadow state at `step` intervals, then refines each detected
/// change by bisection to ~1 ms. Because the penumbra crossing of a low-Earth orbit lasts
/// only seconds, a coarse step will often jump **straight from sunlit to umbra**; the
/// search handles this by refining each intermediate boundary separately, so the returned
/// transitions always move between adjacent regimes (sunlit ↔ penumbra ↔ umbra) and
/// reconstruct the full entry/exit sequence:
///
/// ```text
/// sunlit → penumbra → umbra → penumbra → sunlit
///        entry ramp   total    exit ramp
/// ```
///
/// `step` bounds what the scan can see: a shadow arc **shorter than one step** (or a
/// sunlit gap shorter than one step) can be missed entirely. For LEO, `Duration::seconds(30)`
/// comfortably resolves the ~35-minute shadow arcs; there is no reason to go coarser than a
/// few minutes for any Earth orbit.
///
/// # Errors
///
/// Propagation errors from [`Propagator::propagate`], or
/// [`AstroError::InvalidTime`](crate::AstroError::InvalidTime) when `step` is not positive.
///
/// # Example
///
/// ```
/// use chrono::Duration;
/// use ephemerust::eclipse::shadow_transitions;
/// use ephemerust::satellite::{Propagator, Tle};
///
/// let tle = Tle::parse(
///     "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
///      2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
/// )?;
/// let prop = Propagator::new(&tle)?;
///
/// // One ISS orbit is ~93 minutes: expect both an eclipse entry and an exit.
/// let transitions = shadow_transitions(
///     &prop,
///     tle.epoch,
///     tle.epoch + Duration::minutes(93),
///     Duration::seconds(30),
/// )?;
/// assert!(transitions.len() >= 2);
/// for t in &transitions {
///     println!("{}: {} → {}", t.time.format("%H:%M:%S%.3f"), t.from, t.to);
/// }
/// # Ok::<(), ephemerust::AstroError>(())
/// ```
pub fn shadow_transitions(
    prop: &Propagator,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    step: Duration,
) -> Result<Vec<ShadowTransition>> {
    if step <= Duration::zero() {
        return Err(crate::AstroError::InvalidTime(format!(
            "shadow_transitions step must be positive, got {step}"
        )));
    }
    let mut transitions = Vec::new();
    if window_end <= window_start {
        return Ok(transitions);
    }

    let mut prev_t = window_start;
    let mut prev_s = shadow_state(prop, prev_t)?;

    let mut t = window_start + step;
    loop {
        // Clamp the final sample to just inside the exclusive window end.
        let t_clamped = t.min(window_end - Duration::milliseconds(1));
        if t_clamped <= prev_t {
            break;
        }
        let s = shadow_state(prop, t_clamped)?;

        if s != prev_s {
            refine_crossings(prop, prev_t, prev_s, t_clamped, s, &mut transitions)?;
        }

        prev_t = t_clamped;
        prev_s = s;
        if t >= window_end {
            break;
        }
        t += step;
    }

    Ok(transitions)
}

/// Refines every boundary between `prev_s` at `prev_t` and `s` at `t`, appending one
/// [`ShadowTransition`] per boundary in chronological order.
///
/// When deepening (e.g. sunlit → umbra), boundaries are refined in increasing depth order;
/// when emerging, in decreasing order. Each bisection asks "is the depth at least `k` yet?"
/// (deepening) or "has the depth dropped below `k` yet?" (emerging), which is monotonic
/// across a single coarse step for any physically continuous shadow crossing.
fn refine_crossings(
    prop: &Propagator,
    prev_t: DateTime<Utc>,
    prev_s: ShadowState,
    t: DateTime<Utc>,
    s: ShadowState,
    out: &mut Vec<ShadowTransition>,
) -> Result<()> {
    let (d0, d1) = (depth(prev_s), depth(s));
    let mut lo = prev_t;

    if d1 > d0 {
        for k in (d0 + 1)..=d1 {
            let when = bisect_boundary(prop, lo, t, |st| depth(st) >= k)?;
            out.push(ShadowTransition {
                time: when,
                from: state_at_depth(k - 1),
                to: state_at_depth(k),
            });
            lo = when;
        }
    } else {
        for k in ((d1 + 1)..=d0).rev() {
            let when = bisect_boundary(prop, lo, t, |st| depth(st) < k)?;
            out.push(ShadowTransition {
                time: when,
                from: state_at_depth(k),
                to: state_at_depth(k - 1),
            });
            lo = when;
        }
    }
    Ok(())
}

/// Bisects for the earliest time in `(lo, hi]` where `crossed` becomes true.
///
/// Precondition: `crossed` is false at `lo` and true at `hi`. Refines to ~1 ms.
fn bisect_boundary(
    prop: &Propagator,
    mut lo: DateTime<Utc>,
    mut hi: DateTime<Utc>,
    crossed: impl Fn(ShadowState) -> bool,
) -> Result<DateTime<Utc>> {
    for _ in 0..56 {
        if hi.signed_duration_since(lo) <= Duration::milliseconds(1) {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        if crossed(shadow_state(prop, mid)?) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

fn norm(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::satellite::Tle;
    use chrono::Duration;

    const SUN_X: [f64; 3] = [1.496e8, 0.0, 0.0];

    // Geometry oracle for the synthetic tests below, with the Sun 1 AU along +x and the
    // satellite at x = -7,000 km: the umbra cone has shrunk to a radius of ~6,346 km at
    // that depth and the penumbra cone has grown to ~6,411 km, so the y offsets 6,000 /
    // 6,378 / 7,000 fall cleanly into umbra / penumbra / sunlit.

    #[test]
    fn sunward_side_is_sunlit() {
        assert_eq!(
            shadow_state_from_vectors([7.0e3, 0.0, 0.0], SUN_X),
            ShadowState::Sunlit
        );
    }

    #[test]
    fn anti_sun_axis_is_umbra() {
        assert_eq!(
            shadow_state_from_vectors([-7.0e3, 0.0, 0.0], SUN_X),
            ShadowState::Umbra
        );
        assert_eq!(
            shadow_state_from_vectors([-7.0e3, 6.0e3, 0.0], SUN_X),
            ShadowState::Umbra
        );
    }

    #[test]
    fn shadow_cone_edge_is_penumbra() {
        assert_eq!(
            shadow_state_from_vectors([-7.0e3, 6.378e3, 0.0], SUN_X),
            ShadowState::Penumbra
        );
    }

    #[test]
    fn beyond_penumbra_cone_is_sunlit() {
        assert_eq!(
            shadow_state_from_vectors([-7.0e3, 7.0e3, 0.0], SUN_X),
            ShadowState::Sunlit
        );
    }

    #[test]
    fn perpendicular_to_sun_line_is_sunlit() {
        assert_eq!(
            shadow_state_from_vectors([0.0, 7.0e3, 0.0], SUN_X),
            ShadowState::Sunlit
        );
    }

    #[test]
    fn degenerate_inputs_classified_as_umbra() {
        // At/below the surface, and a zero Sun vector: no direct sunlight is the safe answer.
        assert_eq!(
            shadow_state_from_vectors([1.0e3, 0.0, 0.0], SUN_X),
            ShadowState::Umbra
        );
        assert_eq!(
            shadow_state_from_vectors([7.0e3, 0.0, 0.0], [0.0, 0.0, 0.0]),
            ShadowState::Umbra
        );
    }

    #[test]
    fn shadow_depth_ordering_matches_enum_order() {
        assert!(ShadowState::Sunlit < ShadowState::Penumbra);
        assert!(ShadowState::Penumbra < ShadowState::Umbra);
        assert!(!ShadowState::Umbra.is_illuminated());
        assert!(ShadowState::Penumbra.is_illuminated());
        assert!(ShadowState::Sunlit.is_illuminated());
    }

    const ISS_LINE1: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
    const ISS_LINE2: &str = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

    fn iss_propagator() -> Propagator {
        let tle = Tle::from_lines(None, ISS_LINE1, ISS_LINE2).expect("valid TLE");
        Propagator::new(&tle).expect("valid elements")
    }

    fn iss_epoch() -> DateTime<Utc> {
        Tle::from_lines(None, ISS_LINE1, ISS_LINE2).unwrap().epoch
    }

    #[test]
    fn iss_umbra_fraction_over_one_orbit_is_plausible() {
        // The ISS (~51.6° inclination, ~420 km altitude) spends roughly a third of each
        // orbit in shadow; the exact value depends on the Sun's beta angle, but it can
        // never exceed ~45% at LEO nor plausibly drop below ~15% in mid-July.
        let prop = iss_propagator();
        let start = iss_epoch();
        let period = Duration::seconds((86_400.0 / 15.495_079) as i64);

        let mut umbra = 0usize;
        let mut total = 0usize;
        let mut t = start;
        while t < start + period {
            if prop.shadow_state(t).expect("propagates") == ShadowState::Umbra {
                umbra += 1;
            }
            total += 1;
            t += Duration::seconds(10);
        }
        let fraction = umbra as f64 / total as f64;
        assert!(
            (0.15..=0.45).contains(&fraction),
            "ISS umbra fraction should be ~1/3 of the orbit, got {fraction:.3}"
        );
    }

    #[test]
    fn iss_transitions_alternate_and_bracket_umbra_with_penumbra() {
        let prop = iss_propagator();
        let start = iss_epoch();
        let transitions = shadow_transitions(
            &prop,
            start,
            start + Duration::hours(3),
            Duration::seconds(30),
        )
        .expect("scan succeeds");

        // ~2 orbits => at least 2 full eclipse cycles of 4 boundary crossings each.
        assert!(
            transitions.len() >= 8,
            "expected at least 8 crossings in 3 h, got {}",
            transitions.len()
        );

        for pair in transitions.windows(2) {
            assert!(pair[0].time < pair[1].time, "transitions must be ordered");
            // Chained: each crossing starts from the regime the previous one ended in.
            assert_eq!(pair[0].to, pair[1].from, "regime chain must be continuous");
        }
        for t in &transitions {
            // Adjacent regimes only: sunlit↔penumbra or penumbra↔umbra, never sunlit↔umbra.
            assert_eq!(
                depth(t.from).abs_diff(depth(t.to)),
                1,
                "transition must cross exactly one boundary: {} → {}",
                t.from,
                t.to
            );
        }
    }

    #[test]
    fn iss_penumbra_crossings_last_seconds_not_minutes() {
        let prop = iss_propagator();
        let start = iss_epoch();
        let transitions = shadow_transitions(
            &prop,
            start,
            start + Duration::hours(3),
            Duration::seconds(30),
        )
        .expect("scan succeeds");

        // Every stay in penumbra (between consecutive transitions into and out of it)
        // should be seconds-scale for LEO — this is what the coarse 30 s scan would
        // miss entirely without the per-boundary refinement.
        for pair in transitions.windows(2) {
            if pair[0].to == ShadowState::Penumbra {
                let dwell = pair[1].time - pair[0].time;
                assert!(
                    dwell < Duration::seconds(60),
                    "LEO penumbra dwell should be < 60 s, got {dwell}"
                );
                assert!(
                    dwell > Duration::milliseconds(500),
                    "penumbra dwell should be physically nonzero, got {dwell}"
                );
            }
        }
    }

    #[test]
    fn transitions_empty_window_and_bad_step() {
        let prop = iss_propagator();
        let start = iss_epoch();
        assert!(
            shadow_transitions(&prop, start, start, Duration::seconds(30))
                .expect("empty window is ok")
                .is_empty()
        );
        assert!(shadow_transitions(&prop, start, start + Duration::hours(1), Duration::zero()).is_err());
    }

    #[test]
    fn shadow_state_types_are_send_sync_and_serializable() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ShadowState>();
        assert_send_sync::<ShadowTransition>();

        let json = serde_json::to_string(&ShadowState::Penumbra).unwrap();
        assert_eq!(json, "\"penumbra\"");
    }
}
