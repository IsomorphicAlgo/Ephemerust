//! Command-line front end for the `ephemerust` library (built with the default `cli` feature).

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand, ValueEnum};
use ephemerust::Result;
use serde::Serialize;

/// Ephemerust — an astronomy, orbital-mechanics, and satellite-tracking toolkit in Rust
#[derive(Parser)]
#[command(name = "ephemerust")]
#[command(about = "Astronomy, orbital-mechanics, and satellite-tracking toolkit, written in Rust")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    Convert {
        #[arg(short, long)]
        from: String,
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        coords: String,
        /// Greenwich Mean Sidereal Time in hours (0-24). If not provided, calculated from current time.
        #[arg(long)]
        gmst: Option<f64>,
    },
    RiseSet {
        #[arg(short = 'j', long)]
        object: String,
        #[arg(short = 'a', long)]
        latitude: f64,
        #[arg(short = 'o', long)]
        longitude: f64,
        #[arg(short, long)]
        date: Option<String>,
    },
    Position {
        #[arg(short, long)]
        object: String,
        #[arg(short, long)]
        date: String,
    },
    Time {
        #[arg(short, long)]
        date: String,
        #[arg(short, long)]
        time: Option<String>,
    },
    Orbital {
        /// Semi-major axis in km
        #[arg(short, long)]
        semi_major: f64,
        /// Orbital eccentricity (0-1)
        #[arg(short, long)]
        eccentricity: f64,
        /// Inclination in degrees
        #[arg(short, long)]
        inclination: f64,
        /// Longitude of ascending node (Ω) in degrees
        #[arg(long, default_value_t = 0.0)]
        raan: f64,
        /// Argument of periapsis (ω) in degrees
        #[arg(long = "arg-periapsis", default_value_t = 0.0)]
        arg_periapsis: f64,
        /// Mean anomaly (M) in degrees
        #[arg(long, default_value_t = 0.0)]
        mean_anomaly: f64,
        /// Standard gravitational parameter μ in km³/s² (default: Earth)
        #[arg(long, default_value_t = 398600.4418)]
        mu: f64,
    },
    /// Satellite tracking from a TLE or an OMM (see docs/satellite-tracking-plan.md, docs/omm.md)
    Track {
        /// Path to a file containing a TLE (2- or 3-line element set) or an OMM (JSON, KVN,
        /// XML or CSV); the format is detected from the content
        #[arg(short = 'f', long, visible_alias = "omm-file")]
        tle_file: Option<String>,
        /// Inline TLE text (quote the two/three lines; preserve line breaks) or inline OMM text
        #[arg(short = 't', long, visible_alias = "omm")]
        tle: Option<String>,
        /// Fetch element-set text (TLE or OMM) from this HTTPS URL (e.g. a CelesTrak query;
        /// requires `--features network`). Bounded fetch: no retries — respect provider rate
        /// guidance.
        #[cfg_attr(not(feature = "network"), arg(skip))]
        #[cfg_attr(feature = "network", arg(long = "tle-url", visible_alias = "omm-url"))]
        tle_url: Option<String>,
        /// Select one object from a multi-object source by case-insensitive name substring,
        /// or by exact NORAD catalog number (e.g. `--tle-name zarya` or `--tle-name 25544`)
        #[arg(long = "tle-name", visible_alias = "name")]
        tle_name: Option<String>,
        /// What to print: `all` (default), `tle` summary only, `state`, `subpoint`, `look`,
        /// `passes` (requires `--predict-passes-hours` > 0), or `ground` (requires `--ground-track-hours` > 0)
        #[arg(long, value_enum, default_value_t = TrackMode::All)]
        mode: TrackMode,
        /// Human-readable text or a single JSON document on stdout
        #[arg(long, value_enum, default_value_t = TrackFormat::Human)]
        format: TrackFormat,
        /// Observer latitude in degrees (default: Everett, WA — 47.9088° N)
        #[arg(short = 'a', long)]
        latitude: Option<f64>,
        /// Observer longitude in degrees, positive east (default: −122.2503°)
        #[arg(short = 'o', long)]
        longitude: Option<f64>,
        /// If > 0, list predicted passes for this many hours starting at the element-set epoch
        #[arg(long, default_value_t = 0_u32)]
        predict_passes_hours: u32,
        /// Minimum elevation in degrees when `--predict-passes-hours` is used
        #[arg(long, default_value_t = 10.0)]
        pass_min_elevation_deg: f64,
        /// If > 0, emit a ground track from the element-set epoch for this many hours
        #[arg(long, default_value_t = 0_u32)]
        ground_track_hours: u32,
        /// Sample interval in seconds for `--ground-track-hours` (default 60)
        #[arg(long, default_value_t = 60_u64)]
        ground_track_step_sec: u64,
        /// With `--format human` and a ground track, emit JSON array instead of CSV
        #[arg(long, default_value_t = false)]
        ground_track_json: bool,
        /// Use WGS72 + AFSPC sidereal/epoch propagation (Vallado / legacy NORAD reference)
        #[arg(long, default_value_t = false)]
        afspc: bool,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Default)]
enum TrackMode {
    /// Element-set summary, state, subpoint, look angles, and optional passes / ground track
    #[default]
    All,
    /// Parsed element-set metadata only (TLE or OMM)
    Tle,
    /// TEME position and velocity at element-set epoch
    State,
    /// Sub-satellite geodetic point at epoch
    Subpoint,
    /// Topocentric look angles at epoch for the observer
    Look,
    /// Pass list only (`--predict-passes-hours` must be > 0)
    Passes,
    /// Ground-track samples only (`--ground-track-hours` must be > 0)
    Ground,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Default)]
