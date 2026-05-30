use clap::{Parser, Subcommand};
use cli_astro_calc::Result;

/// A command-line astronomy and orbital mechanics calculator
#[derive(Parser)]
#[command(name = "cli-astro-calc")]
#[command(about = "An Astrological Calculator and exploration into RUST")]
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
    /// Satellite tracking from a TLE (under construction; see docs/satellite-tracking-plan.md)
    Track {
        /// Path to a file containing a TLE (2- or 3-line element set)
        #[arg(short = 'f', long)]
        tle_file: Option<String>,
        /// Inline TLE text (lines separated by literal "\n" or by spaces is not supported; quote the two/three lines)
        #[arg(short = 't', long)]
        tle: Option<String>,
        /// Observer latitude in degrees (positive north)
        #[arg(short = 'a', long)]
        latitude: Option<f64>,
        /// Observer longitude in degrees (positive east)
        #[arg(short = 'o', long)]
        longitude: Option<f64>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    
    match cli.command {
        Commands::Convert { from, to, coords, gmst } => {
            match (from.to_lowercase().as_str(), to.to_lowercase().as_str()) {
                ("ra-dec" | "radec", "alt-az" | "altaz") => {
                    let result = parse_and_convert_radec_to_altaz(&coords)?;
                    let (alt_deg, alt_min, alt_sec, alt_sign) = format_angle(result.alt);
                    let (az_deg, az_min, az_sec, _) = format_angle(result.az);
                    println!("Alt: {}{:02}°{:02}'{:02}\"", alt_sign, alt_deg, alt_min, alt_sec);
                    println!("Az:  {:03}°{:02}'{:02}\"", az_deg, az_min, az_sec);
                },
                ("alt-az" | "altaz", "ra-dec" | "radec") => {
                    let result = parse_and_convert_altaz_to_radec(&coords)?;
                    let (ra_h, ra_m, ra_s) = format_time(result.ra);
                    let (dec_deg, dec_min, dec_sec, dec_sign) = format_angle(result.dec);
                    println!("RA:  {:02}:{:02}:{:02}", ra_h, ra_m, ra_s);
                    println!("Dec: {}{:02}°{:02}'{:02}\"", dec_sign, dec_deg, dec_min, dec_sec);
                },
                ("ecef", "eci") => {
                    let result = parse_and_convert_ecef_to_eci(&coords, gmst)?;
                    println!("X: {:.3} m", result.x);
                    println!("Y: {:.3} m", result.y);
                    println!("Z: {:.3} m", result.z);
                },
                ("eci", "ecef") => {
                    let result = parse_and_convert_eci_to_ecef(&coords, gmst)?;
                    println!("X: {:.3} m", result.x);
                    println!("Y: {:.3} m", result.y);
                    println!("Z: {:.3} m", result.z);
                },
                _ => {
                    return Err(cli_astro_calc::AstroError::InvalidCoordinate(
                        format!("Unsupported conversion: {} to {}", from, to)
                    ));
                }
            }
        },
        Commands::RiseSet { object, latitude, longitude, date } => {
            let date_time = if let Some(date_str) = date {
                parse_date_time(&date_str, None)?
            } else {
                chrono::Utc::now()
            };
            
            let location = cli_astro_calc::celestial::ObserverLocation {
                latitude,
                longitude,
                elevation: 0.0,
            };
            
            let obj = parse_celestial_object(&object)?;
            
            let rise_set = cli_astro_calc::celestial::calculate_rise_set_times(obj, location, date_time)?;
            
            match rise_set.rise {
                Some(t) => println!("Rise: {}", t.format("%H:%M:%S UTC")),
                None => println!("Rise: Does not rise"),
            }
            match rise_set.set {
                Some(t) => println!("Set:  {}", t.format("%H:%M:%S UTC")),
                None => println!("Set:  Does not set"),
            }
        },
        Commands::Position { object, date } => {
            let date_time = parse_date_time(&date, None)?;
            let obj = parse_celestial_object(&object)?;
            
            let pos = cli_astro_calc::celestial::calculate_position(obj, date_time)?;
            let (ra_h, ra_m, ra_s) = format_time(pos.ra);
            let (dec_deg, dec_min, dec_sec, dec_sign) = format_angle(pos.dec);
            
            println!("RA:  {:02}:{:02}:{:02}", ra_h, ra_m, ra_s);
            println!("Dec: {}{:02}°{:02}'{:02}\"", dec_sign, dec_deg, dec_min, dec_sec);
        },
        Commands::Time { date, time } => {
            let date_time = parse_date_time(&date, time.as_deref())?;
            let jd = cli_astro_calc::time::julian_date(date_time);
            let gmst = cli_astro_calc::time::greenwich_mean_sidereal_time(jd);
            let (h, m, s) = format_time(gmst);
            
            println!("JD:   {:.6}", jd);
            println!("GMST: {:02}:{:02}:{:02}", h, m, s);
        },
        Commands::Orbital { semi_major, eccentricity, inclination, raan, arg_periapsis, mean_anomaly, mu } => {
            use cli_astro_calc::orbital::{OrbitalElements, orbital_period, mean_to_true_anomaly, elements_to_state_vector};

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

            println!("Elements: a={} km, e={}, i={}°, Ω={}°, ω={}°, M={}°",
                semi_major, eccentricity, inclination, raan, arg_periapsis, mean_anomaly);
            println!("Period:   {:.1} s ({:.2} min)", period_s, period_s / 60.0);
            println!("True anomaly: {:.4}°", true_anomaly);
            println!("State vector (inertial frame):");
            println!("  Position [km]:   x={:.3} y={:.3} z={:.3}",
                state.position[0], state.position[1], state.position[2]);
            println!("  Velocity [km/s]: vx={:.6} vy={:.6} vz={:.6}",
                state.velocity[0], state.velocity[1], state.velocity[2]);
        },
        Commands::Track { tle_file, tle, latitude: _, longitude: _ } => {
            use cli_astro_calc::satellite::Tle;

            let parsed = match (tle_file, tle) {
                (Some(path), _) => Tle::from_file(&path)?,
                (None, Some(text)) => Tle::parse(&text)?,
                (None, None) => {
                    return Err(cli_astro_calc::AstroError::SatelliteError(
                        "provide a TLE via --tle-file <path> or --tle \"<line1>\\n<line2>\"".to_string()
                    ));
                }
            };

            print_tle_summary(&parsed);
        },
    }
    
    Ok(())
}

