use crate::stores::auth_store;
use dioxus::prelude::ReadableExt;
use serde::{Deserialize, Serialize};

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
use crate::platform::storage;

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const DB_NAME: &str = "nostr_blue_ai_providers";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const DB_VERSION: u32 = 2;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_SETTINGS: &str = "settings";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_CHAT_HISTORY: &str = "chat_history";
#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
const STORAGE_KEY_PREFIX: &str = "nostr_blue_ai_chat_history";
const ANONYMOUS_ACCOUNT_KEY: &str = "anonymous";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PersistedChatRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedToolCall {
    pub id: String,
    pub name: String,
    pub result: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedChatMessage {
    pub id: String,
    pub role: PersistedChatRole,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
}

pub fn account_key_for_pubkey(pubkey: Option<&str>) -> String {
    pubkey
        .and_then(|value| crate::utils::nip19::normalize_pubkey(value).ok())
        .unwrap_or_else(|| ANONYMOUS_ACCOUNT_KEY.to_string())
}

pub fn current_account_key() -> String {
    let current_pubkey = auth_store::AUTH_STATE.read().pubkey.clone();
    account_key_for_pubkey(current_pubkey.as_deref())
}

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
fn storage_key(account_key: &str) -> String {
    format!("{}_{}", STORAGE_KEY_PREFIX, account_key)
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
mod web_db {
    use super::{PersistedChatMessage, DB_NAME, DB_VERSION, STORE_CHAT_HISTORY, STORE_SETTINGS};
    use indexed_db_futures::prelude::*;
    use std::future::IntoFuture;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;
    use web_sys::IdbTransactionMode;

    #[derive(Clone, Debug)]
    pub struct AiChatDb {
        db: Rc<IdbDatabase>,
    }

    unsafe impl Send for AiChatDb {}
    unsafe impl Sync for AiChatDb {}

    impl AiChatDb {
        pub async fn new() -> Result<Self, String> {
            let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
                .map_err(|e| format!("Failed to open AI chat database: {:?}", e))?;
            db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
                let db = evt.db();
                if !db.object_store_names().any(|n| n == STORE_SETTINGS) {
                    db.create_object_store(STORE_SETTINGS)?;
                }
                if !db.object_store_names().any(|n| n == STORE_CHAT_HISTORY) {
                    db.create_object_store(STORE_CHAT_HISTORY)?;
                }
                Ok(())
            }));
            let db: IdbDatabase = db_req
                .into_future()
                .await
                .map_err(|e| format!("Failed to open AI chat database: {:?}", e))?;
            Ok(Self { db: Rc::new(db) })
        }

        pub async fn load_chat_history(
            &self,
            account_key: &str,
        ) -> Result<Vec<PersistedChatMessage>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_CHAT_HISTORY, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_CHAT_HISTORY)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let value = store
                .get(&JsValue::from_str(account_key))
                .map_err(|e| format!("Get error: {:?}", e))?
                .await
                .map_err(|e| format!("Get await error: {:?}", e))?;
            let Some(value) = value else {
                return Ok(Vec::new());
            };
            let json = value
                .as_string()
                .ok_or_else(|| "Stored AI chat history was not a string".to_string())?;
            serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse AI chat history: {}", e))
        }

        pub async fn save_chat_history(
            &self,
            account_key: &str,
            messages: &[PersistedChatMessage],
        ) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_CHAT_HISTORY, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_CHAT_HISTORY)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let json = serde_json::to_string(messages)
                .map_err(|e| format!("Failed to serialize AI chat history: {}", e))?;
            store
                .put_key_val(&JsValue::from_str(account_key), &JsValue::from_str(&json))
                .map_err(|e| format!("Put error: {:?}", e))?;
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }

        pub async fn clear_chat_history(&self, account_key: &str) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_CHAT_HISTORY, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_CHAT_HISTORY)
                .map_err(|e| format!("Store error: {:?}", e))?;
            store
                .delete(&JsValue::from_str(account_key))
                .map_err(|e| format!("Delete error: {:?}", e))?;
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }
    }
}

pub async fn load_chat_history(account_key: &str) -> Result<Vec<PersistedChatMessage>, String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        return web_db::AiChatDb::new()
            .await?
            .load_chat_history(account_key)
            .await;
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        Ok(storage::get(&storage_key(account_key)).unwrap_or_default())
    }
}

pub async fn save_chat_history(
    account_key: &str,
    messages: &[PersistedChatMessage],
) -> Result<(), String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        return web_db::AiChatDb::new()
            .await?
            .save_chat_history(account_key, messages)
            .await;
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        storage::set(&storage_key(account_key), messages)
    }
}

pub async fn clear_chat_history(account_key: &str) -> Result<(), String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        return web_db::AiChatDb::new()
            .await?
            .clear_chat_history(account_key)
            .await;
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        storage::delete(&storage_key(account_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_account_key_to_hex() {
        let key = account_key_for_pubkey(Some(
            "npub1xtscya34g58tk0z605fvr788k263gsu6cy9x0mhnm87echrgufzsevkk5s",
        ));
        assert_eq!(
            key,
            "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245"
        );
    }
}
