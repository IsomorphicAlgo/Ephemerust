//! CLI integration tests for the `track` command: human and JSON formats, modes, TLE sources,
//! optional pass prediction and ground-track CSV, and teaching-oriented parse errors.
//! The same command accepts an OMM (JSON, KVN, XML or CSV) through the same flags or their
//! `--omm-file` / `--omm` / `--name` aliases; the OMM tests at the end of this file cover that.
//! `--tle-url` is absent without the `network` feature; with `cargo test --features network`,
//! two additional tests here cover URL-flag policy and source exclusivity, and
//! `tests/network_fetch.rs` exercises the fetch path offline.
//! The whole file needs the binary, so it compiles only with the (default) `cli` feature.

#![cfg(feature = "cli")]

use std::process::Command;

/// Path to the binary under test, provided by Cargo to integration tests.
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ephemerust")
}

#[cfg(not(feature = "network"))]
#[test]
fn track_rejects_unknown_tle_url_flag_without_network_feature() {
    let output = Command::new(binary())
        .args([
            "track",
            "--tle-url",
            "https://celestrak.org/NORAD/elements/stations.txt",
        ])
        .output()
        .expect("the ephemerust binary should run");

    assert!(
        !output.status.success(),
        "unexpected --tle-url should fail when built without `network`"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lower = stderr.to_lowercase();
    assert!(
        lower.contains("unexpected") && lower.contains("tle-url"),
        "clap should reject unknown flag; stderr was:\n{stderr}"
    );
}

#[test]
fn track_requires_exactly_one_tle_source() {
    let output = Command::new(binary())
        .arg("track")
        .output()
        .expect("the ephemerust binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("exactly one of") || stderr.contains("provide exactly"),
        "expected TLE source hint; stderr was:\n{stderr}"
    );
}

#[test]
fn track_format_json_all_is_valid_json_with_expected_keys() {
    let tle = "ISS (ZARYA)\n\
         1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";
    let output = Command::new(binary())
        .args(["track", "--tle", tle, "--format", "json"])
        .output()
        .expect("the ephemerust binary should run");

    assert!(output.status.success(), "valid TLE should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout must be JSON");
    assert_eq!(v["tle"]["catalog_number"], 25544);
    assert!(v.get("state").is_some());
    assert!(v.get("subpoint").is_some());
    assert!(v.get("look_angles").is_some());
    assert!(
        !stdout.contains("Object:"),
        "JSON mode should not emit human TLE banner; got:\n{stdout}"
    );
}

#[cfg(feature = "network")]
#[test]
fn track_tle_url_refuses_plain_http_to_remote_hosts() {
    // Scheme policy is checked before any connection is attempted, so this stays offline.
    let output = Command::new(binary())
        .args([
            "track",
            "--tle-url",
            "http://celestrak.org/NORAD/elements/stations.txt",
        ])
        .output()
        .expect("the ephemerust binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("https"),
        "expected HTTPS-only policy message; stderr was:\n{stderr}"
    );
}

#[cfg(feature = "network")]
#[test]
fn track_conflicting_tle_sources_error() {
    let tle = "ISS (ZARYA)\n\
         1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";
    let output = Command::new(binary())
        .args([
            "track",
            "--tle",
            tle,
            "--tle-url",
            "https://example.com/nope.txt",
        ])
        .output()
        .expect("the ephemerust binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("only one of"),
        "expected exclusivity error; stderr was:\n{stderr}"
    );
}

#[test]
fn valid_tle_with_predict_passes_prints_pass_section() {
    let tle = "ISS (ZARYA)\n\
         1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";
    let output = Command::new(binary())
        .args(["track", "--tle", tle, "--predict-passes-hours", "48"])
        .output()
        .expect("the ephemerust binary should run");

    assert!(output.status.success(), "valid TLE should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Predicted passes"),
        "expected pass prediction section; stdout was:\n{stdout}"
    );
}

#[test]
fn valid_tle_with_ground_track_prints_csv_header() {
    let tle = "ISS (ZARYA)\n\
         1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";
    let output = Command::new(binary())
        .args([
            "track",
            "--tle",
            tle,
            "--ground-track-hours",
            "1",
            "--ground-track-step-sec",
            "600",
        ])
        .output()
        .expect("the ephemerust binary should run");

    assert!(output.status.success(), "valid TLE should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Ground track (CSV") && stdout.contains("time_utc,latitude_deg"),
        "expected ground track CSV section; stdout was:\n{stdout}"
    );
}

