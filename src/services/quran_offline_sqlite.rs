use crate::services::quran_api::{self, CompleteQuranData, SurahData};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS complete_qurans (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
);
";

pub struct SqliteQuranStorage {
    conn: rusqlite::Connection,
}

impl SqliteQuranStorage {
    pub fn new() -> Self {
        let db_path = crate::platform::storage::data_dir()
            .join("nostr-blue")
            .join("quran_offline.db");
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = rusqlite::Connection::open(&db_path)
            .expect("Failed to open Quran offline database");
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .expect("Failed to set SQLite pragmas");
        conn.execute_batch(SCHEMA)
            .expect("Failed to create Quran offline schema");
        Self { conn }
    }
}

#[async_trait::async_trait(?Send)]
impl super::quran_offline::QuranOfflineStorage for SqliteQuranStorage {
    async fn save_complete_quran(
        &self,
        edition_id: &str,
        data: &CompleteQuranData,
    ) -> Result<(), String> {
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize quran: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO complete_qurans (id, data) VALUES (?1, ?2)",
                rusqlite::params![edition_id, json],
            )
            .map_err(|e| format!("Failed to save quran: {}", e))?;
        Ok(())
    }

    async fn load_surah(&self, edition: &str, surah: u32) -> Option<SurahData> {
        let json: String = self
            .conn
            .query_row(
                "SELECT data FROM complete_qurans WHERE id = ?1",
                rusqlite::params![edition],
                |row| row.get(0),
            )
            .ok()?;
        let complete: CompleteQuranData = serde_json::from_str(&json).ok()?;
        quran_api::build_surah_from_offline(&complete, surah)
    }

    async fn delete_edition(&self, edition: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM complete_qurans WHERE id = ?1",
                rusqlite::params![edition],
            )
            .map_err(|e| format!("Failed to delete edition: {}", e))?;
        Ok(())
    }

    async fn list_downloaded(&self) -> Vec<String> {
        let mut stmt = match self.conn.prepare("SELECT id FROM complete_qurans") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |row| row.get::<_, String>(0));
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    }
}
