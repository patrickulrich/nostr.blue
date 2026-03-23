use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::platform::storage;
use crate::services::ppq::PPQ_CHAT_BASE_URL;

const STORAGE_KEY: &str = "nostr_blue_ai_provider_state";
const CACHE_PROVIDER_STATE_ERROR_PREFIX: &str = "Failed to cache AI provider state: ";
const SHAKESPEARE_PROVIDER_ID: &str = "shakespeare";
const PPQ_PROVIDER_ID: &str = "ppq";
static PROVIDER_STATE_SAVE_EVENT_ID: AtomicU64 = AtomicU64::new(0);
pub static PROVIDER_STATE_SAVE_EVENT: GlobalSignal<Option<ProviderStateSaveEvent>> =
    Signal::global(|| None);

#[derive(Default)]
struct PendingProviderStateSave {
    in_flight: bool,
    latest: Option<AiProviderState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AiProviderKind {
    Ppq,
    OpenAiCompatible,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomAiProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub provider_kind: AiProviderKind,
}

impl fmt::Debug for CustomAiProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomAiProvider")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("provider_kind", &self.provider_kind)
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PpqAccountState {
    pub credit_id: String,
    pub api_key: String,
    #[serde(default)]
    pub managed_api_key: Option<String>,
    #[serde(default)]
    pub active_api_key_id: Option<String>,
}

impl fmt::Debug for PpqAccountState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PpqAccountState")
            .field("credit_id", &self.credit_id)
            .field("api_key", &"<redacted>")
            .field(
                "managed_api_key",
                &self.managed_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("active_api_key_id", &self.active_api_key_id)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiProviderState {
    pub selected_provider_id: String,
    #[serde(default)]
    pub selected_model_by_provider: HashMap<String, String>,
    #[serde(default)]
    pub custom_providers: Vec<CustomAiProvider>,
    #[serde(default)]
    pub ppq_account: Option<PpqAccountState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderStateSaveEvent {
    pub event_id: u64,
    pub snapshot: AiProviderState,
    pub result: Result<(), String>,
}

impl Default for AiProviderState {
    fn default() -> Self {
        Self {
            selected_provider_id: PPQ_PROVIDER_ID.to_string(),
            selected_model_by_provider: HashMap::new(),
            custom_providers: Vec::new(),
            ppq_account: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum ProviderAuth {
    PpqManaged { api_key: Option<String> },
    BearerToken(String),
}

impl fmt::Debug for ProviderAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PpqManaged { api_key } => f
                .debug_struct("PpqManaged")
                .field("api_key", &api_key.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::BearerToken(_) => f.debug_tuple("BearerToken").field(&"<redacted>").finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub provider_kind: AiProviderKind,
    pub auth: ProviderAuth,
    pub is_builtin: bool,
}

impl AiProviderConfig {
    pub fn requires_setup(&self) -> bool {
        matches!(&self.auth, ProviderAuth::PpqManaged { api_key } if api_key.as_deref().unwrap_or("").trim().is_empty())
    }

    pub fn supports_tools(&self) -> bool {
        !matches!(self.provider_kind, AiProviderKind::Ppq)
    }

    pub fn authentication_label(&self) -> &'static str {
        match self.auth {
            ProviderAuth::PpqManaged { .. } => "Managed API Key",
            ProviderAuth::BearerToken(_) => "API Key",
        }
    }
}

pub fn ppq_provider(account: Option<&PpqAccountState>) -> AiProviderConfig {
    AiProviderConfig {
        id: PPQ_PROVIDER_ID.to_string(),
        name: "PPQ".to_string(),
        base_url: PPQ_CHAT_BASE_URL.to_string(),
        provider_kind: AiProviderKind::Ppq,
        auth: ProviderAuth::PpqManaged {
            api_key: account.and_then(|account| {
                let selected_key = account
                    .managed_api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                    .unwrap_or(account.api_key.as_str());
                let trimmed = selected_key.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }),
        },
        is_builtin: true,
    }
}

pub fn resolve_providers(state: &AiProviderState) -> Vec<AiProviderConfig> {
    let mut providers = vec![ppq_provider(state.ppq_account.as_ref())];
    providers.extend(
        state
            .custom_providers
            .iter()
            .map(|provider| AiProviderConfig {
                id: provider.id.clone(),
                name: provider.name.clone(),
                base_url: provider.base_url.clone(),
                provider_kind: provider.provider_kind.clone(),
                auth: ProviderAuth::BearerToken(provider.api_key.clone()),
                is_builtin: false,
            }),
    );
    providers
}

pub fn sanitize_provider_input(input: &str) -> String {
    input.trim().to_string()
}

pub fn normalize_base_url(input: &str) -> String {
    input.trim().trim_end_matches('/').to_string()
}

fn migrate_legacy_state(mut state: AiProviderState) -> AiProviderState {
    if state.selected_provider_id == SHAKESPEARE_PROVIDER_ID {
        state.selected_provider_id = PPQ_PROVIDER_ID.to_string();
    }
    if let Some(model) = state
        .selected_model_by_provider
        .remove(SHAKESPEARE_PROVIDER_ID)
    {
        state
            .selected_model_by_provider
            .entry(PPQ_PROVIDER_ID.to_string())
            .or_insert(model);
    }
    state
}

fn pending_provider_state_save() -> &'static Mutex<PendingProviderStateSave> {
    static PENDING_SAVE: OnceLock<Mutex<PendingProviderStateSave>> = OnceLock::new();
    PENDING_SAVE.get_or_init(|| Mutex::new(PendingProviderStateSave::default()))
}

pub fn queue_provider_state_save(state: AiProviderState) -> Option<AiProviderState> {
    let mut pending = pending_provider_state_save()
        .lock()
        .expect("provider state save queue poisoned");
    pending.latest = Some(state);

    if pending.in_flight {
        None
    } else {
        pending.in_flight = true;
        pending.latest.take()
    }
}

pub fn finish_provider_state_save() -> Option<AiProviderState> {
    let mut pending = pending_provider_state_save()
        .lock()
        .expect("provider state save queue poisoned");

    if let Some(next_state) = pending.latest.take() {
        Some(next_state)
    } else {
        pending.in_flight = false;
        None
    }
}

fn emit_provider_state_save_event(snapshot: AiProviderState, result: Result<(), String>) {
    let event_id = PROVIDER_STATE_SAVE_EVENT_ID.fetch_add(1, Ordering::SeqCst) + 1;
    *PROVIDER_STATE_SAVE_EVENT.write() = Some(ProviderStateSaveEvent {
        event_id,
        snapshot,
        result,
    });
}

pub fn process_queued_provider_state_saves(initial_snapshot: AiProviderState) {
    spawn_forever(async move {
        let mut next_snapshot = Some(initial_snapshot);
        while let Some(current_snapshot) = next_snapshot {
            let result = save_provider_state(&current_snapshot)
                .await
                .and_then(|_| cache_provider_state(&current_snapshot));
            emit_provider_state_save_event(current_snapshot, result);
            next_snapshot = finish_provider_state_save();
        }
    });
}

pub fn is_cache_provider_state_error(error: &str) -> bool {
    error.starts_with(CACHE_PROVIDER_STATE_ERROR_PREFIX)
}

#[cfg(test)]
fn reset_provider_state_save_queue() {
    let mut pending = pending_provider_state_save()
        .lock()
        .expect("provider state save queue poisoned");
    pending.in_flight = false;
    pending.latest = None;
}

#[cfg(test)]
fn provider_state_save_test_lock() -> &'static Mutex<()> {
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    TEST_LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
mod web_db {
    use super::{migrate_legacy_state, AiProviderState};
    use crate::stores::ui::ai_web_db::{open_ai_db_with_schema, STORE_SETTINGS};
    use indexed_db_futures::prelude::*;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;
    use web_sys::IdbTransactionMode;

    const STATE_KEY: &str = "state";

    #[derive(Clone, Debug)]
    pub struct AiProviderDb {
        db: Rc<IdbDatabase>,
    }

    unsafe impl Send for AiProviderDb {}
    unsafe impl Sync for AiProviderDb {}

    impl AiProviderDb {
        pub async fn new() -> Result<Self, String> {
            let db = open_ai_db_with_schema("AI provider")
                .await
                .map_err(|e| format!("Failed to open AI provider database: {}", e))?;
            Ok(Self { db: Rc::new(db) })
        }

        pub async fn load_state(&self) -> Result<Option<AiProviderState>, String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_SETTINGS, IdbTransactionMode::Readonly)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_SETTINGS)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let value = store
                .get(&JsValue::from_str(STATE_KEY))
                .map_err(|e| format!("Get error: {:?}", e))?
                .await
                .map_err(|e| format!("Get await error: {:?}", e))?;
            let Some(value) = value else {
                return Ok(None);
            };
            let json = value
                .as_string()
                .ok_or_else(|| "Stored AI provider state was not a string".to_string())?;
            let state: AiProviderState = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse AI provider state: {}", e))?;
            Ok(Some(migrate_legacy_state(state)))
        }

        pub async fn save_state(&self, state: &AiProviderState) -> Result<(), String> {
            let tx = self
                .db
                .transaction_on_one_with_mode(STORE_SETTINGS, IdbTransactionMode::Readwrite)
                .map_err(|e| format!("Transaction error: {:?}", e))?;
            let store = tx
                .object_store(STORE_SETTINGS)
                .map_err(|e| format!("Store error: {:?}", e))?;
            let json = serde_json::to_string(state)
                .map_err(|e| format!("Failed to serialize AI provider state: {}", e))?;
            store
                .put_key_val(&JsValue::from_str(STATE_KEY), &JsValue::from_str(&json))
                .map_err(|e| format!("Put error: {:?}", e))?;
            tx.await
                .into_result()
                .map_err(|e| format!("Transaction commit error: {:?}", e))?;
            Ok(())
        }
    }
}

pub async fn load_provider_state() -> Result<AiProviderState, String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        match web_db::AiProviderDb::new().await {
            Ok(db) => match db.load_state().await {
                Ok(Some(state)) => return Ok(state),
                Ok(None) => {}
                Err(db_error) => {
                    log::warn!(
                        "Failed to load AI provider state from IndexedDB, falling back to local storage: {}",
                        db_error
                    );
                }
            },
            Err(db_error) => {
                log::warn!(
                    "Failed to open AI provider database, falling back to local storage: {}",
                    db_error
                );
            }
        }
        match storage::get(STORAGE_KEY) {
            Ok(state) => Ok(migrate_legacy_state(state)),
            Err(storage_error) => {
                let error_text = storage_error.to_string();
                if error_text.contains("missing")
                    || error_text.contains("not found")
                    || error_text.contains("does not exist")
                {
                    Ok(AiProviderState::default())
                } else {
                    Err(format!(
                        "Failed to load AI provider state from fallback local storage: {}",
                        storage_error
                    ))
                }
            }
        }
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        Ok(migrate_legacy_state(
            storage::get(STORAGE_KEY).unwrap_or_default(),
        ))
    }
}