#[test]
fn valid_tle_prints_sub_satellite_section() {
    let tle = "ISS (ZARYA)\n\
         1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n\
         2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";
    let output = Command::new(binary())
        .args(["track", "--tle", tle])
        .output()
        .expect("the ephemerust binary should run");

    assert!(output.status.success(), "valid TLE should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Sub-satellite point"),
        "expected sub-satellite section; stdout was:\n{stdout}"
    );
    assert!(stdout.contains("Latitude:"), "stdout was:\n{stdout}");
    assert!(
        stdout.contains("Look angles"),
        "expected look angles section; stdout was:\n{stdout}"
    );
}

#[test]
fn malformed_tle_prints_educational_error_and_hint() {
    // Line 1 is far shorter than the required 69 columns.
    let output = Command::new(binary())
        .args(["track", "--tle", "1 25544U\n2 25544"])
        .output()
        .expect("the ephemerust binary should run");

    assert!(
        !output.status.success(),
        "a malformed TLE must yield a non-zero exit code"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error:"),
        "missing error line; stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("69 are required"),
        "error should explain the 69-column rule; stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("Hint:"),
        "error should be followed by a corrective hint; stderr was:\n{stderr}"
    );
}

#[test]
fn checksum_error_explains_the_rule() {
    // A valid-length line 1 with a deliberately wrong final check digit (…9992 → …9990).
    let line1 = "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9990";
    let line2 = "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";
    let tle = format!("{line1}\n{line2}");

    let output = Command::new(binary())
        .args(["track", "--tle", &tle])
        .output()
        .expect("the ephemerust binary should run");

    assert!(
        !output.status.success(),
        "a bad checksum must yield a non-zero exit code"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("modulo-10"),
        "checksum error should teach the modulo-10 rule; stderr was:\n{stderr}"
    );
}

// --- OMM input ----------------------------------------------------------------------------

/// Directory holding the OMM fixtures (see `tests/omm_reference.rs` for what they are).
fn omm_fixture(name: &str) -> String {
    format!("{}/tests/data/omm/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn run_track(args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .arg("track")
        .args(args)
        .output()
        .expect("the ephemerust binary should run")
}

#[test]
fn omm_file_is_detected_and_tracked() {
    for (file, label) in [("mario.xml", "OMM (XML)"), ("mario.csv", "OMM (CSV)")] {
        let output = run_track(&["--omm-file", &omm_fixture(file)]);
        assert!(output.status.success(), "{file} should exit 0");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(label), "{file}: stdout was:\n{stdout}");
        assert!(
            stdout.contains("Object:        MARIO"),
            "stdout was:\n{stdout}"
        );
        assert!(
            stdout.contains("Catalog #:     55123 (U)"),
            "stdout was:\n{stdout}"
        );
        assert!(
            stdout.contains("Sub-satellite point"),
            "stdout was:\n{stdout}"
        );
        assert!(stdout.contains("Look angles"), "stdout was:\n{stdout}");
    }
}

#[test]
fn omm_and_tle_of_the_same_element_set_give_the_same_state() {
    let tle = "MARIO\n\
         1 55123U 98067UQ  23115.44827133  .00787702  29408-3  15680-2 0  9999\n\
         2 55123  51.6242 216.2930 0014649 331.8976  28.1241 15.99081912 18396";
    let from_tle = run_track(&["--tle", tle, "--format", "json"]);
    // The original `-f` flag accepts an OMM too: the format comes from the content.
    let from_omm = run_track(&["-f", &omm_fixture("mario.csv"), "--format", "json"]);
    assert!(from_tle.status.success() && from_omm.status.success());

    let a: serde_json::Value = serde_json::from_slice(&from_tle.stdout).expect("TLE JSON");
    let b: serde_json::Value = serde_json::from_slice(&from_omm.stdout).expect("OMM JSON");
    assert_eq!(a["element_set_format"], "tle");
    assert_eq!(b["element_set_format"], "omm");
    assert_eq!(a["tle"]["catalog_number"], b["tle"]["catalog_number"]);
    // The designator is reported as each format writes it.
    assert_eq!(a["tle"]["international_designator"], "98067UQ");
    assert_eq!(b["tle"]["international_designator"], "1998-067UQ");

    // Each state is taken at its own epoch; the epochs agree to a microsecond, so the
    // positions agree to centimetres.
    for axis in 0..3 {
        let pa = a["state"]["position_km"][axis].as_f64().expect("number");
        let pb = b["state"]["position_km"][axis].as_f64().expect("number");
        assert!((pa - pb).abs() < 1e-4, "axis {axis}: {pa} vs {pb}");
    }
}

