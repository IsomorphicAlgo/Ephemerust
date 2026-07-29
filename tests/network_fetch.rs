//! Offline integration tests for the `network` feature (`--features network`).
//!
//! No test here touches the real network: a minimal HTTP/1.1 fixture server on a loopback
//! port stands in for CelesTrak (the scheme policy explicitly permits plain HTTP toward
//! loopback for exactly this purpose). Covered: the success path into `select_tle`, redirect
//! following, non-success statuses (single attempt, no retries), the response size cap, the
//! HTTPS-only policy for remote hosts, and the CLI `--tle-url` / `--tle-name` end-to-end path.
#![cfg(feature = "network")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use ephemerust::net::fetch_tle_text;
use ephemerust::satellite::select_tle;

const ISS_NAME: &str = "ISS (ZARYA)";
const ISS_LINE1: &str = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992";
const ISS_LINE2: &str = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

// A second, fictional element set (valid checksums) so the bulletin is genuinely multi-object.
const NAUKA_NAME: &str = "NAUKA";
const NAUKA_LINE1: &str = "1 49044U 21066A   21198.53000000  .00001000  00000-0  30000-4 0  9993";
const NAUKA_LINE2: &str = "2 49044  51.6400 200.0000 0002000  90.0000 270.0000 15.48000000    10";

fn stations_body() -> String {
    format!(
        "{ISS_NAME}\r\n{ISS_LINE1}\r\n{ISS_LINE2}\r\n{NAUKA_NAME}\r\n{NAUKA_LINE1}\r\n{NAUKA_LINE2}\r\n"
    )
}

/// Spawns a one-shot-per-connection HTTP/1.1 fixture server on an ephemeral loopback port.
///
/// Returns the `http://127.0.0.1:<port>` base URL and a counter of requests served, so tests
/// can assert the "no retries" contract.
fn spawn_fixture_server() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let base = format!("http://{}", listener.local_addr().expect("local addr"));
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_in_thread = Arc::clone(&hits);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            hits_in_thread.fetch_add(1, Ordering::SeqCst);
            serve_one(&mut stream);
        }
    });
    (base, hits)
}

fn serve_one(stream: &mut TcpStream) {
    // Read until the end of the request headers.
    let mut request = Vec::new();
    let mut buf = [0u8; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    let request = String::from_utf8_lossy(&request);
    let path = request.split_whitespace().nth(1).unwrap_or("/");

    match path {
        "/stations.txt" => respond(stream, "200 OK", &stations_body(), &[]),
        "/redirect" => respond(
            stream,
            "301 Moved Permanently",
            "",
            &[("Location", "/stations.txt")],
        ),
        "/huge" => {
            // One kilobyte past the client's documented 2 MiB cap.
            let body = "A".repeat((ephemerust::net::MAX_RESPONSE_BYTES + 1024) as usize);
            respond(stream, "200 OK", &body, &[]);
        }
        _ => respond(stream, "404 Not Found", "no such bulletin\n", &[]),
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str, extra_headers: &[(&str, &str)]) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

#[test]
fn fetches_bulletin_and_selects_objects() {
    let (base, _) = spawn_fixture_server();

    let text = fetch_tle_text(&format!("{base}/stations.txt")).expect("fetch succeeds");
    assert!(text.contains(ISS_NAME), "body should round-trip:\n{text}");

    let iss = select_tle(&text, Some("zarya")).expect("name selection");
    assert_eq!(iss.catalog_number, 25544);

    let nauka = select_tle(&text, Some("49044")).expect("catalog-number selection");
    assert_eq!(nauka.name.as_deref(), Some(NAUKA_NAME));
}

#[test]
fn http_error_status_is_reported_without_retry() {
    let (base, hits) = spawn_fixture_server();

    let err = fetch_tle_text(&format!("{base}/missing.txt")).expect_err("404 must error");
    let msg = err.to_string();
    assert!(msg.contains("404"), "status code should be surfaced: {msg}");
    assert!(
        msg.contains("do not") && msg.contains("retry"),
        "error should teach the no-retry rule: {msg}"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "exactly one request: errors must not trigger retries"
    );
}

#[test]
fn redirects_are_followed() {
    let (base, hits) = spawn_fixture_server();

    let text = fetch_tle_text(&format!("{base}/redirect")).expect("301 should be followed");
    assert!(text.contains(ISS_NAME));
    assert_eq!(hits.load(Ordering::SeqCst), 2, "redirect + target fetch");
}

#[test]
fn oversized_body_is_rejected() {
    let (base, _) = spawn_fixture_server();

    let err = fetch_tle_text(&format!("{base}/huge")).expect_err("2 MiB cap must trip");
    let msg = err.to_string();
    assert!(
        msg.contains("limit") || msg.contains("body"),
        "error should mention the size limit: {msg}"
    );
}

#[test]
fn remote_plain_http_is_refused_before_connecting() {
    // 192.0.2.0/24 is TEST-NET-1: if the policy check failed, the connect would hang and
    // this test would take the full connect timeout instead of returning instantly.
    let started = std::time::Instant::now();
    let err = fetch_tle_text("http://192.0.2.1/stations.txt").expect_err("policy must refuse");
    assert!(err.to_string().contains("https"), "{err}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "the refusal must happen before any connection attempt"
    );
}

/// Path to the binary under test, provided by Cargo to integration tests.
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ephemerust")
}

#[test]
fn cli_tle_url_with_tle_name_tracks_selected_object() {
    let (base, _) = spawn_fixture_server();

    let output = std::process::Command::new(binary())
        .args([
            "track",
            "--tle-url",
            &format!("{base}/stations.txt"),
            "--tle-name",
            "zarya",
            "--mode",
            "tle",
            "--format",
            "json",
        ])
        .output()
        .expect("the ephemerust binary should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr was:\n{stderr}");
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("JSON output");
    assert_eq!(v["tle"]["catalog_number"], 25544);
}

#[test]
fn cli_multi_object_url_without_tle_name_lists_available_objects() {
    let (base, _) = spawn_fixture_server();

    let output = std::process::Command::new(binary())
        .args(["track", "--tle-url", &format!("{base}/stations.txt")])
        .output()
        .expect("the ephemerust binary should run");

    assert!(!output.status.success(), "ambiguous bulletin must error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("select one") && stderr.contains(ISS_NAME),
        "error should list the available objects; stderr was:\n{stderr}"
    );
}
