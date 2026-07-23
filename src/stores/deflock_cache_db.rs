//! DeFlock ALPR Camera Cache Database
//!
//! Lightweight IndexedDB wrapper for persisting fetched ALPR cameras + bbox
//! coverage across sessions. Mirrors the `shop_database.rs` pattern.
//!
//! ## Storage Model
//!
//! - `cameras` store: key = `osm_id` (as string), value = serialized `AlprCamera` JSON
//! - `fetched_bboxes` store: key = `"{south},{west},{north},{east}"`, value = `CachedBbox`

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, unused_imports, unused_variables)
)]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use crate::services::deflock::AlprCamera;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedBbox {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
    pub fetched_at: u64,
}

#[cfg(not(target_arch = "wasm32"))]
mod native_stub {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct DeflockCacheDb;

    impl DeflockCacheDb {
        pub async fn new() -> Result<Self, String> {
            Err("DeflockCacheDb not available on native targets".to_string())
        }

        pub async fn bulk_insert_cameras(&self, _cameras: &[AlprCamera]) -> Result<(), String> {
            Ok(())
        }

        pub async fn get_all_cameras(&self) -> Result<Vec<AlprCamera>, String> {
            Ok(Vec::new())
        }

        pub async fn insert_bbox(&self, _bbox: &CachedBbox) -> Result<(), String> {
            Ok(())
        }

        pub async fn get_all_bboxes(&self) -> Result<Vec<CachedBbox>, String> {
            Ok(Vec::new())
        }

        pub async fn clear(&self) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_stub::DeflockCacheDb;

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
mod wasm_impl {
    use super::*;
    use indexed_db_futures::prelude::*;
    use serde::de::DeserializeOwned;
    use std::future::IntoFuture;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;
    use web_sys::IdbTransactionMode;

    const DB_NAME: &str = "nostr_blue_deflock";
    const DB_VERSION: u32 = 1;
    const STORE_CAMERAS: &str = "cameras";
    const STORE_BBOXES: &str = "fetched_bboxes";

    #[derive(Clone, Debug)]
    pub struct DeflockCacheDb {
        db: Rc<IdbDatabase>,
    }

    unsafe impl Send for DeflockCacheDb {}
    unsafe impl Sync for DeflockCacheDb {}

    impl DeflockCacheDb {
        fn make_error(msg: String) -> String {
            msg
        }

        pub async fn new() -> Result<Self, String> {
            let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
                .map_err(|e| format!("Failed to open deflock database: {:?}", e))?;
            db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
                let db = evt.db();
                if !db.object_store_names().any(|n| n == STORE_CAMERAS) {
                    db.create_object_store(STORE_CAMERAS)
                        .expect("Failed to create cameras store");
                }
                if !db.object_store_names().any(|n| n == STORE_BBOXES) {
                    db.create_object_store(STORE_BBOXES)
                        .expect("Failed to create fetched_bboxes store");
                }
                Ok(())
            }));
            let db: IdbDatabase = db_req
                .into_future()
                .await
                .map_err(|e| format!("Failed to open deflock database: {:?}", e))?;
            log::info!("Deflock cache database initialized successfully");
            Ok(Self { db: Rc::new(db) })
        }

        pub async fn bulk_insert_cameras(&self, cameras: &[AlprCamera]) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_CAMERAS, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_CAMERAS)
                .map_err(|e| format!("Store error: {:?}", e))?;
            for cam in cameras {
                let key = JsValue::from_str(&cam.osm_id.to_string());
                let json_str = serde_json::to_string(cam)
                    .map_err(|e| format!("Serialization error: {}", e))?;
                let value = JsValue::from_str(&json_str);
                store
                    .put_key_val(&key, &value)
                    .map_err(|e| format!("Put error: {:?}", e))?;
            }
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }

        async fn get_all_values<T: DeserializeOwned>(
            &self,
            store_name: &str,
        ) -> Result<Vec<T>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let values = store
                .get_all()
                .map_err(|e| format!("Get all error: {:?}", e))?
                .await
                .map_err(|e| format!("Get all await error: {:?}", e))?;
            let mut results = Vec::new();
            for value in values {
                let json_str = value
                    .as_string()
                    .ok_or_else(|| "Value is not a string".to_string())?;
                let parsed: T = serde_json::from_str(&json_str)
                    .map_err(|e| format!("Deserialization error: {}", e))?;
                results.push(parsed);
            }
            Ok(results)
        }

        pub async fn get_all_cameras(&self) -> Result<Vec<AlprCamera>, String> {
            self.get_all_values(STORE_CAMERAS).await
        }

        pub async fn insert_bbox(&self, bbox: &CachedBbox) -> Result<(), String> {
            let key = format!(
                "{},{},{},{}",
                bbox.south, bbox.west, bbox.north, bbox.east
            );
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_BBOXES, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_BBOXES)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let json_str = serde_json::to_string(bbox)
                .map_err(|e| format!("Serialization error: {}", e))?;
            let js_key = JsValue::from_str(&key);
            let js_value = JsValue::from_str(&json_str);
            store
                .put_key_val(&js_key, &js_value)
                .map_err(|e| format!("Put error: {:?}", e))?;
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }

        pub async fn get_all_bboxes(&self) -> Result<Vec<CachedBbox>, String> {
            self.get_all_values(STORE_BBOXES).await
        }

        pub async fn clear(&self) -> Result<(), String> {
            for store_name in [STORE_CAMERAS, STORE_BBOXES] {
                let tx = self
                    .db
                    .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
                    .map_err(|e| format!("Transaction error: {:?}", e))?;
                let store = tx
                    .object_store(store_name)
                    .map_err(|e| format!("Store error: {:?}", e))?;
                store
                    .clear()
                    .map_err(|e| format!("Clear error: {:?}", e))?;
                tx.await
                    .into_result()
                    .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            }
            Ok(())
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub use wasm_impl::DeflockCacheDb;