enum TrackFormat {
    #[default]
    Human,
    /// Single JSON document on stdout (RFC 8259)
    Json,
}

fn main() {
    if let Err(err) = run() {
        // Present errors as legible, teaching-oriented text on stderr (rather than the default
        // `Debug` rendering) and exit non-zero. Where the error knows how the input should be
        // shaped, a `Hint:` line follows with the correction.
        eprintln!("Error: {err}");
        if let Some(hint) = err.hint() {
            eprintln!("Hint:  {hint}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match cli.command {
        Commands::Convert {
            from,
            to,
            coords,
            gmst,
        } => match (from.to_lowercase().as_str(), to.to_lowercase().as_str()) {
            ("ra-dec" | "radec", "alt-az" | "altaz") => {
                let result = parse_and_convert_radec_to_altaz(&coords)?;
                let (alt_deg, alt_min, alt_sec, alt_sign) = format_angle(result.alt);
                let (az_deg, az_min, az_sec, _) = format_angle(result.az);
                println!(
                    "Alt: {}{:02}°{:02}'{:02}\"",
                    alt_sign, alt_deg, alt_min, alt_sec
                );
                println!("Az:  {:03}°{:02}'{:02}\"", az_deg, az_min, az_sec);
            }
            ("alt-az" | "altaz", "ra-dec" | "radec") => {
                let result = parse_and_convert_altaz_to_radec(&coords)?;
                let (ra_h, ra_m, ra_s) = format_time(result.ra);
                let (dec_deg, dec_min, dec_sec, dec_sign) = format_angle(result.dec);
                println!("RA:  {:02}:{:02}:{:02}", ra_h, ra_m, ra_s);
                println!(
                    "Dec: {}{:02}°{:02}'{:02}\"",
                    dec_sign, dec_deg, dec_min, dec_sec
                );
            }
            ("ecef", "eci") => {
                let result = parse_and_convert_ecef_to_eci(&coords, gmst)?;
                println!("X: {:.3} m", result.x);
                println!("Y: {:.3} m", result.y);
                println!("Z: {:.3} m", result.z);
            }
            ("eci", "ecef") => {
                let result = parse_and_convert_eci_to_ecef(&coords, gmst)?;
                println!("X: {:.3} m", result.x);
                println!("Y: {:.3} m", result.y);
                println!("Z: {:.3} m", result.z);
            }
            _ => {
                return Err(ephemerust::AstroError::InvalidCoordinate(format!(
                    "Unsupported conversion: {} to {}",
                    from, to
                )));
            }
        },
        Commands::RiseSet {
            object,
            latitude,
            longitude,
            date,
        } => {
            let date_time = if let Some(date_str) = date {
                parse_date_time(&date_str, None)?
            } else {
                chrono::Utc::now()
            };

            let location = ephemerust::celestial::ObserverLocation {
                latitude,
                longitude,
                elevation: 0.0,
            };

            let obj = parse_celestial_object(&object)?;

            let rise_set =
                ephemerust::celestial::calculate_rise_set_times(obj, location, date_time)?;

            match rise_set.rise {
                Some(t) => println!("Rise: {}", t.format("%H:%M:%S UTC")),
                None => println!("Rise: Does not rise"),
            }
            match rise_set.set {
                Some(t) => println!("Set:  {}", t.format("%H:%M:%S UTC")),
                None => println!("Set:  Does not set"),
            }
        }
        Commands::Position { object, date } => {
            let date_time = parse_date_time(&date, None)?;
            let obj = parse_celestial_object(&object)?;

            let pos = ephemerust::celestial::calculate_position(obj, date_time)?;
            let (ra_h, ra_m, ra_s) = format_time(pos.ra);
            let (dec_deg, dec_min, dec_sec, dec_sign) = format_angle(pos.dec);

            println!("RA:  {:02}:{:02}:{:02}", ra_h, ra_m, ra_s);
            println!(
                "Dec: {}{:02}°{:02}'{:02}\"",
                dec_sign, dec_deg, dec_min, dec_sec
            );
        }
        Commands::Time { date, time } => {
            let date_time = parse_date_time(&date, time.as_deref())?;
            let jd = ephemerust::time::julian_date(date_time);
            let gmst = ephemerust::time::greenwich_mean_sidereal_time(jd);
            let (h, m, s) = format_time(gmst);

            println!("JD:   {:.6}", jd);
            println!("GMST: {:02}:{:02}:{:02}", h, m, s);
        }
        Commands::Orbital {
            semi_major,
            eccentricity,
            inclination,
            raan,
            arg_periapsis,
            mean_anomaly,
            mu,
        } => {
            use ephemerust::orbital::{
                OrbitalElements, elements_to_state_vector, mean_to_true_anomaly, orbital_period,
            };

            let elements = OrbitalElements {
                semi_major_axis: semi_major,
                eccentricity,
                inclination,
                longitude_ascending_node: raan,
                argument_periapsis: arg_periapsis,
                mean_anomaly,
            };

            let period_s = orbital_period(semi_major, mu);
            let true_anomaly = mean_to_true_anomaly(mean_anomaly, eccentricity);
            let state = elements_to_state_vector(elements, mu)?;

            println!(
                "Elements: a={} km, e={}, i={}°, Ω={}°, ω={}°, M={}°",
                semi_major, eccentricity, inclination, raan, arg_periapsis, mean_anomaly
            );
            println!("Period:   {:.1} s ({:.2} min)", period_s, period_s / 60.0);
            println!("True anomaly: {:.4}°", true_anomaly);
            println!("State vector (inertial frame):");
            println!(
                "  Position [km]:   x={:.3} y={:.3} z={:.3}",
                state.position[0], state.position[1], state.position[2]
            );
            println!(
                "  Velocity [km/s]: vx={:.6} vy={:.6} vz={:.6}",
                state.velocity[0], state.velocity[1], state.velocity[2]
            );
        }
        Commands::Track {
            tle_file,
            tle,
            tle_url,
            tle_name,
            mode,
            format,
            latitude,
            longitude,
            predict_passes_hours,
            pass_min_elevation_deg,
            ground_track_hours,
            ground_track_step_sec,
            ground_track_json,
            afspc,
        } => run_track(
            tle_file,
            tle,
            tle_url,
            tle_name,
            mode,
            format,
            latitude,
            longitude,
            predict_passes_hours,
            pass_min_elevation_deg,
            ground_track_hours,
            ground_track_step_sec,
            ground_track_json,
            afspc,
        )?,
    }

    Ok(())
}

#[derive(Serialize)]
struct TleSummaryJson {
    name: Option<String>,
    catalog_number: Option<u32>,
    classification: char,
    international_designator: String,
    epoch_rfc3339: String,
    mean_motion_dot: f64,
    mean_motion_ddot: f64,
    bstar: f64,
    element_set_number: u32,
    inclination_deg: f64,
    raan_deg: f64,
    eccentricity: f64,
    arg_perigee_deg: f64,
    mean_anomaly_deg: f64,
    mean_motion: f64,
    revolution_number: u32,
}

#[derive(Serialize)]
struct ObserverJson {
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_m: f64,
}

#[derive(Serialize)]
struct TrackJsonOutput {
    /// `"tle"` or `"omm"`: the format the element set arrived in.
    element_set_format: &'static str,
    /// Element-set summary. The key is `tle` for both formats (it predates OMM support).
    #[serde(skip_serializing_if = "Option::is_none")]
    tle: Option<TleSummaryJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observer: Option<ObserverJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<ephemerust::satellite::TemeState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subpoint: Option<ephemerust::satellite::Subpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    look_angles: Option<ephemerust::satellite::LookAngles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    passes: Option<Vec<ephemerust::satellite::Pass>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ground_track: Option<Vec<ephemerust::satellite::GroundTrackSample>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    predict_passes_hours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pass_min_elevation_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ground_track_hours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ground_track_step_sec: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    propagation_model: Option<ephemerust::satellite::PropagationModel>,
}

fn tle_summary_json(tle: &ephemerust::satellite::Tle) -> TleSummaryJson {
    TleSummaryJson {
        name: tle.name.clone(),
        catalog_number: Some(tle.catalog_number),
        classification: tle.classification,
        international_designator: tle.international_designator.clone(),
        epoch_rfc3339: tle
            .epoch
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        mean_motion_dot: tle.mean_motion_dot,
        mean_motion_ddot: tle.mean_motion_ddot,
        bstar: tle.bstar,
        element_set_number: tle.element_set_number,
        inclination_deg: tle.inclination_deg,
        raan_deg: tle.raan_deg,
        eccentricity: tle.eccentricity,
        arg_perigee_deg: tle.arg_perigee_deg,
        mean_anomaly_deg: tle.mean_anomaly_deg,
        mean_motion: tle.mean_motion,
        revolution_number: tle.revolution_number,
    }
}

fn omm_summary_json(omm: &ephemerust::omm::Omm) -> TleSummaryJson {
    TleSummaryJson {
        name: omm.name.clone(),
        catalog_number: omm.catalog_number,
        classification: omm.classification,
        international_designator: omm.international_designator.clone().unwrap_or_default(),
        epoch_rfc3339: omm
            .epoch
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        mean_motion_dot: omm.mean_motion_dot,
        mean_motion_ddot: omm.mean_motion_ddot,
        bstar: omm.bstar,
        element_set_number: omm.element_set_number,
        inclination_deg: omm.inclination_deg,
        raan_deg: omm.raan_deg,
        eccentricity: omm.eccentricity,
        arg_perigee_deg: omm.arg_perigee_deg,
        mean_anomaly_deg: omm.mean_anomaly_deg,
        mean_motion: omm.mean_motion,
        revolution_number: omm.revolution_number,
    }
}

/// The element set `track` was given, in whichever format it arrived.
///
/// Both formats describe the same SGP4 mean elements, so everything after parsing runs through
/// a [`Propagator`](ephemerust::Propagator) and does not care which one this is.
enum ElementSet {
    Tle(ephemerust::satellite::Tle),
    Omm(ephemerust::omm::Omm, ephemerust::omm::OmmFormat),
}

impl ElementSet {
    fn epoch(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            ElementSet::Tle(tle) => tle.epoch,
            ElementSet::Omm(omm, _) => omm.epoch,
        }
    }

    fn format_label(&self) -> &'static str {
        match self {
            ElementSet::Tle(_) => "tle",
            ElementSet::Omm(..) => "omm",
        }
    }

    /// Initializes SGP4 for this element set.
    fn propagator(
        &self,
        model: ephemerust::satellite::PropagationModel,
    ) -> Result<ephemerust::Propagator> {
        match self {
            ElementSet::Tle(tle) => ephemerust::Propagator::with_model(tle, model),
            ElementSet::Omm(omm, _) => ephemerust::Propagator::from_omm_with_model(omm, model),
        }
    }

    fn summary_json(&self) -> TleSummaryJson {
        match self {
            ElementSet::Tle(tle) => tle_summary_json(tle),
            ElementSet::Omm(omm, _) => omm_summary_json(omm),
        }
    }

    fn print_summary(&self) {
        match self {
            ElementSet::Tle(tle) => print_tle_summary(tle),
            ElementSet::Omm(omm, format) => print_omm_summary(omm, *format),
        }
    }
}

