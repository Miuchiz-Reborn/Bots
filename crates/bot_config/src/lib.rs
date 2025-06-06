use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BotConfig {
    pub host: String,
    pub port: u16,

    pub owner_id: u32,
    pub privilege_password: String,
}
