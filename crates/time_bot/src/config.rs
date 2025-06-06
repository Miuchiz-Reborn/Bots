use bot_config::BotConfig;
use serde::{Deserialize, Serialize};

/// Represents the top-level configuration from the TOML file.
#[derive(Debug, Serialize, Deserialize)]
pub struct TimeBotConfig {
    /// Holds the base bot connection and authentication details.
    /// Corresponds to the `[bot_config]` table in the TOML file.
    pub bot_config: BotConfig,

    /// Holds settings specific to the Time Bot.
    /// Corresponds to the `[time_bot_config]` table in the TOML file.
    pub time_bot_config: TimeBotSpecificConfig,
}

/// Contains settings specific to the Time Bot's functionality.
#[derive(Debug, Serialize, Deserialize)]
pub struct TimeBotSpecificConfig {
    pub time_zone: String,
    pub world: String,
    pub update_ms: u64,
}
