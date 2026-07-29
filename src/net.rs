//! Bounded HTTPS retrieval of element-set text (the `network` feature).
//!
//! This module exists so the `track` CLI flag `--tle-url` (and any library consumer that
//! wants live element sets) can fetch a CelesTrak-style bulletin **safely**: bounded in
//! time, bounded in size, HTTPS-only, and identifying itself. It follows the engineering
//! plan in `http_plan.md` and CelesTrak's published operational expectations, which this
//! module treats as requirements, not suggestions:
//!
//! - **Do not poll.** Public GP data refreshes roughly every **two hours**; re-fetching the
//!   same resource faster than that gains nothing and contributes to the provider's
//!   documented IP-blocking thresholds. Ephemerust performs **on-demand fetch only** — if
//!   you wrap it in automation, cache the last successful response and check its age first.
//! - **Do not retry on error.** A `403`, `404`, or `5xx` will not fix itself seconds later;
//!   [`fetch_tle_text`] returns a structured error and never loops.
//! - **Request the smallest resource** that contains your object (e.g. the `stations`
//!   bulletin for the ISS, not the full catalog).
//!
//! The fetched text usually contains **many** element sets; pair this module with
//! [`crate::satellite::select_tle`] to pick one:
//!
//! ```no_run
//! use ephemerust::net::fetch_tle_text;
//! use ephemerust::satellite::select_tle;
//!
//! let text = fetch_tle_text("https://celestrak.org/NORAD/elements/gp.php?GROUP=stations&FORMAT=tle")?;
//! let iss = select_tle(&text, Some("ISS (ZARYA)"))?;
//! println!("{} epoch: {}", iss.name.as_deref().unwrap_or("?"), iss.epoch);
//! # Ok::<(), ephemerust::AstroError>(())
//! ```
//!
//! ## Security posture
//!
//! The URL is user-controlled input, so the client enforces:
//!
//! - **HTTPS only**, with one documented exception: plain `http://` is accepted for
//!   **loopback** hosts (`localhost`, `127.x.x.x`, `[::1]`) so tests and local fixture
//!   servers work offline. `file:` and every other scheme are rejected.
//! - A **connect timeout** (10 s), an **overall timeout** (30 s), and a **response size
//!   cap** ([`MAX_RESPONSE_BYTES`], 2 MiB) — a misbehaving or malicious server cannot hang
//!   the process or balloon its memory.
//! - Redirects are followed (CelesTrak's legacy `.com` hosts answer `301`), but at most 5.
//!
//! Pointing `--tle-url` at untrusted internal services is still an operator decision;
//! Ephemerust cannot know your network topology (see `http_plan.md` §3.4).

use crate::{AstroError, Result};

/// Maximum accepted response body, in bytes (2 MiB).
///
/// Sized generously above the largest common per-group CelesTrak bulletins while refusing
/// pathological responses. Fetching an entire multi-megabyte catalog is deliberately out of
/// scope — request a smaller group instead.
pub const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

/// Connect timeout for the HTTP client.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Overall (connect + transfer) timeout for one request.
const OVERALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// `true` when `host` is a loopback address, for which plain HTTP is permitted.
fn is_loopback_host(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.eq_ignore_ascii_case("localhost") {
        return true;
    }
    bare.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// Validates the URL scheme policy: HTTPS anywhere, plain HTTP only toward loopback.
fn check_url_policy(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        // Strip userinfo and port to isolate the host.
        let host_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = if host_port.starts_with('[') {
            host_port.split(']').next().unwrap_or(host_port)
        } else {
            host_port.split(':').next().unwrap_or(host_port)
        };
        if is_loopback_host(host) {
            return Ok(());
        }
        return Err(AstroError::SatelliteError(format!(
            "refusing plain-HTTP URL to non-loopback host \"{host}\": use https:// \
             (unencrypted element sets can be tampered with in transit)"
        )));
    }
    Err(AstroError::SatelliteError(format!(
        "unsupported URL scheme in \"{url}\": only https:// is accepted \
         (plus http:// toward localhost for testing)"
    )))
}

/// Fetches element-set text from `url` with the bounded, CelesTrak-respecting client.
///
/// Performs a single GET — **no retries** — with the timeouts, size cap, redirect limit,
/// and scheme policy described in the [module docs](self), sending a descriptive
/// `User-Agent` that identifies Ephemerust and its repository. The body is decoded as
/// UTF-8 and returned untouched; pass it to [`crate::satellite::select_tle`] (bulletins
/// with many objects) or [`crate::satellite::Tle::parse`] (exactly one object).
///
/// # Errors
///
/// [`AstroError::SatelliteError`] describing exactly which policy or transport step failed:
/// scheme rejection, connect/transfer timeout, a non-success HTTP status (reported with its
/// code — do **not** simply retry; see the module docs), an oversized body, or invalid
/// UTF-8.
pub fn fetch_tle_text(url: &str) -> Result<String> {
    check_url_policy(url)?;

    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(OVERALL_TIMEOUT))
        .max_redirects(5)
        .user_agent(concat!(
            "ephemerust/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/IsomorphicAlgo/ephemerust)"
        ))
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let mut response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => AstroError::SatelliteError(format!(
            "the server answered HTTP {code} for \"{url}\"; check the URL, and do not \
             retry in a loop — repeated errors trigger provider-side blocking"
        )),
        other => AstroError::SatelliteError(format!(
            "could not fetch \"{url}\": {other} (single attempt, no retries)"
        )),
    })?;

    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| {
            AstroError::SatelliteError(format!(
                "could not read the response body from \"{url}\" \
                 (limit {MAX_RESPONSE_BYTES} bytes, UTF-8 required): {e}"
            ))
        })?;

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_is_always_allowed_by_policy() {
        assert!(check_url_policy("https://celestrak.org/NORAD/elements/stations.txt").is_ok());
        assert!(check_url_policy("HTTPS://celestrak.org/x").is_ok());
    }

    #[test]
    fn plain_http_is_loopback_only() {
        assert!(check_url_policy("http://127.0.0.1:8080/tle.txt").is_ok());
        assert!(check_url_policy("http://localhost/tle.txt").is_ok());
        assert!(check_url_policy("http://[::1]:9999/tle.txt").is_ok());

        assert!(check_url_policy("http://celestrak.org/stations.txt").is_err());
        assert!(check_url_policy("http://192.168.1.10/tle.txt").is_err());
    }

    #[test]
    fn other_schemes_are_rejected() {
        assert!(check_url_policy("file:///C:/tle.txt").is_err());
        assert!(check_url_policy("ftp://celestrak.org/x").is_err());
        assert!(check_url_policy("stations.txt").is_err());
    }
}
