use crate::stores::auth_store;
use dioxus::core::spawn_forever;
use dioxus::prelude::ReadableExt;
use dioxus::prelude::{GlobalSignal, Signal};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
use crate::platform::storage;

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
const STORAGE_KEY_PREFIX: &str = "nostr_blue_ai_chat_history";
const ANONYMOUS_ACCOUNT_KEY: &str = "anonymous";
static CHAT_HISTORY_SAVE_EVENT_ID: AtomicU64 = AtomicU64::new(0);
pub static CHAT_HISTORY_SAVE_EVENT: GlobalSignal<Option<ChatHistorySaveEvent>> =
    Signal::global(|| None);

#[derive(Default)]
struct PendingChatHistorySaveQueue {
    entries: HashMap<String, PendingChatHistorySave>,
}

#[derive(Default)]
struct PendingChatHistorySave {
    in_flight: bool,
    latest: Option<Vec<PersistedChatMessage>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PersistedChatRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PersistedToolCall {
    pub id: String,
    pub name: String,
    pub result: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PersistedChatImage {
    pub url: String,
    #[serde(default)]
    pub alt: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PersistedChatMessage {
    pub id: String,
    pub role: PersistedChatRole,
    pub content: String,
    #[serde(default)]
    pub images: Vec<PersistedChatImage>,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatHistorySaveEvent {
    pub event_id: u64,
    pub account_key: String,
    pub snapshot: Vec<PersistedChatMessage>,
    pub result: Result<(), String>,
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

fn pending_chat_history_save_queue() -> &'static Mutex<PendingChatHistorySaveQueue> {
    static PENDING_SAVE: OnceLock<Mutex<PendingChatHistorySaveQueue>> = OnceLock::new();
    PENDING_SAVE.get_or_init(|| Mutex::new(PendingChatHistorySaveQueue::default()))
}

pub fn queue_chat_history_save(
    account_key: String,
    snapshot: Vec<PersistedChatMessage>,
) -> Option<(String, Vec<PersistedChatMessage>)> {
    let mut pending = pending_chat_history_save_queue()
        .lock()
        .expect("chat history save queue poisoned");
    let entry = pending.entries.entry(account_key.clone()).or_default();
    entry.latest = Some(snapshot);

    if entry.in_flight {
        None
    } else {
        entry.in_flight = true;
        entry.latest.take().map(|snapshot| (account_key, snapshot))
    }
}

fn finish_chat_history_save(account_key: &str) -> Option<(String, Vec<PersistedChatMessage>)> {
    let mut pending = pending_chat_history_save_queue()
        .lock()
        .expect("chat history save queue poisoned");

    let mut remove_entry = false;
    let next = if let Some(entry) = pending.entries.get_mut(account_key) {
        if let Some(snapshot) = entry.latest.take() {
            Some((account_key.to_string(), snapshot))
        } else {
            entry.in_flight = false;
            remove_entry = true;
            None
        }
    } else {
        None
    };

    if remove_entry {
        pending.entries.remove(account_key);
    }

    next
}

fn emit_chat_history_save_event(
    account_key: String,
    snapshot: Vec<PersistedChatMessage>,
    result: Result<(), String>,
) {
    let event_id = CHAT_HISTORY_SAVE_EVENT_ID.fetch_add(1, Ordering::SeqCst) + 1;
    *CHAT_HISTORY_SAVE_EVENT.write() = Some(ChatHistorySaveEvent {
        event_id,
        account_key,
        snapshot,
        result,
    });
}

pub fn process_queued_chat_history_saves(
    initial_account_key: String,
    initial_snapshot: Vec<PersistedChatMessage>,
) {
    spawn_forever(async move {
        let mut next_save = Some((initial_account_key, initial_snapshot));
        while let Some((account_key, snapshot)) = next_save {
            let result = if snapshot.is_empty() {
                clear_chat_history(&account_key).await
            } else {
                save_chat_history(&account_key, &snapshot).await
            };
            emit_chat_history_save_event(account_key.clone(), snapshot, result);
            next_save = finish_chat_history_save(&account_key);
        }
    });
}

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
fn storage_key(account_key: &str) -> String {
    format!("{}_{}", STORAGE_KEY_PREFIX, account_key)
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
mod web_db {
    use super::PersistedChatMessage;
    use crate::stores::ui::ai_web_db::{open_ai_db_with_schema, STORE_CHAT_HISTORY};
    use indexed_db_futures::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;
    use web_sys::IdbTransactionMode;

    #[derive(Clone, Debug)]
    pub struct AiChatDb {
        db: Rc<IdbDatabase>,
    }

    thread_local! {
        static AI_CHAT_DB: RefCell<Option<AiChatDb>> = const { RefCell::new(None) };
    }

    impl AiChatDb {
        pub async fn new() -> Result<Self, String> {
            let db = open_ai_db_with_schema("AI chat")
                .await
                .map_err(|e| format!("Failed to open AI chat database: {}", e))?;
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

    pub async fn get_cached_db() -> Result<AiChatDb, String> {
        if let Some(db) = AI_CHAT_DB.with(|cached| cached.borrow().clone()) {
            return Ok(db);
        }

        let db = AiChatDb::new().await?;
        AI_CHAT_DB.with(|cached| {
            *cached.borrow_mut() = Some(db.clone());
        });
        Ok(db)
    }
}

pub async fn load_chat_history(account_key: &str) -> Result<Vec<PersistedChatMessage>, String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        return web_db::get_cached_db()
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
        return web_db::get_cached_db()
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
        return web_db::get_cached_db()
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

    #[test]
    fn falls_back_to_anonymous_for_missing_pubkey() {
        assert_eq!(account_key_for_pubkey(None), "anonymous");
    }

    #[test]
    fn falls_back_to_anonymous_for_invalid_pubkey() {
        assert_eq!(account_key_for_pubkey(Some("invalid")), "anonymous");
    }
}
