//! Shop Order Database
//!
//! Lightweight IndexedDB wrapper for shop order persistence.
//! Extracted from indexeddb_database.rs to decouple shop storage from Cashu wallet.

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, unused_imports, unused_variables)
)]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

#[cfg(not(target_arch = "wasm32"))]
mod native_stub {
    use crate::utils::nip99::ShopOrder;
    use std::collections::HashMap;

    #[derive(Clone, Debug)]
    pub struct ShopDatabase;

    impl ShopDatabase {
        pub async fn new() -> Result<Self, String> {
            Err("ShopDatabase not available on native targets".to_string())
        }

        pub async fn save_order(&self, _order: &ShopOrder) -> Result<(), String> {
            Ok(())
        }

        pub async fn get_order(
            &self,
            _order_id: &str,
        ) -> Result<Option<ShopOrder>, String> {
            Ok(None)
        }

        pub async fn get_all_orders(&self) -> Result<Vec<ShopOrder>, String> {
            Ok(Vec::new())
        }

        pub async fn update_order(&self, _order: &ShopOrder) -> Result<(), String> {
            Ok(())
        }

        pub async fn delete_order(&self, _order_id: &str) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_stub::ShopDatabase;

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
mod wasm_impl {
    use crate::utils::nip99::ShopOrder;
    use indexed_db_futures::prelude::*;
    use serde::{de::DeserializeOwned, Serialize};
    use std::future::IntoFuture;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;
    use web_sys::IdbTransactionMode;

    const DB_NAME: &str = "nostr_blue_shop";
    const DB_VERSION: u32 = 1;
    const STORE_SHOP_ORDERS: &str = "shop_orders";

    #[derive(Clone, Debug)]
    pub struct ShopDatabase {
        db: Rc<IdbDatabase>,
    }

    unsafe impl Send for ShopDatabase {}
    unsafe impl Sync for ShopDatabase {}

    impl ShopDatabase {
        fn make_error(msg: String) -> String {
            msg
        }

        pub async fn new() -> Result<Self, String> {
            let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
                .map_err(|e| format!("Failed to open shop database: {:?}", e))?;
            db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
                let db = evt.db();
                if !db.object_store_names().any(|n| n == STORE_SHOP_ORDERS) {
                    db.create_object_store(STORE_SHOP_ORDERS)
                        .expect("Failed to create shop_orders store");
                }
                Ok(())
            }));
            let db: IdbDatabase = db_req
                .into_future()
                .await
                .map_err(|e| format!("Failed to open shop database: {:?}", e))?;
            log::info!("Shop database initialized successfully");
            Ok(Self { db: Rc::new(db) })
        }

        async fn put_value<T: Serialize>(
            &self,
            store_name: &str,
            key: &str,
            value: &T,
        ) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let json_str = serde_json::to_string(value)
                .map_err(|e| format!("Serialization error: {}", e))?;
            let js_key = JsValue::from_str(key);
            let js_value = JsValue::from_str(&json_str);
            store
                .put_key_val(&js_key, &js_value)
                .map_err(|e| format!("Put error: {:?}", e))?;
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }

        async fn get_value<T: DeserializeOwned>(
            &self,
            store_name: &str,
            key: &str,
        ) -> Result<Option<T>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let js_key = JsValue::from_str(key);
            let value_opt = store
                .get(&js_key)
                .map_err(|e| format!("Get error: {:?}", e))?
                .await
                .map_err(|e| format!("Get await error: {:?}", e))?;
            if let Some(value) = value_opt {
                let json_str = value
                    .as_string()
                    .ok_or_else(|| "Value is not a string".to_string())?;
                let parsed: T = serde_json::from_str(&json_str)
                    .map_err(|e| format!("Deserialization error: {}", e))?;
                Ok(Some(parsed))
            } else {
                Ok(None)
            }
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

        async fn delete_value(&self, store_name: &str, key: &str) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(store_name)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let js_key = JsValue::from_str(key);
            store
                .delete(&js_key)
                .map_err(|e| format!("Delete error: {:?}", e))?;
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }

        pub async fn save_order(&self, order: &ShopOrder) -> Result<(), String> {
            let key = order.order_id.clone();
            self.put_value(STORE_SHOP_ORDERS, &key, order).await
        }

        pub async fn get_order(&self, order_id: &str) -> Result<Option<ShopOrder>, String> {
            self.get_value(STORE_SHOP_ORDERS, order_id).await
        }

        pub async fn get_all_orders(&self) -> Result<Vec<ShopOrder>, String> {
            self.get_all_values(STORE_SHOP_ORDERS).await
        }

        pub async fn update_order(&self, order: &ShopOrder) -> Result<(), String> {
            let key = order.order_id.clone();
            self.put_value(STORE_SHOP_ORDERS, &key, order).await
        }

        pub async fn delete_order(&self, order_id: &str) -> Result<(), String> {
            self.delete_value(STORE_SHOP_ORDERS, order_id).await
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
pub use wasm_impl::ShopDatabase;
