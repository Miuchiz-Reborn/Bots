use aw_sdk::{AwInstance, MessageInfo, SdkError};
use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Tz;
use clap::Parser;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod config;
use config::TimeBotConfig;

// =================================================================================================
//                                     COMMAND LINE ARGUMENTS
// =================================================================================================
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the TOML configuration file
    #[arg(short, long)]
    config: PathBuf,
}

// =================================================================================================
//                                     CONFIGURATION
// =================================================================================================

// LATITUDE_DEGREES: The latitude of the observer, affecting sun/moon altitude.
//   - 0.0   = Equator
//   - 45.0  = Mid-latitude (e.g., North America, Europe)
//   - 60.0  = Northern latitude
const LATITUDE_DEGREES: f32 = 45.0;

// =================================================================================================
//                                       CORE STRUCTS
// =================================================================================================

struct TimeBot {
    pub instance: AwInstance,
    pub time_zone: Tz,
    // If true, the bot's time will track the real-world time in the specified timezone.
    // If false, the time is frozen at the value of `current_hour`.
    pub auto_advance_enabled: bool,
    pub current_hour: f32,
    pub last_update_time: Option<Instant>,
    pub update_interval_ms: u64,
}

impl TimeBot {
    fn new(instance: AwInstance, time_zone: Tz, update_interval_ms: u64) -> Self {
        // Start frozen at noon until the user starts the bot or sets a time.
        Self {
            instance,
            time_zone,
            auto_advance_enabled: true,
            current_hour: 12.0,
            last_update_time: None,
            update_interval_ms,
        }
    }

    // Get update interval as a Duration.
    fn update_interval(&self) -> Duration {
        Duration::from_millis(self.update_interval_ms)
    }