/// Prints a human-readable summary of a parsed TLE.
///
/// Interim Milestone 1 output: propagation, look angles, passes, and ground tracks are
/// added in later milestones (see docs/satellite-tracking-plan.md).
fn print_tle_summary(tle: &cli_astro_calc::satellite::Tle) {
    if let Some(name) = &tle.name {
        println!("Object:        {}", name);
    }
    println!("Catalog #:     {} ({})", tle.catalog_number, tle.classification);
    println!("Intl. desig.:  {}", tle.international_designator);
    println!("Epoch (UTC):   {}", tle.epoch.format("%Y-%m-%d %H:%M:%S%.3f"));
    println!("Inclination:   {:.4}°", tle.inclination_deg);
    println!("RAAN:          {:.4}°", tle.raan_deg);
    println!("Eccentricity:  {:.7}", tle.eccentricity);
    println!("Arg perigee:   {:.4}°", tle.arg_perigee_deg);
    println!("Mean anomaly:  {:.4}°", tle.mean_anomaly_deg);
    println!("Mean motion:   {:.8} rev/day", tle.mean_motion);
    println!("B* drag:       {:.6e} 1/earth-radii", tle.bstar);
    println!("Rev # @ epoch: {}", tle.revolution_number);
    println!();
    println!("(propagation and pass prediction are forthcoming — see the satellite-tracking plan)");
}

fn init_logging(verbose: bool) {
    let log_level = if verbose { "debug" } else { "info" };
    std::env::set_var("RUST_LOG", log_level);
    env_logger::init();
}

fn parse_date_time(date_str: &str, time_str: Option<&str>) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, Utc, NaiveDate, NaiveTime};
    
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| cli_astro_calc::AstroError::InvalidTime(format!("Invalid date: {}", e)))?;
    
    let time = if let Some(ts) = time_str {
        NaiveTime::parse_from_str(ts, "%H:%M:%S")
            .map_err(|e| cli_astro_calc::AstroError::InvalidTime(format!("Invalid time: {}", e)))?
    } else {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    };
    
    Ok(DateTime::from_naive_utc_and_offset(date.and_time(time), Utc))
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

