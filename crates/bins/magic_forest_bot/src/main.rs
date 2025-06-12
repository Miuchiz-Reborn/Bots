use std::time::Duration;

use obstacle_course::{ObstacleBot, ObstacleBotConfig};
// =================================================================================================
//                                         CONFIGURATION
// =================================================================================================

// --- Game Settings ---
const TOTAL_CHECKPOINTS: u32 = 11;

// --- World and Position Settings ---
const MION_WORLD: &str = "MION";
const MAGIC_FOREST_WORLD: &str = "pawzobst";
const MAGIC_FOREST_SPAWN_X: i32 = -55300;
const MAGIC_FOREST_SPAWN_Y: i32 = 1000;
const MAGIC_FOREST_SPAWN_Z: i32 = 1600;
const MAGIC_FOREST_SPAWN_ROTATION: i32 = 880;
const MION_RETURN_SPAWN_X: i32 = -144310;
const MION_RETURN_SPAWN_Y: i32 = -17900;
const MION_RETURN_SPAWN_Z: i32 = -20000;
const MION_RETURN_SPAWN_ROTATION: i32 = 900;
const TICKET_TAKER_X: i32 = -143584;
const TICKET_TAKER_Y: i32 = -17964;
const TICKET_TAKER_Z: i32 = -20218;

// =================================================================================================
//                                          ENTRYPOINT
// =================================================================================================

use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MagicForestBotConfig {
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
        .and_then(|s| toml::from_str::<MagicForestBotConfig>(&s).map_err(|e| e.to_string()))
    {
        Ok(config) => config,
        Err(e) => {
            println!("Failed to load config: {}", e);
            return;
        }
    };

    loop {
        let obstacle_bot_config = ObstacleBotConfig {
            game_name: "MagicForest".to_string(),
            tagline: None,
            ticket_price: 5,
            min_players: 1,
            ticket_world_name: MION_WORLD.to_string(),
            game_world_name: MAGIC_FOREST_WORLD.to_string(),
            ticket_taker_pos: (TICKET_TAKER_X, TICKET_TAKER_Y, TICKET_TAKER_Z),
            game_spawn_pos: (
                MAGIC_FOREST_SPAWN_X,
                MAGIC_FOREST_SPAWN_Y,
                MAGIC_FOREST_SPAWN_Z,
                MAGIC_FOREST_SPAWN_ROTATION,
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
            bump_keyword: "PawzRacer".to_string(),
            sign_keyword: "WinnerMagicForest".to_string(),
            welcome_messages: vec![
                "Welcome to the Magic Forest, where you must find the pot of gold.  You must pass all the waypoints in the numbered order to unlock the secret of the forest. You will have 6 minutes to pass all the checkpoints (IN ORDER!) to find the pot of gold."
                    .to_string(),
                "Along the way there are many traps to watch out for.  You can also set off many traps for your fellow players who want to beat you! Good luck, may the spirit of the forest help you along your journey."
                    .to_string(),
                "Go! Make sure you find all the checkpoints by going under the yellow numbers or you will not be allowed to the pot of gold."
                    .to_string(),
            ],
            win_game_message: Box::new(|winner_name, time_in_seconds| {
                format!("{winner_name} wins, finding the pot of gold in {time_in_seconds} seconds!")
            }),
            thirty_second_warning_message: "You only have 30 seconds left to find the pot of gold - hurry!".to_string(),
            ticket_taker_action: "~TicketTaker=MagicForest~".to_string(),
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
