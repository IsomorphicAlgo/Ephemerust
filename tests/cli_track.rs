//! CLI integration tests for the `track` command: successful output includes sub-satellite,
//! look angles, and optional pass prediction; malformed input yields teaching-oriented errors.

use std::process::Command;

/// Path to the binary under test, provided by Cargo to integration tests.
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ephemerust")
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
    assert!(stderr.contains("Error:"), "missing error line; stderr was:\n{stderr}");
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

    assert!(!output.status.success(), "a bad checksum must yield a non-zero exit code");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("modulo-10"),
        "checksum error should teach the modulo-10 rule; stderr was:\n{stderr}"
    );
}
