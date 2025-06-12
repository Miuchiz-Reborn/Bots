use std::time::Duration;

use obstacle_course::{ObstacleBot, ObstacleBotConfig};
// =================================================================================================
//                                         CONFIGURATION
// =================================================================================================

// --- Game Settings ---
const TOTAL_CHECKPOINTS: u32 = 11;

// --- World and Position Settings ---
const MION_WORLD: &str = "MION";
const MALL_RACE_WORLD: &str = "bratzobs";
const MALL_RACE_SPAWN_X: i32 = -1650;
const MALL_RACE_SPAWN_Y: i32 = 64;
const MALL_RACE_SPAWN_Z: i32 = 1650;
const MALL_RACE_SPAWN_ROTATION: i32 = 950;
const MION_RETURN_SPAWN_X: i32 = 32388;
const MION_RETURN_SPAWN_Y: i32 = -14488;
const MION_RETURN_SPAWN_Z: i32 = -59313;
const MION_RETURN_SPAWN_ROTATION: i32 = 2325;
const TICKET_TAKER_X: i32 = 31500;
const TICKET_TAKER_Y: i32 = -14290;
const TICKET_TAKER_Z: i32 = -59738;

// =================================================================================================
//                                          ENTRYPOINT
// =================================================================================================

use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MallRaceBotConfig {
    pub host: String,
    pub port: u16,
    pub character_host: String,
    pub character_port: u16,

    pub owner_id: u32,
    pub privilege_password: String,
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct Args {
    config_path: String,
}

fn main() {
    let args = Args::parse();
    let config = match std::fs::read_to_string(&args.config_path)
        .map_err(|e| e.to_string())
        .and_then(|s| toml::from_str::<MallRaceBotConfig>(&s).map_err(|e| e.to_string()))
    {
        Ok(config) => config,
        Err(e) => {
            println!("Failed to load config: {}", e);
            return;
        }
    };

    loop {
        let obstacle_bot_config = ObstacleBotConfig {
            game_name: "MallRace".to_string(),
            tagline: None,
            ticket_price: 5,
            min_players: 1,
            ticket_world_name: MION_WORLD.to_string(),
            game_world_name: MALL_RACE_WORLD.to_string(),
            ticket_taker_pos: (TICKET_TAKER_X, TICKET_TAKER_Y, TICKET_TAKER_Z),
            game_spawn_pos: (
                MALL_RACE_SPAWN_X,
                MALL_RACE_SPAWN_Y,
                MALL_RACE_SPAWN_Z,
                MALL_RACE_SPAWN_ROTATION,
            ),
            mion_return_spawn_pos: (
                MION_RETURN_SPAWN_X,
                MION_RETURN_SPAWN_Y,
                MION_RETURN_SPAWN_Z,
                MION_RETURN_SPAWN_ROTATION,
            ),
            total_checkpoints: TOTAL_CHECKPOINTS,
            host: config.host.clone(),
            port: config.port,
            character_host: config.character_host.clone(),
            character_port: config.character_port,
            owner_id: config.owner_id,
            privilege_password: config.privilege_password.clone(),
            bump_keyword: "MallRace".to_string(),
            sign_keyword: "WinnerMallRace".to_string(),
            welcome_messages: vec![
                "Run through the Mall and solve all the puzzles while collecting the numbers.  You will have 6 minutes to solve all the puzzles.  Make sure you hit all the numbers before going to the next puzzle."
                    .to_string(),
                "Good luck, and happy shopping!"
                    .to_string(),
                "Go! Solve all the puzzles, don't forget to hit all the numbers."
                    .to_string(),
            ],
            win_game_message: Box::new(|winner_name, time_in_seconds| {
                format!("{winner_name} wins, solving all the puzzles in {time_in_seconds} seconds!")
            }),
            thirty_second_warning_message: "You only have 30 seconds left to solve all the puzzles - hurry!".to_string(),
            ticket_taker_action: "~shopTicketTaker=MallRace~".to_string(),
            ad_no_players_interval: Duration::from_secs(10 * 60),
            ad_waiting_interval: Duration::from_secs(60),
            ad_post_game_delay: Duration::from_secs(5),
        };
        match ObstacleBot::new(obstacle_bot_config) {
            Ok(mut bot) => {
                if let Err(e) = bot.run() {
                    println!("Bot encountered an error: {:?}. Restarting.", e);
                }
            }
            Err(e) => {
                println!("Failed to initialize bot: {:?}. Retrying.", e);
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}