    fn run(&mut self, config: &TimeBotConfig) -> Result<(), SdkError> {
        self.instance.login(aw_sdk::LoginParams::Bot {
            name: "Time Bot".to_string(),
            owner_id: config.bot_config.owner_id,
            privilege_password: config.bot_config.privilege_password.clone(),
            application: "Time Bot".to_string(),
        })?;

        self.instance.enter(&config.time_bot_config.world, true)?;
        self.instance.state_change(aw_sdk::StateChangeParams {
            north: 0,
            height: 0,
            west: 0,
            rotation: 0,
            gesture: 0,
            av_type: 0,
            av_state: 0,
        })?;

        // Set initial time to noon
        update_world_for_time(self, self.current_hour);

        loop {
            let events = self.instance.tick();
            for event in events {
                if let aw_sdk::AwEvent::Message(message_info) = &event {
                    let _ = handle_message(self, &message_info);
                }
                if let aw_sdk::AwEvent::UniverseDisconnected | aw_sdk::AwEvent::WorldDisconnected =
                    event
                {
                    return Err(SdkError::connection_state("Universe or world disconnected"));
                }
            }

            // Handle automatic time advancement if enabled.
            if self.auto_advance_enabled {
                let now = Instant::now();
                let should_update = match self.last_update_time {
                    Some(last_update_time) => {
                        now.duration_since(last_update_time) >= self.update_interval()
                    }
                    None => true,
                };

                if should_update {
                    let real_hour = get_current_hour_in_tz(self.time_zone);
                    // Only send an update if the time has changed meaningfully.
                    if (real_hour - self.current_hour).abs()
                        > (self.update_interval_ms as f32 / 3_600_000.0)
                    {
                        update_world_for_time(self, real_hour);
                    }
                    self.last_update_time = Some(now);
                }
            }

            // Small sleep to prevent high CPU usage.
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

// Basic struct for an RGB color.
#[derive(Debug, Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

// Struct for a 3D position vector.
#[derive(Debug, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

// Holds all calculated sky colors for each of the 6 skybox faces.
#[derive(Debug, Clone)]
struct SkyColors {
    north: Color,
    south: Color,
    east: Color,
    west: Color,
    top: Color,
    bottom: Color,
}

// Represents the complete state of the world's time and lighting for a given moment.
#[derive(Debug, Clone)]
struct WorldTimeState {
    sky_colors: SkyColors,
    light_position: Position,
    light_color: Color,
    light_texture: String,
    light_mask: String,
    light_glow: String, // "Y" for sun glow, "N" for no moon glow.
    star_opacity: u8,
    fog_color: Color,
}

// =================================================================================================
//                                        MAIN LOGIC
// =================================================================================================

fn main() {
    let args = Args::parse();

    let config_string =
        std::fs::read_to_string(args.config).expect("Could not read configuration file.");
    let config: TimeBotConfig =
        toml::from_str(&config_string).expect("Could not parse configuration file.");

    let time_zone: Tz = config
        .time_bot_config
        .time_zone
        .parse()
        .expect("Invalid timezone string in config file.");

    println!("Time bot starting in timezone: {}", time_zone);

    loop {
        let mut time_bot = TimeBot::new(
            AwInstance::new(&config.bot_config.host, config.bot_config.port).unwrap(),
            time_zone,
            config.time_bot_config.update_ms,
        );

        match time_bot.run(&config) {
            Ok(_) => {
                println!("Time bot stopped.");
                break;
            }
            Err(e) => {
                println!("Error: {}", e);
                println!("Reconnecting in 5 seconds...");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

// =================================================================================================
//                                      COMMAND HANDLING
// =================================================================================================

fn handle_message(time_bot: &mut TimeBot, message_info: &MessageInfo) -> Result<(), SdkError> {
    let msg = &message_info.message;

    if msg.starts_with("/time ") {
        if let Some(time_str) = msg.strip_prefix("/time ") {
            let time_str = time_str.trim();
            handle_time_set_command(time_bot, time_str)?;
        }
    } else if msg == "/starttime" {
        handle_start_time_command(time_bot)?;
    } else if msg == "/stoptime" {
        handle_stop_time_command(time_bot)?;
    } else if msg == "/gettime" {
        handle_get_time_command(time_bot)?;
    } else if msg == "/timehelp" {
        handle_help_command(time_bot)?;
    }
    Ok(())
}

fn handle_time_set_command(time_bot: &mut TimeBot, time_str: &str) -> Result<(), SdkError> {
    let hour = match parse_time_string(time_str) {
        Some(h) => h,
        None => {
            time_bot.instance.say(&format!(
                "Invalid time: '{}'. Use HH:MM or a name like 'dawn', 'noon', 'dusk', 'midnight'.",
                time_str
            ))?;
            return Ok(());
        }
    };

    time_bot.auto_advance_enabled = false; // Manually setting time stops auto-advancement.
    update_world_for_time(time_bot, hour);
    let readable_time = format_time(hour);
    time_bot.instance.say(&format!(
        "Time manually set to {}. Real-time tracking is now OFF.",
        readable_time
    ))?;
    Ok(())
}

fn handle_start_time_command(time_bot: &mut TimeBot) -> Result<(), SdkError> {
    if !time_bot.auto_advance_enabled {
        time_bot.auto_advance_enabled = true;
        // Immediately sync to the current real time.
        let real_hour = get_current_hour_in_tz(time_bot.time_zone);
        update_world_for_time(time_bot, real_hour);
        time_bot.instance.say(&format!(
            "Automatic time advancement started. Tracking real time for {}.",
            time_bot.time_zone
        ))?;
    } else {
        time_bot
            .instance
            .say("Time is already advancing automatically.")?;
    }
    Ok(())
}

fn handle_stop_time_command(time_bot: &mut TimeBot) -> Result<(), SdkError> {
    if time_bot.auto_advance_enabled {
        time_bot.auto_advance_enabled = false;
        let readable_time = format_time(time_bot.current_hour);
        time_bot.instance.say(&format!(
            "Automatic time advancement stopped. Time frozen at {}.",
            readable_time
        ))?;
    } else {
        time_bot
            .instance
            .say("Time is not advancing automatically.")?;
    }
    Ok(())
}

fn handle_get_time_command(time_bot: &mut TimeBot) -> Result<(), SdkError> {
    let status = if time_bot.auto_advance_enabled {
        "tracking real time"
    } else {
        "static"
    };
    let readable_time = format_time(time_bot.current_hour);
    time_bot
        .instance
        .say(&format!("Current time: {} ({}).", readable_time, status))?;
    Ok(())
}

fn handle_help_command(time_bot: &mut TimeBot) -> Result<(), SdkError> {
    time_bot.instance.say("Time Bot Commands:")?;
    time_bot
        .instance
        .say("/time <HH:MM|name> - Sets a static time and stops real-time tracking.")?;
    time_bot.instance.say(&format!(
        "/starttime - Starts tracking real-world time for the configured timezone ({}).",
        time_bot.time_zone
    ))?;
    time_bot
        .instance
        .say("/stoptime - Stops auto-advancing time, freezing it at the current moment.")?;
    time_bot
        .instance
        .say("/gettime - Shows the current time and tracking status.")?;
    time_bot
        .instance
        .say("/timehelp - Shows this help message.")?;
    Ok(())
}

// =================================================================================================
//                                      WORLD UPDATE
// =================================================================================================

/// Central function to calculate the world state and send the update to the server.
fn update_world_for_time(time_bot: &mut TimeBot, hour: f32) {
    time_bot.current_hour = hour;
    let state = calculate_world_state(hour);

    if let Ok(mut attributes) = time_bot.instance.world_attributes() {
        attributes.sky_north_red = Some(state.sky_colors.north.r.to_string());
        attributes.sky_north_green = Some(state.sky_colors.north.g.to_string());
        attributes.sky_north_blue = Some(state.sky_colors.north.b.to_string());
        attributes.sky_south_red = Some(state.sky_colors.south.r.to_string());
        attributes.sky_south_green = Some(state.sky_colors.south.g.to_string());
        attributes.sky_south_blue = Some(state.sky_colors.south.b.to_string());
        attributes.sky_east_red = Some(state.sky_colors.east.r.to_string());
        attributes.sky_east_green = Some(state.sky_colors.east.g.to_string());
        attributes.sky_east_blue = Some(state.sky_colors.east.b.to_string());
        attributes.sky_west_red = Some(state.sky_colors.west.r.to_string());
        attributes.sky_west_green = Some(state.sky_colors.west.g.to_string());
        attributes.sky_west_blue = Some(state.sky_colors.west.b.to_string());
        attributes.sky_top_red = Some(state.sky_colors.top.r.to_string());
        attributes.sky_top_green = Some(state.sky_colors.top.g.to_string());
        attributes.sky_top_blue = Some(state.sky_colors.top.b.to_string());
        attributes.sky_bottom_red = Some(state.sky_colors.bottom.r.to_string());
        attributes.sky_bottom_green = Some(state.sky_colors.bottom.g.to_string());
        attributes.sky_bottom_blue = Some(state.sky_colors.bottom.b.to_string());
        attributes.light_x = Some(state.light_position.x.to_string());
        attributes.light_y = Some(state.light_position.y.to_string());
        attributes.light_z = Some(state.light_position.z.to_string());
        attributes.light_red = Some(state.light_color.r.to_string());
        attributes.light_green = Some(state.light_color.g.to_string());
        attributes.light_blue = Some(state.light_color.b.to_string());
        attributes.light_texture = Some(state.light_texture);
        attributes.light_mask = Some(state.light_mask);
        attributes.light_draw_bright = Some(state.light_glow);
        attributes.clouds_layer1_mask = Some("stars1".to_string());
        attributes.clouds_layer1_texture = Some("stars1".to_string());
        attributes.clouds_layer1_opacity = Some(state.star_opacity.to_string());
        attributes.fog_red = Some(state.fog_color.r.to_string());
        attributes.fog_green = Some(state.fog_color.g.to_string());
        attributes.fog_blue = Some(state.fog_color.b.to_string());
        attributes.fog_enable = Some("Y".to_string());
        attributes.fog_maximum = Some("1200".to_string());
        attributes.fog_minimum = Some("100".to_string());
        let _ = time_bot.instance.world_attribute_change(&attributes);
    }
}

// =================================================================================================
//                                  TIME & COLOR CALCULATIONS
// =================================================================================================

/// The main function that orchestrates all time-based calculations.
/// It determines the sun/moon positions and calculates all colors and lighting attributes.
fn calculate_world_state(hour: f32) -> WorldTimeState {
    // 1. Calculate the "true" astronomical position of the sun for atmospheric scattering.
    let sun_elevation = calculate_sun_elevation(hour);
    let sun_azimuth = calculate_sun_azimuth(hour);
    let moon_elevation = calculate_sun_elevation((hour + 12.0) % 24.0); // Also get moon elevation

    // 2. Determine which light source is active and calculate its "visual" position.
    // The sun is the source from 6 AM to 6 PM; the moon is the source from 6 PM to 6 AM.
    let is_daylight = hour >= 6.0 && hour < 18.0;

    let light_position = if is_daylight {
        calculate_compressed_celestial_position(hour, true) // Sun's 12-hour arc
    } else {
        calculate_compressed_celestial_position(hour, false) // Moon's 12-hour arc
    };

    let light_texture = if is_daylight {
        "c_sun".to_string()
    } else {
        "c_moon2".to_string()
    };
    let light_mask = if is_daylight {
        "c_sun".to_string()
    } else {
        "c_moon2".to_string()
    };
    let light_glow = if is_daylight {
        "Y".to_string()
    } else {
        "N".to_string()
    };

    // 3. Calculate all sky colors based on the sun's true position (for atmospheric effects).
    let sky_colors = calculate_sky_colors(sun_elevation, sun_azimuth, moon_elevation);

    // 4. Calculate the color of the light source itself.
    let light_color = calculate_light_color(sun_elevation, !is_daylight);

    // 5. Calculate the opacity of the stars.
    let star_opacity = calculate_star_opacity(sun_elevation);

    // 6. Fog color should be the average of the north, south, east, and west colors.
    let fog_color = Color {
        r: ((sky_colors.north.r as u16
            + sky_colors.south.r as u16
            + sky_colors.east.r as u16
            + sky_colors.west.r as u16)
            / 4) as u8,
        g: ((sky_colors.north.g as u16
            + sky_colors.south.g as u16
            + sky_colors.east.g as u16
            + sky_colors.west.g as u16)
            / 4) as u8,
        b: ((sky_colors.north.b as u16
            + sky_colors.south.b as u16
            + sky_colors.east.b as u16
            + sky_colors.west.b as u16)
            / 4) as u8,
    };

    WorldTimeState {
        sky_colors,
        light_position,
        light_color,
        light_texture,
        light_mask,
        light_glow,
        star_opacity,
        fog_color,
    }
}

// -------------------------------------------------------------------------------------------------
// Positional Calculations
// -------------------------------------------------------------------------------------------------

/// Calculates the sun's elevation angle in degrees (altitude above the horizon).
/// This is the primary driver for most color and light calculations.
fn calculate_sun_elevation(hour: f32) -> f32 {
    let lat_rad = LATITUDE_DEGREES.to_radians();
    // Simplified declination for a basic seasonal model. For now, 0 (equinox).
    let declination_rad = (0.0_f32).to_radians();
    let hour_angle_rad = (hour - 12.0) * 15.0 * std::f32::consts::PI / 180.0;

    let sin_elevation = lat_rad.sin() * declination_rad.sin()
        + lat_rad.cos() * declination_rad.cos() * hour_angle_rad.cos();

    sin_elevation.asin().to_degrees()
}

/// Calculates the sun's azimuth angle in degrees (direction along the horizon).
/// 0° = North, 90° = East, 180° = South, 270° = West.
fn calculate_sun_azimuth(hour: f32) -> f32 {
    let lat_rad = LATITUDE_DEGREES.to_radians();
    // Simplified declination for a basic seasonal model. For now, 0 (equinox).
    let declination_rad = (0.0_f32).to_radians();
    let hour_angle_rad = (hour - 12.0) * 15.0_f32.to_radians();

    // Using atan2 for a more robust azimuth calculation.
    // This formula calculates azimuth from South, positive towards the West.
    let y = hour_angle_rad.sin();
    let x = hour_angle_rad.cos() * lat_rad.sin() - declination_rad.tan() * lat_rad.cos();

    // Convert from South-based to North-based azimuth (0° North, 90° East).
    let azimuth_from_south = y.atan2(x).to_degrees();
    (azimuth_from_south + 180.0) % 360.0
}

/// Calculates the position of the visible light source (sun or moon) on its
/// compressed 12-hour trajectory from -20° East to -20° West.
fn calculate_compressed_celestial_position(hour: f32, is_sun: bool) -> Position {
    // Normalize the hour to a 0.0-1.0 progress value over the 12-hour arc.
    let progress = if is_sun {
        (hour - 6.0) / 12.0 // Sun: 6 AM to 6 PM
    } else {
        // Moon: 6 PM to 6 AM (wraps around midnight)
        if hour >= 18.0 {
            (hour - 18.0) / 12.0
        } else {
            (hour + 6.0) / 12.0
        }
    };

    // Trajectory spans 180 degrees of azimuth, from East (90) to West (270).
    let azimuth_degrees = 90.0 + progress * 180.0;

    // Elevation follows a sine curve from -20° up to a peak and back down to -20°.
    let peak_elevation = 90.0 - LATITUDE_DEGREES;
    let min_elevation = -20.0;
    let elevation_degrees =
        min_elevation + (peak_elevation - min_elevation) * (progress * std::f32::consts::PI).sin();

    convert_spherical_to_cartesian(elevation_degrees, azimuth_degrees)
}

/// Converts spherical coordinates (elevation, azimuth) to 3D Cartesian coordinates for the game engine.
fn convert_spherical_to_cartesian(elevation_degrees: f32, azimuth_degrees: f32) -> Position {
    let radius = 1000.0;
    let elev_rad = elevation_degrees.to_radians();
    let azim_rad = azimuth_degrees.to_radians();

    let x = radius * elev_rad.cos() * azim_rad.sin();
    let y = -radius * elev_rad.sin(); // Invert Y so positive is down
    let z = -radius * elev_rad.cos() * azim_rad.cos();

    Position { x, y, z }
}

// -------------------------------------------------------------------------------------------------
// Sky, Light, and Star Color Calculations
// -------------------------------------------------------------------------------------------------

fn calculate_sky_colors(sun_elevation: f32, sun_azimuth: f32, moon_elevation: f32) -> SkyColors {
    // Define base colors for the sky model
    const DAY_ZENITH: Color = Color {
        r: 140,
        g: 190,
        b: 240,
    };
    const NIGHT_ZENITH: Color = Color {
        r: 30,
        g: 25,
        b: 55,
    }; // Brighter, violet night
    const SUNSET_COLOR: Color = Color {
        r: 255,
        g: 100,
        b: 20,
    };
    const HORIZON_DAWN_DUSK: Color = Color {
        r: 100,
        g: 80,
        b: 100,
    };
    const NIGHT_HORIZON: Color = Color {
        r: 40,
        g: 35,
        b: 65,
    }; // Brighter, violet night
    const MOON_GLOW_COLOR: Color = Color {
        r: 70,
        g: 70,
        b: 100,
    }; // Ambient glow from moon

    // --- Moonlight Calculation ---
    // Factor for how much "night" it is (sun well below horizon)
    let night_factor = ((-sun_elevation - 8.0) / 10.0).clamp(0.0, 1.0);
    // Factor for how high the moon is in the sky
    let moon_up_factor = (moon_elevation.max(0.0) / 90.0).clamp(0.0, 1.0).powf(0.5);
    // Total moonlight influence
    let moon_influence = night_factor * moon_up_factor;

    // Apply moonlight to the base night colors
    let night_zenith_color =
        interpolate_color(&NIGHT_ZENITH, &MOON_GLOW_COLOR, moon_influence * 0.6);
    let night_horizon_color = interpolate_color(&NIGHT_HORIZON, &MOON_GLOW_COLOR, moon_influence);

    // --- Sun-based Calculation ---
    // Determine the overall day/night transition factor (0.0 for night, 1.0 for day)
    let day_factor = (sun_elevation.max(-18.0) + 18.0) / 36.0; // Smooth transition from -18° to +18°
    let day_factor = day_factor.clamp(0.0, 1.0).powf(0.5); // Use powf for a non-linear curve

    // Calculate sunset/sunrise influence. This factor is 1.0 when the sun is near the
    // horizon and 0.0 when it's high in the sky or deep into night.
    // The effect is centered at 10° elevation and extends from -20° to 40°.
    const SUNSET_ELEVATION_CENTER: f32 = 10.0;
    const SUNSET_ELEVATION_WIDTH: f32 = 30.0; // Extends 30° above and below the center

    let sunset_factor = (1.0
        - ((sun_elevation - SUNSET_ELEVATION_CENTER).abs() / SUNSET_ELEVATION_WIDTH)
            .clamp(0.0, 1.0))
    .powi(2);

    // --- Final Color Blending ---
    // Calculate zenith (top) color by blending from the moonlit night to day, then adding sunset glow.
    let zenith_color = interpolate_color(&night_zenith_color, &DAY_ZENITH, day_factor);
    let zenith_color = interpolate_color(&zenith_color, &SUNSET_COLOR, sunset_factor * 0.4);

    // Calculate horizon (bottom) color
    let horizon_base = interpolate_color(&night_horizon_color, &HORIZON_DAWN_DUSK, day_factor);
    let bottom_color = interpolate_color(&horizon_base, &SUNSET_COLOR, sunset_factor * 0.5);

    // Calculate directional colors
    let north =
        calculate_directional_color(0.0, sun_azimuth, zenith_color, SUNSET_COLOR, sunset_factor);
    let south = calculate_directional_color(
        180.0,
        sun_azimuth,
        zenith_color,
        SUNSET_COLOR,
        sunset_factor,
    );
    let east =
        calculate_directional_color(90.0, sun_azimuth, zenith_color, SUNSET_COLOR, sunset_factor);
    let west = calculate_directional_color(
        270.0,
        sun_azimuth,
        zenith_color,
        SUNSET_COLOR,
        sunset_factor,
    );

    SkyColors {
        north,
        south,
        east,
        west,
        top: zenith_color,
        bottom: bottom_color,
    }
}

fn calculate_directional_color(
    direction_azimuth: f32,
    sun_azimuth: f32,
    base_color: Color,
    sunset_color: Color,
    sunset_factor: f32,
) -> Color {
    // Find the angular difference between the sun and the direction we're coloring
    let angle_diff = 180.0 - ((sun_azimuth - direction_azimuth).abs() - 180.0).abs();

    // The closer the direction is to the sun, the more sunset color we apply
    let directional_sunset_factor = sunset_factor * (1.0 - angle_diff / 180.0).powf(2.0);

    interpolate_color(&base_color, &sunset_color, directional_sunset_factor)
}

fn calculate_light_color(sun_elevation: f32, is_moonlit: bool) -> Color {
    if is_moonlit {
        // Simple moon color - can be expanded later
        return Color {
            r: 150,
            g: 150,
            b: 200,
        };
    }

    // Define key colors for sunlight
    const HIGH_SUN: Color = Color {
        r: 255,
        g: 255,
        b: 255,
    };
    const LOW_SUN: Color = Color {
        r: 255,
        g: 200,
        b: 150,
    };
    const DEEP_NIGHT: Color = Color {
        r: 50,
        g: 50,
        b: 80,
    };

    // Blend from deep night color, to low sun color, to high sun color
    if sun_elevation > 10.0 {
        let factor = (sun_elevation - 10.0) / 80.0; // Transition from 10° to 90°
        interpolate_color(&LOW_SUN, &HIGH_SUN, factor.clamp(0.0, 1.0))
    } else {
        let factor = sun_elevation / 10.0; // Transition from 0° to 10°
        interpolate_color(&DEEP_NIGHT, &LOW_SUN, factor.clamp(0.0, 1.0))
    }
}

fn calculate_star_opacity(sun_elevation: f32) -> u8 {
    // Stars are fully visible when sun is below -12°, fade out by -6°
    let factor = ((-sun_elevation - 6.0) / 6.0).clamp(0.0, 1.0);
    (1.0 + factor * 169.0).round() as u8
}

// =================================================================================================
//                                      HELPER FUNCTIONS
// =================================================================================================

/// Parses a string into an hour value (0.0 to 23.99).
fn parse_time_string(time_str: &str) -> Option<f32> {
    match time_str.to_lowercase().as_str() {
        "dawn" | "sunrise" => Some(6.0),
        "noon" | "midday" => Some(12.0),
        "dusk" | "sunset" => Some(18.0),
        "night" | "midnight" => Some(0.0),
        _ => {
            let parts: Vec<&str> = time_str.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
                    if h <= 23 && m <= 59 {
                        return Some(h as f32 + m as f32 / 60.0);
                    }
                }
            }
            None
        }
    }
}

/// Formats an hour value into a readable HH:MM string.
fn format_time(hour: f32) -> String {
    let h = hour.floor() as u8 % 24;
    let m = ((hour - hour.floor()) * 60.0).round() as u8;
    format!("{:02}:{:02}", h, m)
}

/// Helper to get the current time in a given timezone as a fractional hour.
fn get_current_hour_in_tz(time_zone: Tz) -> f32 {
    let now: DateTime<Tz> = Utc::now().with_timezone(&time_zone);
    now.hour() as f32 + now.minute() as f32 / 60.0 + now.second() as f32 / 3600.0
}

/// Linearly interpolates between two colors.
fn interpolate_color(color1: &Color, color2: &Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    Color {
        r: (color1.r as f32 * (1.0 - factor) + color2.r as f32 * factor) as u8,
        g: (color1.g as f32 * (1.0 - factor) + color2.g as f32 * factor) as u8,
        b: (color1.b as f32 * (1.0 - factor) + color2.b as f32 * factor) as u8,
    }
}

// =================================================================================================
//                                           TESTS
// =================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_continuity() {
        let mut previous_state: Option<WorldTimeState> = None;
        const MAX_TOTAL_CHANGE_PER_MINUTE: f32 = 50.0; // Max allowed RGB change for a sky face.

        // Iterate through a full day in 1-minute increments
        for i in 0..=1440 {
            let hour = i as f32 / 60.0;
            let current_state = calculate_world_state(hour);

            if let Some(prev) = previous_state {
                // Check for jumps in sky colors
                let sky_faces = [
                    (current_state.sky_colors.top, prev.sky_colors.top),
                    (current_state.sky_colors.bottom, prev.sky_colors.bottom),
                    (current_state.sky_colors.north, prev.sky_colors.north),
                    (current_state.sky_colors.south, prev.sky_colors.south),
                    (current_state.sky_colors.east, prev.sky_colors.east),
                    (current_state.sky_colors.west, prev.sky_colors.west),
                ];

                for (current_color, prev_color) in sky_faces.iter() {
                    let r_diff = (current_color.r as f32 - prev_color.r as f32).abs();
                    let g_diff = (current_color.g as f32 - prev_color.g as f32).abs();
                    let b_diff = (current_color.b as f32 - prev_color.b as f32).abs();
                    let total_diff = r_diff + g_diff + b_diff;

                    assert!(
                        total_diff <= MAX_TOTAL_CHANGE_PER_MINUTE,
                        "Abrupt sky color change detected at hour {:.2}. Change: {:.1}",
                        hour,
                        total_diff
                    );
                }
            }

            previous_state = Some(current_state);
        }
    }

    #[test]
    fn test_color_realism() {
        // --- Test Noon (12:00) ---
        let noon = calculate_world_state(12.0);
        // Sky top should be blue
        assert!(
            noon.sky_colors.top.b > noon.sky_colors.top.r,
            "Noon sky top should be blue."
        );
        assert!(
            noon.sky_colors.top.b > noon.sky_colors.top.g,
            "Noon sky top should be blue."
        );
        // Sky should be bright
        assert!(noon.sky_colors.top.b > 150, "Noon sky should be bright.");
        // Light should be bright white
        assert!(
            noon.light_color.r > 240 && noon.light_color.g > 240 && noon.light_color.b > 240,
            "Noon light should be white."
        );
        // Should be sun
        assert_eq!(noon.light_texture, "c_sun");
        // Stars should be hidden
        assert!(noon.star_opacity <= 1, "Noon stars should be hidden.");

        // --- Test Midnight (0:00) ---
        let midnight = calculate_world_state(0.0);
        // Sky should be very dark, but not pure black
        assert!(
            midnight.sky_colors.top.b > 1 && midnight.sky_colors.top.b < 50,
            "Midnight sky should be dark blue."
        );
        let total_brightness = midnight.sky_colors.top.r as u16
            + midnight.sky_colors.top.g as u16
            + midnight.sky_colors.top.b as u16;
        assert!(
            total_brightness > 10,
            "Midnight sky should not be pure black."
        );
        // Light should be dim and cool (moonlight)
        assert!(
            midnight.light_color.b > midnight.light_color.r,
            "Moonlight should be cool."
        );
        assert!(midnight.light_color.r < 200, "Moonlight should be dim.");
        // Should be moon
        assert_eq!(midnight.light_texture, "c_moon2");
        // Stars should be visible
        assert!(
            midnight.star_opacity > 150,
            "Midnight stars should be visible."
        );

        // --- Test Sunset (18:00) ---
        let sunset = calculate_world_state(18.0);
        // West sky should be reddish/orange
        assert!(
            sunset.sky_colors.west.r > sunset.sky_colors.west.b,
            "Sunset sky (west) should be reddish."
        );
        // East sky should be dark and blueish/purplish
        assert!(
            sunset.sky_colors.east.b > sunset.sky_colors.east.r,
            "Twilight sky (east) should be blueish."
        );
        // Light source should now be the moon
        assert_eq!(sunset.light_texture, "c_moon2");

        // --- Test Sunrise (6:00) ---
        let sunrise = calculate_world_state(6.0);
        // East sky should be reddish/orange
        assert!(
            sunrise.sky_colors.east.r > sunrise.sky_colors.east.b,
            "Sunrise sky (east) should be reddish."
        );
        // West sky should be dark and blueish/purplish
        assert!(
            sunrise.sky_colors.west.b > sunrise.sky_colors.west.r,
            "Twilight sky (west) should be blueish."
        );
        // Light source should now be the sun
        assert_eq!(sunrise.light_texture, "c_sun");
    }
}