pub async fn save_provider_state(state: &AiProviderState) -> Result<(), String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        return web_db::AiProviderDb::new().await?.save_state(state).await;
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        let _ = state;
        Ok(())
    }
}

pub fn cache_provider_state(state: &AiProviderState) -> Result<(), String> {
    storage::set(STORAGE_KEY, state)
        .map_err(|error| format!("{CACHE_PROVIDER_STATE_ERROR_PREFIX}{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_base_url() {
        assert_eq!(
            normalize_base_url(" https://api.example.com/v1/ "),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn resolves_builtin_and_custom_providers() {
        let state = AiProviderState {
            selected_provider_id: "custom".to_string(),
            selected_model_by_provider: HashMap::new(),
            custom_providers: vec![CustomAiProvider {
                id: "custom".to_string(),
                name: "Custom".to_string(),
                base_url: "https://example.com/v1".to_string(),
                api_key: "secret".to_string(),
                provider_kind: AiProviderKind::OpenAiCompatible,
            }],
            ppq_account: Some(PpqAccountState {
                credit_id: "credit-123".to_string(),
                api_key: "sk-managed".to_string(),
                managed_api_key: None,
                active_api_key_id: Some("key-1".to_string()),
            }),
        };

        let providers = resolve_providers(&state);
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].id, PPQ_PROVIDER_ID);
        assert_eq!(providers[1].id, "custom");
        assert!(matches!(
            providers[0].auth,
            ProviderAuth::PpqManaged { api_key: Some(_) }
        ));
        assert!(matches!(providers[1].auth, ProviderAuth::BearerToken(_)));
    }

    #[test]
    fn ppq_provider_requires_setup_without_key() {
        let provider = ppq_provider(None);
        assert!(provider.requires_setup());
        assert!(!provider.supports_tools());
    }

    #[test]
    fn custom_provider_supports_tools() {
        let provider = AiProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            provider_kind: AiProviderKind::OpenAiCompatible,
            auth: ProviderAuth::BearerToken("secret".to_string()),
            is_builtin: false,
        };
        assert!(provider.supports_tools());
    }

    #[test]
    fn debug_output_redacts_provider_secrets() {
        let provider = CustomAiProvider {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "secret-api-key".to_string(),
            provider_kind: AiProviderKind::OpenAiCompatible,
        };
        let account = PpqAccountState {
            credit_id: "credit-123".to_string(),
            api_key: "secret-account-key".to_string(),
            managed_api_key: Some("secret-managed-key".to_string()),
            active_api_key_id: Some("key-123".to_string()),
        };
        let provider_debug = format!("{provider:?}");
        let account_debug = format!("{account:?}");
        let auth_debug = format!(
            "{:?}",
            ProviderAuth::BearerToken("secret-bearer-key".to_string())
        );

        assert!(!provider_debug.contains("secret-api-key"));
        assert!(!account_debug.contains("secret-account-key"));
        assert!(!account_debug.contains("secret-managed-key"));
        assert!(account_debug.contains("key-123"));
        assert!(!auth_debug.contains("secret-bearer-key"));
    }

    #[test]
    fn migrates_shakespeare_selection_to_ppq() {
        let migrated = migrate_legacy_state(AiProviderState {
            selected_provider_id: "shakespeare".to_string(),
            selected_model_by_provider: HashMap::new(),
            custom_providers: vec![],
            ppq_account: None,
        });
        assert_eq!(migrated.selected_provider_id, PPQ_PROVIDER_ID);
    }

    #[test]
    fn migrates_shakespeare_model_selection_to_ppq() {
        let mut selected_model_by_provider = HashMap::new();
        selected_model_by_provider.insert("shakespeare".to_string(), "model-a".to_string());

        let migrated = migrate_legacy_state(AiProviderState {
            selected_provider_id: "shakespeare".to_string(),
            selected_model_by_provider,
            custom_providers: vec![],
            ppq_account: None,
        });

        assert_eq!(
            migrated.selected_model_by_provider.get(PPQ_PROVIDER_ID),
            Some(&"model-a".to_string())
        );
        assert!(!migrated
            .selected_model_by_provider
            .contains_key(SHAKESPEARE_PROVIDER_ID));
    }

    #[test]
    fn deserializes_legacy_state_without_model_map() {
        let json = r#"{"selected_provider_id":"shakespeare","custom_providers":[]}"#;
        let state: AiProviderState = serde_json::from_str(json).unwrap();
        assert!(state.selected_model_by_provider.is_empty());
        assert_eq!(
            migrate_legacy_state(state).selected_provider_id,
            PPQ_PROVIDER_ID
        );
    }

    #[test]
    fn queues_latest_provider_state_save_snapshot() {
        let _test_lock = provider_state_save_test_lock()
            .lock()
            .expect("provider state save test lock poisoned");
        reset_provider_state_save_queue();

        let mut first = AiProviderState::default();
        first
            .selected_model_by_provider
            .insert("ppq".to_string(), "model-a".to_string());

        let mut second = first.clone();
        second
            .selected_model_by_provider
            .insert("ppq".to_string(), "model-b".to_string());

        let queued_first = queue_provider_state_save(first.clone());
        assert_eq!(queued_first, Some(first));
        assert_eq!(queue_provider_state_save(second.clone()), None);
        assert_eq!(finish_provider_state_save(), Some(second));
        assert_eq!(finish_provider_state_save(), None);

        reset_provider_state_save_queue();
    }

    #[test]
    fn queued_provider_state_save_keeps_latest_custom_provider_snapshot() {
        let _test_lock = provider_state_save_test_lock()
            .lock()
            .expect("provider state save test lock poisoned");
        reset_provider_state_save_queue();

        let first = AiProviderState {
            selected_provider_id: "custom-a".to_string(),
            selected_model_by_provider: HashMap::new(),
            custom_providers: vec![CustomAiProvider {
                id: "custom-a".to_string(),
                name: "Custom A".to_string(),
                base_url: "https://example.com/v1".to_string(),
                api_key: "secret-a".to_string(),
                provider_kind: AiProviderKind::OpenAiCompatible,
            }],
            ppq_account: None,
        };
        let second = AiProviderState {
            selected_provider_id: "custom-b".to_string(),
            selected_model_by_provider: HashMap::new(),
            custom_providers: vec![CustomAiProvider {
                id: "custom-b".to_string(),
                name: "Custom B".to_string(),
                base_url: "https://example.net/v1".to_string(),
                api_key: "secret-b".to_string(),
                provider_kind: AiProviderKind::OpenAiCompatible,
            }],
            ppq_account: None,
        };

        assert_eq!(queue_provider_state_save(first.clone()), Some(first));
        assert_eq!(queue_provider_state_save(second.clone()), None);
        assert_eq!(finish_provider_state_save(), Some(second));
        assert_eq!(finish_provider_state_save(), None);

        reset_provider_state_save_queue();
    }
}
