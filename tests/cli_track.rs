//! CLI integration tests for the `track` command: human and JSON formats, modes, TLE sources,
//! optional pass prediction and ground-track CSV, and teaching-oriented parse errors.
//! By default eight tests run (`--tle-url` is absent without the `network` feature); with
//! `cargo test --features network`, two additional tests cover the URL stub path.

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
fn track_tle_url_only_errors_with_placeholder_message() {
    let output = Command::new(binary())
        .args([
            "track",
            "--tle-url",
            "https://celestrak.org/NORAD/elements/stations.txt",
        ])
        .output()
        .expect("the ephemerust binary should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not implemented"),
        "expected placeholder message; stderr was:\n{stderr}"
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