/// Parses element-set text from any `track` source, honoring `--tle-name` selection.
///
/// Text that [`OmmFormat::detect`](ephemerust::omm::OmmFormat::detect) recognises is read as
/// an OMM through [`select_omm`](ephemerust::omm::select_omm), which handles one message or
/// many and lists the available objects when a selector is needed. Everything else is a TLE.
fn parse_element_text(text: &str, selector: Option<&str>) -> Result<ElementSet> {
    if let Some(format) = ephemerust::omm::OmmFormat::detect(text) {
        let omm = ephemerust::omm::select_omm(text, selector)?;
        return Ok(ElementSet::Omm(omm, format));
    }
    parse_tle_text(text, selector).map(ElementSet::Tle)
}

/// Parses TLE text, honoring `--tle-name` selection.
///
/// Single-object text without a selector keeps the exact [`Tle::parse`] behavior (and its
/// teaching-oriented diagnostics). Multi-object bulletins — the norm for URLs and catalog
/// files — go through [`select_tle`](ephemerust::satellite::select_tle), whose errors list
/// the available objects and point at `--tle-name`.
fn parse_tle_text(text: &str, selector: Option<&str>) -> Result<ephemerust::satellite::Tle> {
    let multiple_sets = text
        .lines()
        .filter(|l| l.trim_start().starts_with("2 "))
        .count()
        > 1;
    if selector.is_some() || multiple_sets {
        ephemerust::satellite::select_tle(text, selector)
    } else {
        ephemerust::satellite::Tle::parse(text)
    }
}

