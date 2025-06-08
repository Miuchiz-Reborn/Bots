use aw_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct CharacterServerConfig {
    pub host: String,
    pub port: u16,
    pub database: DatabaseConfig,
}