#[test]
fn omm_objects_beyond_the_tle_catalog_limit_are_selected_by_number() {
    let file = omm_fixture("high_catalog.json");
    for (selector, name) in [
        ("100123", "EXAMPLE-LEO"),
        ("340000", "EXAMPLE-MOLNIYA"),
        ("799500123", "EXAMPLE-GEO"),
    ] {
        let output = run_track(&["-f", &file, "--name", selector, "--format", "json"]);
        assert!(output.status.success(), "selector {selector} should exit 0");
        let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
        assert_eq!(v["tle"]["name"], name);
        assert_eq!(v["tle"]["catalog_number"].to_string(), selector);
        assert!(v["look_angles"]["range_km"].as_f64().expect("range") > 100.0);
    }

    // `--tle-name` is the same option under its original name, and matches names too.
    let output = run_track(&["-f", &file, "--tle-name", "molniya", "--mode", "tle"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Catalog #:     340000 (U)"),
        "stdout was:\n{stdout}"
    );
}

#[test]
fn omm_passes_are_predicted_from_a_json_file() {
    let output = run_track(&[
        "-f",
        &omm_fixture("high_catalog.json"),
        "--name",
        "leo",
        "--format",
        "json",
        "--mode",
        "passes",
        "--predict-passes-hours",
        "24",
    ]);
    assert!(output.status.success());
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    let passes = v["passes"].as_array().expect("passes array");
    assert!(
        !passes.is_empty(),
        "a day of a polar orbit has passes over 48° N"
    );
    for pass in passes {
        assert!(pass["max_elevation_deg"].as_f64().expect("number") >= 10.0);
    }
}

#[test]
fn omm_with_several_objects_and_no_selector_lists_them() {
    let output = run_track(&["-f", &omm_fixture("high_catalog.json")]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("3 OMM messages"), "stderr was:\n{stderr}");
    for name in ["EXAMPLE-LEO", "EXAMPLE-MOLNIYA", "EXAMPLE-GEO"] {
        assert!(stderr.contains(name), "stderr was:\n{stderr}");
    }
}

#[test]
fn malformed_omm_prints_educational_error_and_hint() {
    // Inline KVN that stops after the epoch: the mean elements are missing.
    let output = run_track(&[
        "--omm",
        "CCSDS_OMM_VERS = 2.0\nOBJECT_NAME = INCOMPLETE\nEPOCH = 2026-08-01T12:00:00\n",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error:"), "stderr was:\n{stderr}");
    assert!(stderr.contains("MEAN_MOTION"), "stderr was:\n{stderr}");
    assert!(stderr.contains("Hint:"), "stderr was:\n{stderr}");

    // A TLE-style eccentricity pasted into an OMM is caught as out of range.
    let csv = std::fs::read_to_string(omm_fixture("mario.csv")).expect("fixture");
    let output = run_track(&["--omm", &csv.replace(",.0014649,", ",0014649,")]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ECCENTRICITY"), "stderr was:\n{stderr}");
    assert!(stderr.contains("out of range"), "stderr was:\n{stderr}");
}

#[test]
fn omm_summary_mode_reports_only_the_message() {
    // Summary mode reports the parsed message and nothing that needs propagation.
    let csv = std::fs::read_to_string(omm_fixture("mario.csv")).expect("fixture");
    let output = run_track(&["--omm", &csv, "--mode", "tle", "--format", "json"]);
    assert!(output.status.success());
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(v["tle"]["revolution_number"], 1839);
    assert!(v.get("state").is_none());
}
