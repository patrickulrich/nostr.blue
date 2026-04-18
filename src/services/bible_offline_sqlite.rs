use crate::services::bible_api::{self, ChapterResponse, TranslationComplete};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS complete_translations (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
);
";

pub struct SqliteBibleStorage {
    conn: Connection,
}

use rusqlite::Connection;

impl SqliteBibleStorage {
    pub fn new() -> Self {
        let db_path =
            crate::platform::storage::data_dir().join("nostr-blue").join("bible_offline.db");
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&db_path).expect("Failed to open Bible offline database");
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .expect("Failed to set SQLite pragmas");
        conn.execute_batch(SCHEMA)
            .expect("Failed to create Bible offline schema");
        let _ = conn.execute_batch("DROP TABLE IF EXISTS translations; DROP TABLE IF EXISTS chapters;");
        Self { conn }
    }
}

#[async_trait::async_trait(?Send)]
impl super::bible_offline::BibleOfflineStorage for SqliteBibleStorage {
    async fn save_complete_translation(
        &self,
        translation_id: &str,
        data: &TranslationComplete,
    ) -> Result<(), String> {
        let json = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize translation: {}", e))?;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO complete_translations (id, data) VALUES (?1, ?2)",
                rusqlite::params![translation_id, json],
            )
            .map_err(|e| format!("Failed to save translation: {}", e))?;
        Ok(())
    }

    async fn load_chapter(
        &self,
        translation: &str,
        book: &str,
        chapter: u32,
    ) -> Option<ChapterResponse> {
        let json: String = self
            .conn
            .query_row(
                "SELECT data FROM complete_translations WHERE id = ?1",
                rusqlite::params![translation],
                |row| row.get(0),
            )
            .ok()?;
        let complete: TranslationComplete = serde_json::from_str(&json).ok()?;
        bible_api::build_chapter_response_from_offline(&complete, book, chapter)
    }

    async fn delete_translation(&self, translation: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM complete_translations WHERE id = ?1",
                rusqlite::params![translation],
            )
            .map_err(|e| format!("Failed to delete translation: {}", e))?;
        Ok(())
    }

    async fn list_downloaded(&self) -> Vec<String> {
        let mut stmt = match self.conn.prepare("SELECT id FROM complete_translations") {
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