fn resolve_tle_input(
    tle_file: Option<String>,
    tle: Option<String>,
    tle_url: Option<String>,
    tle_name: Option<String>,
) -> Result<ElementSet> {
    #[cfg(feature = "network")]
    const TLE_ONE_OF: &str = "provide exactly one of --tle-file, --tle, or --tle-url";
    #[cfg(not(feature = "network"))]
    const TLE_ONE_OF: &str = "provide exactly one of --tle-file or --tle";
    #[cfg(feature = "network")]
    const TLE_ONLY_ONE: &str = "only one of --tle-file, --tle, or --tle-url may be given";
    #[cfg(not(feature = "network"))]
    const TLE_ONLY_ONE: &str = "only one of --tle-file or --tle may be given";

    let n = tle_file.is_some() as u8 + tle.is_some() as u8 + tle_url.is_some() as u8;
    if n == 0 {
        return Err(ephemerust::AstroError::SatelliteError(TLE_ONE_OF.into()));
    }
    if n > 1 {
        return Err(ephemerust::AstroError::SatelliteError(TLE_ONLY_ONE.into()));
    }

    let selector = tle_name.as_deref();
    if let Some(url) = tle_url {
        #[cfg(feature = "network")]
        {
            log::info!("track: fetching element sets from {url}");
            let text = ephemerust::net::fetch_tle_text(&url)?;
            return parse_element_text(&text, selector);
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = url;
            return Err(ephemerust::AstroError::SatelliteError(
                "--tle-url requires a build with `--features network`".into(),
            ));
        }
    }
    match (tle_file, tle) {
        (Some(path), None) => {
            let text = std::fs::read_to_string(&path)?;
            parse_element_text(&text, selector)
        }
        (None, Some(text)) => parse_element_text(&text, selector),
        _ => Err(ephemerust::AstroError::SatelliteError(
            "internal: TLE source resolution".into(),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_track(
    tle_file: Option<String>,
    tle: Option<String>,
    tle_url: Option<String>,
    tle_name: Option<String>,
    mode: TrackMode,
    format: TrackFormat,
    latitude: Option<f64>,
    longitude: Option<f64>,
    predict_passes_hours: u32,
    pass_min_elevation_deg: f64,
    ground_track_hours: u32,
    ground_track_step_sec: u64,
    ground_track_json: bool,
    afspc: bool,
) -> Result<()> {
    use chrono::Duration;
    use ephemerust::celestial::ObserverLocation;
    use ephemerust::satellite::{PropagationModel, ground_track_to_csv, ground_track_to_json};

    let model = if afspc {
        PropagationModel::AfspcCompatibility
    } else {
        PropagationModel::Modern
    };

    const DEFAULT_LAT_DEG: f64 = 47.9088;
    const DEFAULT_LON_DEG: f64 = -122.2503;

    if !(-90.0..=90.0).contains(&pass_min_elevation_deg) {
        return Err(ephemerust::AstroError::SatelliteError(
            "--pass-min-elevation-deg must be between -90 and 90".into(),
        ));
    }

    if mode == TrackMode::Passes && predict_passes_hours == 0 {
        return Err(ephemerust::AstroError::SatelliteError(
            "mode `passes` requires --predict-passes-hours > 0".into(),
        ));
    }
    if mode == TrackMode::Ground && ground_track_hours == 0 {
        return Err(ephemerust::AstroError::SatelliteError(
            "mode `ground` requires --ground-track-hours > 0".into(),
        ));
    }

    let parsed = resolve_tle_input(tle_file, tle, tle_url, tle_name)?;
    let epoch = parsed.epoch();
    // SGP4 is initialized where a result needs it, so a summary-only request still works for
    // an element set that parses but cannot be propagated.
    let prop = || parsed.propagator(model);
    if afspc {
        log::info!("track: using AFSPC compatibility propagation (WGS72 + legacy sidereal/epoch)");
    }
    let obs = ObserverLocation {
        latitude: latitude.unwrap_or(DEFAULT_LAT_DEG),
        longitude: longitude.unwrap_or(DEFAULT_LON_DEG),
        elevation: 0.0,
    };

    match format {
        TrackFormat::Json => {
            let mut out = TrackJsonOutput {
                element_set_format: parsed.format_label(),
                tle: None,
                observer: None,
                state: None,
                subpoint: None,
                look_angles: None,
                passes: None,
                ground_track: None,
                predict_passes_hours: None,
                pass_min_elevation_deg: None,
                ground_track_hours: None,
                ground_track_step_sec: None,
                propagation_model: None,
            };

            let set_propagation_meta = |out: &mut TrackJsonOutput| {
                out.propagation_model = Some(model);
            };

            let include_tle = matches!(
                mode,
                TrackMode::All
                    | TrackMode::Tle
                    | TrackMode::State
                    | TrackMode::Subpoint
                    | TrackMode::Look
                    | TrackMode::Passes
                    | TrackMode::Ground
            );
            if include_tle {
                out.tle = Some(parsed.summary_json());
            }

            match mode {
                TrackMode::Tle => {
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
                TrackMode::State => {
                    set_propagation_meta(&mut out);
                    out.state = Some(prop()?.propagate(epoch)?);
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
                TrackMode::Subpoint => {
                    set_propagation_meta(&mut out);
                    out.subpoint = Some(prop()?.subpoint(epoch)?);
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
                TrackMode::Look => {
                    set_propagation_meta(&mut out);
                    out.observer = Some(ObserverJson {
                        latitude_deg: obs.latitude,
                        longitude_deg: obs.longitude,
                        elevation_m: obs.elevation,
                    });
                    out.look_angles = Some(prop()?.look_angles(epoch, obs)?);
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
                TrackMode::Passes => {
                    set_propagation_meta(&mut out);
                    let win_end = epoch + Duration::hours(i64::from(predict_passes_hours));
                    out.observer = Some(ObserverJson {
                        latitude_deg: obs.latitude,
                        longitude_deg: obs.longitude,
                        elevation_m: obs.elevation,
                    });
                    out.predict_passes_hours = Some(predict_passes_hours);
                    out.pass_min_elevation_deg = Some(pass_min_elevation_deg);
                    out.passes = Some(prop()?.predict_passes(
                        obs,
                        epoch,
                        win_end,
                        pass_min_elevation_deg,
                    )?);
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
                TrackMode::Ground => {
                    set_propagation_meta(&mut out);
                    if ground_track_step_sec == 0 {
                        return Err(ephemerust::AstroError::SatelliteError(
                            "--ground-track-step-sec must be at least 1".into(),
                        ));
                    }
                    let win_end = epoch + Duration::hours(i64::from(ground_track_hours));
                    let step =
                        Duration::seconds(i64::try_from(ground_track_step_sec).map_err(|_| {
                            ephemerust::AstroError::SatelliteError(
                                "--ground-track-step-sec is too large for chrono::Duration".into(),
                            )
                        })?);
                    out.ground_track_hours = Some(ground_track_hours);
                    out.ground_track_step_sec = Some(ground_track_step_sec);
                    out.ground_track = Some(prop()?.ground_track(epoch, win_end, step)?);
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
                TrackMode::All => {
                    set_propagation_meta(&mut out);
                    out.observer = Some(ObserverJson {
                        latitude_deg: obs.latitude,
                        longitude_deg: obs.longitude,
                        elevation_m: obs.elevation,
                    });
                    out.state = Some(prop()?.propagate(epoch)?);
                    out.subpoint = Some(prop()?.subpoint(epoch)?);
                    out.look_angles = Some(prop()?.look_angles(epoch, obs)?);
                    if predict_passes_hours > 0 {
                        let win_end = epoch + Duration::hours(i64::from(predict_passes_hours));
                        out.predict_passes_hours = Some(predict_passes_hours);
                        out.pass_min_elevation_deg = Some(pass_min_elevation_deg);
                        out.passes = Some(prop()?.predict_passes(
                            obs,
                            epoch,
                            win_end,
                            pass_min_elevation_deg,
                        )?);
                    }
                    if ground_track_hours > 0 {
                        if ground_track_step_sec == 0 {
                            return Err(ephemerust::AstroError::SatelliteError(
                                "--ground-track-step-sec must be at least 1".into(),
                            ));
                        }
                        let win_end = epoch + Duration::hours(i64::from(ground_track_hours));
                        let step = Duration::seconds(
                            i64::try_from(ground_track_step_sec).map_err(|_| {
                                ephemerust::AstroError::SatelliteError(
                                    "--ground-track-step-sec is too large for chrono::Duration"
                                        .into(),
                                )
                            })?,
                        );
                        out.ground_track_hours = Some(ground_track_hours);
                        out.ground_track_step_sec = Some(ground_track_step_sec);
                        out.ground_track = Some(prop()?.ground_track(epoch, win_end, step)?);
                    }
                    let s = serde_json::to_string_pretty(&out).map_err(|e| {
                        ephemerust::AstroError::SatelliteError(format!("JSON: {e}"))
                    })?;
                    println!("{s}");
                    return Ok(());
                }
            }
        }
        TrackFormat::Human => match mode {
            TrackMode::Tle => parsed.print_summary(),
            TrackMode::State => {
                let state = prop()?.propagate(epoch)?;
                println!("State at epoch (TEME frame):");
                println!(
                    "  Position [km]:   x={:.3} y={:.3} z={:.3}",
                    state.position_km[0], state.position_km[1], state.position_km[2]
                );
                println!(
                    "  Velocity [km/s]: vx={:.6} vy={:.6} vz={:.6}",
                    state.velocity_km_s[0], state.velocity_km_s[1], state.velocity_km_s[2]
                );
            }
            TrackMode::Subpoint => {
                let sub = prop()?.subpoint(epoch)?;
                println!("Sub-satellite point at epoch (WGS84 geodetic):");
                println!("  Latitude:  {:+.6}°", sub.latitude_deg);
                println!("  Longitude: {:+.6}°", sub.longitude_deg);
                println!("  Altitude:  {:.3} km (ellipsoidal)", sub.altitude_km);
            }
            TrackMode::Look => {
                let look = prop()?.look_angles(epoch, obs)?;
                println!(
                    "Look angles at epoch (observer {:.4}° N, {:.4}° lon, WGS84 h = {:.0} m):",
                    obs.latitude, obs.longitude, obs.elevation
                );
                println!(
                    "  Azimuth:    {:7.3}° (clockwise from true north)",
                    look.azimuth_deg
                );
                println!("  Elevation:  {:7.3}°", look.elevation_deg);
                println!("  Range:      {:10.3} km", look.range_km);
                println!(
                    "  Range rate: {:+.6} km/s (negative → approaching)",
                    look.range_rate_km_s
                );
            }
            TrackMode::Passes => {
                let win_end = epoch + Duration::hours(i64::from(predict_passes_hours));
                let passes = prop()?.predict_passes(obs, epoch, win_end, pass_min_elevation_deg)?;
                println!(
                    "Predicted passes ({} h from epoch, min elevation {:.1}°):",
                    predict_passes_hours, pass_min_elevation_deg
                );
                if passes.is_empty() {
                    println!("  (none)");
                } else {
                    for (i, p) in passes.iter().enumerate() {
                        println!(
                            "  Pass {}: AOS {}  max el {:5.1}° @ {}  LOS {}",
                            i + 1,
                            p.aos.format("%Y-%m-%d %H:%M:%S"),
                            p.max_elevation_deg,
                            p.culmination.format("%H:%M:%S"),
                            p.los.format("%Y-%m-%d %H:%M:%S"),
                        );
                        println!(
                            "           az @ AOS {:6.1}°   az @ LOS {:6.1}°",
                            p.aos_azimuth_deg, p.los_azimuth_deg
                        );
                    }
                }
            }
            TrackMode::Ground => {
                if ground_track_step_sec == 0 {
                    return Err(ephemerust::AstroError::SatelliteError(
                        "--ground-track-step-sec must be at least 1".into(),
                    ));
                }
                let win_end = epoch + Duration::hours(i64::from(ground_track_hours));
                let step =
                    Duration::seconds(i64::try_from(ground_track_step_sec).map_err(|_| {
                        ephemerust::AstroError::SatelliteError(
                            "--ground-track-step-sec is too large for chrono::Duration".into(),
                        )
                    })?);
                let samples = prop()?.ground_track(epoch, win_end, step)?;
                if ground_track_json {
                    println!("Ground track (JSON, {} samples):", samples.len());
                    println!("{}", ground_track_to_json(&samples)?);
                } else {
                    println!(
                        "Ground track (CSV, {} h from epoch, step {} s, {} samples):",
                        ground_track_hours,
                        ground_track_step_sec,
                        samples.len()
                    );
                    print!("{}", ground_track_to_csv(&samples));
                }
            }
            TrackMode::All => {
                parsed.print_summary();
                let state = prop()?.propagate(epoch)?;
                let sub = prop()?.subpoint(epoch)?;
                let look = prop()?.look_angles(epoch, obs)?;
                println!();
                println!("State at epoch (TEME frame):");
                println!(
                    "  Position [km]:   x={:.3} y={:.3} z={:.3}",
                    state.position_km[0], state.position_km[1], state.position_km[2]
                );
                println!(
                    "  Velocity [km/s]: vx={:.6} vy={:.6} vz={:.6}",
                    state.velocity_km_s[0], state.velocity_km_s[1], state.velocity_km_s[2]
                );
                println!();
                println!("Sub-satellite point at epoch (WGS84 geodetic):");
                println!("  Latitude:  {:+.6}°", sub.latitude_deg);
                println!("  Longitude: {:+.6}°", sub.longitude_deg);
                println!("  Altitude:  {:.3} km (ellipsoidal)", sub.altitude_km);
                println!();
                println!(
                    "Look angles at epoch (observer {:.4}° N, {:.4}° lon, WGS84 h = {:.0} m):",
                    obs.latitude, obs.longitude, obs.elevation
                );
                println!(
                    "  Azimuth:    {:7.3}° (clockwise from true north)",
                    look.azimuth_deg
                );
                println!("  Elevation:  {:7.3}°", look.elevation_deg);
                println!("  Range:      {:10.3} km", look.range_km);
                println!(
                    "  Range rate: {:+.6} km/s (negative → approaching)",
                    look.range_rate_km_s
                );
                if predict_passes_hours > 0 {
                    let win_end = epoch + Duration::hours(i64::from(predict_passes_hours));
                    let passes =
                        prop()?.predict_passes(obs, epoch, win_end, pass_min_elevation_deg)?;
                    println!();
                    println!(
                        "Predicted passes ({} h from epoch, min elevation {:.1}°):",
                        predict_passes_hours, pass_min_elevation_deg
                    );
                    if passes.is_empty() {
                        println!("  (none)");
                    } else {
                        for (i, p) in passes.iter().enumerate() {
                            println!(
                                "  Pass {}: AOS {}  max el {:5.1}° @ {}  LOS {}",
                                i + 1,
                                p.aos.format("%Y-%m-%d %H:%M:%S"),
                                p.max_elevation_deg,
                                p.culmination.format("%H:%M:%S"),
                                p.los.format("%Y-%m-%d %H:%M:%S"),
                            );
                            println!(
                                "           az @ AOS {:6.1}°   az @ LOS {:6.1}°",
                                p.aos_azimuth_deg, p.los_azimuth_deg
                            );
                        }
                    }
                }
                if ground_track_hours > 0 {
                    if ground_track_step_sec == 0 {
                        return Err(ephemerust::AstroError::SatelliteError(
                            "--ground-track-step-sec must be at least 1".into(),
                        ));
                    }
                    let win_end = epoch + Duration::hours(i64::from(ground_track_hours));
                    let step =
                        Duration::seconds(i64::try_from(ground_track_step_sec).map_err(|_| {
                            ephemerust::AstroError::SatelliteError(
                                "--ground-track-step-sec is too large for chrono::Duration".into(),
                            )
                        })?);
                    let samples = prop()?.ground_track(epoch, win_end, step)?;
                    println!();
                    if ground_track_json {
                        println!("Ground track (JSON, {} samples):", samples.len());
                        println!("{}", ground_track_to_json(&samples)?);
                    } else {
                        println!(
                            "Ground track (CSV, {} h from epoch, step {} s, {} samples):",
                            ground_track_hours,
                            ground_track_step_sec,
                            samples.len()
                        );
                        print!("{}", ground_track_to_csv(&samples));
                    }
                }
            }
        },
    }

    Ok(())
}

/// Prints a human-readable summary of a parsed TLE (used by `track` before state, subpoint,
/// and look-angle output).
fn print_tle_summary(tle: &ephemerust::satellite::Tle) {
    if let Some(name) = &tle.name {
        println!("Object:        {}", name);
    }
    println!(
        "Catalog #:     {} ({})",
        tle.catalog_number, tle.classification
    );
    println!("Intl. desig.:  {}", tle.international_designator);
    println!(
        "Epoch (UTC):   {}",
        tle.epoch.format("%Y-%m-%d %H:%M:%S%.3f")
    );
    println!("Inclination:   {:.4}°", tle.inclination_deg);
    println!("RAAN:          {:.4}°", tle.raan_deg);
    println!("Eccentricity:  {:.7}", tle.eccentricity);
    println!("Arg perigee:   {:.4}°", tle.arg_perigee_deg);
    println!("Mean anomaly:  {:.4}°", tle.mean_anomaly_deg);
    println!("Mean motion:   {:.8} rev/day", tle.mean_motion);
    println!("B* drag:       {:.6e} 1/earth-radii", tle.bstar);
    println!("Rev # @ epoch: {}", tle.revolution_number);
}

/// Prints the same summary for an OMM, headed by the serialization it arrived in.
fn print_omm_summary(omm: &ephemerust::omm::Omm, format: ephemerust::omm::OmmFormat) {
    println!("Element set:   OMM ({format})");
    if let Some(name) = &omm.name {
        println!("Object:        {}", name);
    }
    match omm.catalog_number {
        Some(number) => println!("Catalog #:     {} ({})", number, omm.classification),
        None => println!("Catalog #:     not cataloged ({})", omm.classification),
    }
    if let Some(designator) = &omm.international_designator {
        println!("Intl. desig.:  {designator}");
    }
    println!(
        "Epoch (UTC):   {}",
        omm.epoch.format("%Y-%m-%d %H:%M:%S%.3f")
    );
    println!("Inclination:   {:.4}°", omm.inclination_deg);
    println!("RAAN:          {:.4}°", omm.raan_deg);
    println!("Eccentricity:  {:.7}", omm.eccentricity);
    println!("Arg perigee:   {:.4}°", omm.arg_perigee_deg);
    println!("Mean anomaly:  {:.4}°", omm.mean_anomaly_deg);
    println!("Mean motion:   {:.8} rev/day", omm.mean_motion);
    println!("B* drag:       {:.6e} 1/earth-radii", omm.bstar);
    println!("Rev # @ epoch: {}", omm.revolution_number);
}

fn init_logging(verbose: bool) {
    // Configure the default level directly rather than via `std::env::set_var`,
    // which became `unsafe` in edition 2024 (mutating the environment is not
    // thread-safe). A user-supplied RUST_LOG still takes precedence.
    let default_level = if verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .init();
}

fn parse_date_time(
    date_str: &str,
    time_str: Option<&str>,
) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, NaiveDate, NaiveTime, Utc};

    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| ephemerust::AstroError::InvalidTime(format!("Invalid date: {}", e)))?;

    let time = if let Some(ts) = time_str {
        NaiveTime::parse_from_str(ts, "%H:%M:%S")
            .map_err(|e| ephemerust::AstroError::InvalidTime(format!("Invalid time: {}", e)))?
    } else {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    };

    Ok(DateTime::from_naive_utc_and_offset(
        date.and_time(time),
        Utc,
    ))
}

fn format_time(hours: f64) -> (i32, i32, i32) {
    let h = hours as i32;
    let m = ((hours - h as f64) * 60.0) as i32;
    let s = ((hours - h as f64 - m as f64 / 60.0) * 3600.0) as i32;
    (h, m, s)
}

fn format_angle(degrees: f64) -> (i32, i32, i32, &'static str) {
    let deg = degrees.abs() as i32;
    let min = ((degrees.abs() - deg as f64) * 60.0) as i32;
    let sec = ((degrees.abs() - deg as f64 - min as f64 / 60.0) * 3600.0) as i32;
    let sign = if degrees >= 0.0 { "+" } else { "-" };
    (deg, min, sec, sign)
}

fn parse_and_convert_radec_to_altaz(coords: &str) -> Result<ephemerust::coordinates::AltAz> {
    use ephemerust::coordinates::{RaDec, ra_dec_to_alt_az};
    use ephemerust::time::{greenwich_mean_sidereal_time, julian_date, local_sidereal_time};

    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 2 {
        return Err(ephemerust::AstroError::InvalidCoordinate(
            "Expected: hours,degrees".to_string(),
        ));
    }

    let ra: f64 = parts[0]
        .trim()
        .parse()
        .map_err(|_| ephemerust::AstroError::InvalidCoordinate("Invalid RA".to_string()))?;
    let dec: f64 = parts[1]
        .trim()
        .parse()
        .map_err(|_| ephemerust::AstroError::InvalidCoordinate("Invalid Dec".to_string()))?;

    let jd = julian_date(chrono::Utc::now());
    let gmst = greenwich_mean_sidereal_time(jd);
    let (lat, lon) = (47.9088, -122.2503);
    let lst = local_sidereal_time(gmst, lon);

    ra_dec_to_alt_az(RaDec { ra, dec }, lat, lon, lst)
}

fn parse_and_convert_altaz_to_radec(coords: &str) -> Result<ephemerust::coordinates::RaDec> {
    use ephemerust::coordinates::{AltAz, alt_az_to_ra_dec};
    use ephemerust::time::{greenwich_mean_sidereal_time, julian_date, local_sidereal_time};

    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 2 {
        return Err(ephemerust::AstroError::InvalidCoordinate(
            "Expected: altitude,azimuth".to_string(),
        ));
    }

    let alt: f64 = parts[0]
        .trim()
        .parse()
        .map_err(|_| ephemerust::AstroError::InvalidCoordinate("Invalid altitude".to_string()))?;
    let az: f64 = parts[1]
        .trim()
        .parse()
        .map_err(|_| ephemerust::AstroError::InvalidCoordinate("Invalid azimuth".to_string()))?;

    let jd = julian_date(chrono::Utc::now());
    let gmst = greenwich_mean_sidereal_time(jd);
    let (lat, lon) = (47.9088, -122.2503);
    let lst = local_sidereal_time(gmst, lon);

    alt_az_to_ra_dec(AltAz { alt, az }, lat, lon, lst)
}

fn parse_and_convert_ecef_to_eci(
    coords: &str,
    gmst_opt: Option<f64>,
) -> Result<ephemerust::coordinates::Eci> {
    use ephemerust::coordinates::{Ecef, ecef_to_eci};
    use ephemerust::time::{greenwich_mean_sidereal_time, julian_date};

    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 3 {
        return Err(ephemerust::AstroError::InvalidCoordinate(
            "Expected: x,y,z (in meters)".to_string(),
        ));
    }

    let x: f64 = parts[0].trim().parse().map_err(|_| {
        ephemerust::AstroError::InvalidCoordinate("Invalid x coordinate".to_string())
    })?;
    let y: f64 = parts[1].trim().parse().map_err(|_| {
        ephemerust::AstroError::InvalidCoordinate("Invalid y coordinate".to_string())
    })?;
    let z: f64 = parts[2].trim().parse().map_err(|_| {
        ephemerust::AstroError::InvalidCoordinate("Invalid z coordinate".to_string())
    })?;

    let gmst = if let Some(gmst_val) = gmst_opt {
        gmst_val
    } else {
        // Auto-calculate from current time
        let jd = julian_date(chrono::Utc::now());
        greenwich_mean_sidereal_time(jd)
    };

    ecef_to_eci(Ecef { x, y, z }, gmst)
}

fn parse_and_convert_eci_to_ecef(
    coords: &str,
    gmst_opt: Option<f64>,
) -> Result<ephemerust::coordinates::Ecef> {
    use ephemerust::coordinates::{Eci, eci_to_ecef};
    use ephemerust::time::{greenwich_mean_sidereal_time, julian_date};

    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 3 {
        return Err(ephemerust::AstroError::InvalidCoordinate(
            "Expected: x,y,z (in meters)".to_string(),
        ));
    }

    let x: f64 = parts[0].trim().parse().map_err(|_| {
        ephemerust::AstroError::InvalidCoordinate("Invalid x coordinate".to_string())
    })?;
    let y: f64 = parts[1].trim().parse().map_err(|_| {
        ephemerust::AstroError::InvalidCoordinate("Invalid y coordinate".to_string())
    })?;
    let z: f64 = parts[2].trim().parse().map_err(|_| {
        ephemerust::AstroError::InvalidCoordinate("Invalid z coordinate".to_string())
    })?;

    let gmst = if let Some(gmst_val) = gmst_opt {
        gmst_val
    } else {
        // Auto-calculate from current time
        let jd = julian_date(chrono::Utc::now());
        greenwich_mean_sidereal_time(jd)
    };

    eci_to_ecef(Eci { x, y, z }, gmst)
}

/// Parses a celestial object name into a CelestialObject enum.
///
/// Supports:
/// - "sun" or "Sun" → Sun
/// - "moon" or "Moon" → Moon
/// - Planet names (case-insensitive): mercury, venus, mars, jupiter, saturn, uranus, neptune
///
/// # Arguments
/// * `object_name` - Name of the celestial object
///
/// # Returns
/// CelestialObject enum variant
///
/// # Errors
/// Returns an error if the object name is not recognized
fn parse_celestial_object(object_name: &str) -> Result<ephemerust::celestial::CelestialObject> {
    let obj_lower = object_name.to_lowercase();

    match obj_lower.as_str() {
        "sun" => Ok(ephemerust::celestial::CelestialObject::Sun),
        "moon" => Ok(ephemerust::celestial::CelestialObject::Moon),
        // Not the Sun or Moon: try the `FromStr` impl on `Planet` (str::parse), replacing
        // its planet-only error with one that also mentions sun/moon.
        planet_name => planet_name
            .parse::<ephemerust::planets::Planet>()
            .map(ephemerust::celestial::CelestialObject::Planet)
            .map_err(|_| {
                ephemerust::AstroError::InvalidCoordinate(format!(
                    "Unknown object: {}. Supported: sun, moon, mercury, venus, mars, jupiter, saturn, uranus, neptune",
                    object_name
                ))
            }),
    }
}
