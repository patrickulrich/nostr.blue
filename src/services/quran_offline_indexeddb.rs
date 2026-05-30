use crate::services::quran_api::{self, CompleteQuranData, SurahData};
use indexed_db_futures::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

const DB_NAME: &str = "nostr_blue_quran_offline";
const DB_VERSION: u32 = 1;
const STORE: &str = "complete_qurans";

pub struct IndexedDbQuranStorage;

impl IndexedDbQuranStorage {
    pub fn new() -> Self {
        Self
    }

    async fn open_db(&self) -> Result<IdbDatabase, String> {
        let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
            .map_err(|e| format!("Failed to open Quran DB: {:?}", e))?;
        db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
            let db = evt.db();
            if !db.object_store_names().any(|n| n == STORE) {
                db.create_object_store(STORE)
                    .expect("Failed to create store");
            }
            Ok(())
        }));
        db_req
            .await
            .map_err(|e| format!("Failed to open Quran DB: {:?}", e))
    }
}

#[async_trait::async_trait(?Send)]
impl super::quran_offline::QuranOfflineStorage for IndexedDbQuranStorage {
    async fn save_complete_quran(
        &self,
        edition_id: &str,
        data: &CompleteQuranData,
    ) -> Result<(), String> {
        let db = self.open_db().await?;
        let tx = db
            .transaction_on_one_with_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| format!("Failed to start transaction: {:?}", e))?;
        let store = tx
            .object_store(STORE)
            .map_err(|e| format!("Failed to get store: {:?}", e))?;
        let json_str = serde_json::to_string(data)
            .map_err(|e| format!("Failed to serialize quran: {}", e))?;
        let js_key = JsValue::from_str(edition_id);
        let js_val = JsValue::from_str(&json_str);
        store
            .put_key_val(&js_key, &js_val)
            .map_err(|e| format!("Failed to put quran: {:?}", e))?;
        tx.await
            .into_result()
            .map_err(|e| format!("Transaction failed: {:?}", e))?;
        Ok(())
    }

    async fn load_surah(&self, edition: &str, surah: u32) -> Option<SurahData> {
        let db = self.open_db().await.ok()?;
        let tx = db
            .transaction_on_one_with_mode(STORE, IdbTransactionMode::Readonly)
            .ok()?;
        let store = tx.object_store(STORE).ok()?;
        let js_key = JsValue::from_str(edition);
        let req = store.get(&js_key).ok()?;
        let value = req.await.ok()??;
        let json_str = value.as_string()?;
        let complete: CompleteQuranData = serde_json::from_str(&json_str).ok()?;
        quran_api::build_surah_from_offline(&complete, surah)
    }

    async fn delete_edition(&self, edition: &str) -> Result<(), String> {
        let db = self.open_db().await?;
        let tx = db
            .transaction_on_one_with_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| format!("Failed to start transaction: {:?}", e))?;
        let store = tx
            .object_store(STORE)
            .map_err(|e| format!("Failed to get store: {:?}", e))?;
        let js_key = JsValue::from_str(edition);
        store
            .delete(&js_key)
            .map_err(|e| format!("Failed to delete edition: {:?}", e))?;
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
        let all_keys_req = match store.get_all_keys() {
            Ok(req) => req,
            Err(_) => return Vec::new(),
        };
        let all_keys = match all_keys_req.await {
            Ok(keys) => keys,
            Err(_) => return Vec::new(),
        };
        js_sys::try_iter(&all_keys)
            .ok()
            .and_then(|i| i)
            .map(|iter| iter.filter_map(|v| v.ok()).filter_map(|v| v.as_string()).collect())
            .unwrap_or_default()
    }
}