fn parse_and_convert_radec_to_altaz(coords: &str) -> Result<cli_astro_calc::coordinates::AltAz> {
    use cli_astro_calc::coordinates::{RaDec, ra_dec_to_alt_az};
    use cli_astro_calc::time::{julian_date, greenwich_mean_sidereal_time, local_sidereal_time};
    
    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 2 {
        return Err(cli_astro_calc::AstroError::InvalidCoordinate("Expected: hours,degrees".to_string()));
    }
    
    let ra: f64 = parts[0].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid RA".to_string()))?;
    let dec: f64 = parts[1].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid Dec".to_string()))?;
    
    let jd = julian_date(chrono::Utc::now());
    let gmst = greenwich_mean_sidereal_time(jd);
    let (lat, lon) = (47.9088, -122.2503);
    let lst = local_sidereal_time(gmst, lon);
    
    ra_dec_to_alt_az(RaDec { ra, dec }, lat, lon, lst)
}

fn parse_and_convert_altaz_to_radec(coords: &str) -> Result<cli_astro_calc::coordinates::RaDec> {
    use cli_astro_calc::coordinates::{AltAz, alt_az_to_ra_dec};
    use cli_astro_calc::time::{julian_date, greenwich_mean_sidereal_time, local_sidereal_time};
    
    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 2 {
        return Err(cli_astro_calc::AstroError::InvalidCoordinate("Expected: altitude,azimuth".to_string()));
    }
    
    let alt: f64 = parts[0].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid altitude".to_string()))?;
    let az: f64 = parts[1].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid azimuth".to_string()))?;
    
    let jd = julian_date(chrono::Utc::now());
    let gmst = greenwich_mean_sidereal_time(jd);
    let (lat, lon) = (47.9088, -122.2503);
    let lst = local_sidereal_time(gmst, lon);
    
    alt_az_to_ra_dec(AltAz { alt, az }, lat, lon, lst)
}

fn parse_and_convert_ecef_to_eci(coords: &str, gmst_opt: Option<f64>) -> Result<cli_astro_calc::coordinates::Eci> {
    use cli_astro_calc::coordinates::{Ecef, ecef_to_eci};
    use cli_astro_calc::time::{julian_date, greenwich_mean_sidereal_time};
    
    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 3 {
        return Err(cli_astro_calc::AstroError::InvalidCoordinate("Expected: x,y,z (in meters)".to_string()));
    }
    
    let x: f64 = parts[0].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid x coordinate".to_string()))?;
    let y: f64 = parts[1].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid y coordinate".to_string()))?;
    let z: f64 = parts[2].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid z coordinate".to_string()))?;
    
    let gmst = if let Some(gmst_val) = gmst_opt {
        gmst_val
    } else {
        // Auto-calculate from current time
        let jd = julian_date(chrono::Utc::now());
        greenwich_mean_sidereal_time(jd)
    };
    
    ecef_to_eci(Ecef { x, y, z }, gmst)
}

fn parse_and_convert_eci_to_ecef(coords: &str, gmst_opt: Option<f64>) -> Result<cli_astro_calc::coordinates::Ecef> {
    use cli_astro_calc::coordinates::{Eci, eci_to_ecef};
    use cli_astro_calc::time::{julian_date, greenwich_mean_sidereal_time};
    
    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 3 {
        return Err(cli_astro_calc::AstroError::InvalidCoordinate("Expected: x,y,z (in meters)".to_string()));
    }
    
    let x: f64 = parts[0].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid x coordinate".to_string()))?;
    let y: f64 = parts[1].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid y coordinate".to_string()))?;
    let z: f64 = parts[2].trim().parse()
        .map_err(|_| cli_astro_calc::AstroError::InvalidCoordinate("Invalid z coordinate".to_string()))?;
    
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
fn parse_celestial_object(object_name: &str) -> Result<cli_astro_calc::celestial::CelestialObject> {
    let obj_lower = object_name.to_lowercase();
    
    match obj_lower.as_str() {
        "sun" => Ok(cli_astro_calc::celestial::CelestialObject::Sun),
        "moon" => Ok(cli_astro_calc::celestial::CelestialObject::Moon),
        planet_name => {
            // Try to parse as a planet
            if let Some(planet) = cli_astro_calc::planets::Planet::from_str(planet_name) {
                Ok(cli_astro_calc::celestial::CelestialObject::Planet(planet))
            } else {
                Err(cli_astro_calc::AstroError::InvalidCoordinate(
                    format!("Unknown object: {}. Supported: sun, moon, mercury, venus, mars, jupiter, saturn, uranus, neptune", object_name)
                ))
            }
        }
    }
}
