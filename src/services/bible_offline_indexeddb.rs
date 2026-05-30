use crate::services::bible_api::{self, ChapterResponse, TranslationComplete};
use indexed_db_futures::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

const DB_NAME: &str = "nostr_blue_bible_offline";
const DB_VERSION: u32 = 2;
const STORE: &str = "complete_translations";

pub struct IndexedDbBibleStorage;

impl IndexedDbBibleStorage {
    pub fn new() -> Self {
        Self
    }

    async fn open_db(&self) -> Result<IdbDatabase, String> {
        let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
            .map_err(|e| format!("Failed to open Bible DB: {:?}", e))?;
        db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
            let db = evt.db();
            if !db.object_store_names().any(|n| n == STORE) {
                db.create_object_store(STORE)
                    .expect("Failed to create store");
            }
            let old_stores = ["translations", "chapters"];
            for old in old_stores {
                if db.object_store_names().any(|n| n == old) {
                    let _ = db.delete_object_store(old);
                }
            }
            Ok(())
        }));
        db_req
            .await
            .map_err(|e| format!("Failed to open Bible DB: {:?}", e))
    }
}

#[async_trait::async_trait(?Send)]
impl super::bible_offline::BibleOfflineStorage for IndexedDbBibleStorage {
    async fn save_complete_translation(
        &self,
        translation_id: &str,
        data: &TranslationComplete,
    ) -> Result<(), String> {
        let db = self.open_db().await?;
        let tx = db
            .transaction_on_one_with_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| format!("Failed to start transaction: {:?}", e))?;
        let store = tx
            .object_store(STORE)
            .map_err(|e| format!("Failed to get store: {:?}", e))?;
        let json_str = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize translation: {}", e))?;
        let js_key = JsValue::from_str(translation_id);
        let js_val = JsValue::from_str(&json_str);
        store
            .put_key_val(&js_key, &js_val)
            .map_err(|e| format!("Failed to put translation: {:?}", e))?;
        tx.await
            .into_result()
            .map_err(|e| format!("Transaction failed: {:?}", e))?;
        Ok(())
    }

    async fn load_chapter(
        &self,
        translation: &str,
        book: &str,
        chapter: u32,
    ) -> Option<ChapterResponse> {
        let db = self.open_db().await.ok()?;
        let tx = db
            .transaction_on_one_with_mode(STORE, IdbTransactionMode::Readonly)
            .ok()?;
        let store = tx.object_store(STORE).ok()?;
        let js_key = JsValue::from_str(translation);
        let req = store.get(&js_key).ok()?;
        let value = req.await.ok()??;
        let json_str = value.as_string()?;
        let complete: TranslationComplete = serde_json::from_str(&json_str).ok()?;
        bible_api::build_chapter_response_from_offline(&complete, book, chapter)
    }

    async fn delete_translation(&self, translation: &str) -> Result<(), String> {
        let db = self.open_db().await?;
        let tx = db
            .transaction_on_one_with_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| format!("Failed to start transaction: {:?}", e))?;
        let store = tx
            .object_store(STORE)
            .map_err(|e| format!("Failed to get store: {:?}", e))?;
        let js_key = JsValue::from_str(translation);
        store
            .delete(&js_key)
            .map_err(|e| format!("Failed to delete translation: {:?}", e))?;
        tx.await
            .into_result()
            .map_err(|e| format!("Transaction failed: {:?}", e))?;
        Ok(())
    }

    async fn list_downloaded(&self) -> Vec<String> {
        let db = match self.open_db().await {
            Ok(db) => db,
            Err(_) => return Vec::new(),
        };
        let tx = match db.transaction_on_one_with_mode(STORE, IdbTransactionMode::Readonly) {
            Ok(tx) => tx,
            Err(_) => return Vec::new(),
        };
        let store = match tx.object_store(STORE) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let all_req = match store.get_all() {
            Ok(req) => req,
            Err(_) => return Vec::new(),
        };
        let all = match all_req.await {
            Ok(vals) => vals,
            Err(_) => return Vec::new(),
        };
        js_sys::try_iter(&all)
            .ok()
            .and_then(|i| i)
            .map(|iter| {
                iter.filter_map(|v| v.ok())
                    .filter_map(|v| v.as_string())
                    .filter_map(|json_str| {
                        let v: serde_json::Value = serde_json::from_str(&json_str).ok()?;
                        v.get("translation")?.get("id")?.as_str().map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
