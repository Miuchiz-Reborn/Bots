pub use aw_db::{
    Database, DatabaseConfig, DatabaseResult, DatabaseType, MysqlConfig, SqliteConfig,
};

pub struct MiuchizDatabase {
    db: Database,
}

#[derive(Debug, Clone)]
pub struct MiuchizStats {
    pub creditz: u32,
    pub happiness: u32,
    pub hunger: u32,
    pub boredom: u32,
}

impl MiuchizDatabase {
    pub fn new(config: DatabaseConfig) -> Self {
        let db = Database::new(config).unwrap();
        let result = Self { db };

        result.create_tables();

        result
    }

    fn create_tables(&self) -> DatabaseResult<()> {
        let result = self.db.exec(
            "CREATE TABLE IF NOT EXISTS miuchiz_stats (
            citizen_id INTEGER PRIMARY KEY NOT NULL,
            creditz INTEGER NOT NULL DEFAULT 0,
            happiness INTEGER NOT NULL DEFAULT 0,
            hunger INTEGER NOT NULL DEFAULT 0,
            boredom INTEGER NOT NULL DEFAULT 0);",
            vec![],
        );

        match result {
            DatabaseResult::Ok(_) => {}
            DatabaseResult::DatabaseError => {
                return DatabaseResult::DatabaseError;
            }
        }

        DatabaseResult::Ok(())
    }

    pub fn init_player_if_not_exists(&self, citizen_id: u32) -> DatabaseResult<()> {
        let result = self.db.exec(
            "INSERT INTO miuchiz_stats (citizen_id) VALUES (?)",
            vec![citizen_id.to_string()],
        );

        match result {
            DatabaseResult::Ok(_) => DatabaseResult::Ok(()),
            DatabaseResult::DatabaseError => DatabaseResult::DatabaseError,
        }
    }

    pub fn get_stats(&self, citizen_id: u32) -> DatabaseResult<MiuchizStats> {
        let result = self.db.exec(
            "SELECT * FROM miuchiz_stats WHERE citizen_id = ?",
            vec![citizen_id.to_string()],
        );

        let rows = match result {
            DatabaseResult::Ok(rows) => rows,
            DatabaseResult::DatabaseError => return DatabaseResult::DatabaseError,
        };

        if rows.len() > 1 {
            return DatabaseResult::DatabaseError;
        }

        let row = match rows.first() {
            Some(row) => row,
            None => return DatabaseResult::DatabaseError,
        };

        let Some(creditz_i64) = row.fetch_int("creditz") else {
            return DatabaseResult::DatabaseError;
        };

        let Some(happiness_i64) = row.fetch_int("happiness") else {
            return DatabaseResult::DatabaseError;
        };

        let Some(hunger_i64) = row.fetch_int("hunger") else {
            return DatabaseResult::DatabaseError;
        };

        let Some(boredom_i64) = row.fetch_int("boredom") else {
            return DatabaseResult::DatabaseError;
        };

        let creditz = u32::try_from(creditz_i64).unwrap_or(0);
        let happiness = u32::try_from(happiness_i64).unwrap_or(0);
        let hunger = u32::try_from(hunger_i64).unwrap_or(0);
        let boredom = u32::try_from(boredom_i64).unwrap_or(0);

        let stats = MiuchizStats {
            creditz,
            happiness,
            hunger,
            boredom,
        };

        DatabaseResult::Ok(stats)
    }

    pub fn set_stats(&self, citizen_id: u32, stats: &MiuchizStats) -> DatabaseResult<()> {
        let result = self.db.exec(
            "UPDATE miuchiz_stats SET creditz = ?, happiness = ?, hunger = ?, boredom = ? WHERE citizen_id = ?",
            vec![
                stats.creditz.to_string(),
                stats.happiness.to_string(),
                stats.hunger.to_string(),
                stats.boredom.to_string(),
                citizen_id.to_string()
            ],
        );

        match result {
            DatabaseResult::Ok(_) => DatabaseResult::Ok(()),
            DatabaseResult::DatabaseError => DatabaseResult::DatabaseError,
        }
    }
}
