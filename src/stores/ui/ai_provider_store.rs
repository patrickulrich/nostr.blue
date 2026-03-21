use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::platform::storage;
use crate::services::ppq::PPQ_CHAT_BASE_URL;

const STORAGE_KEY: &str = "nostr_blue_ai_provider_state";
const SHAKESPEARE_PROVIDER_ID: &str = "shakespeare";
const PPQ_PROVIDER_ID: &str = "ppq";

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomAiProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub provider_kind: AiProviderKind,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PpqAccountState {
    pub credit_id: String,
    pub api_key: String,
    #[serde(default)]
    pub active_api_key_id: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderAuth {
    PpqManaged { api_key: Option<String> },
    BearerToken(String),
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
        false
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
                let trimmed = account.api_key.trim();
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

#[cfg(test)]
fn reset_provider_state_save_queue() {
    let mut pending = pending_provider_state_save()
        .lock()
        .expect("provider state save queue poisoned");
    pending.in_flight = false;
    pending.latest = None;
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

        pub async fn load_state(&self) -> Result<AiProviderState, String> {
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
                return Ok(AiProviderState::default());
            };
            let json = value
                .as_string()
                .ok_or_else(|| "Stored AI provider state was not a string".to_string())?;
            let state: AiProviderState = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse AI provider state: {}", e))?;
            Ok(migrate_legacy_state(state))
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
        if let Ok(cached_state) = storage::get(STORAGE_KEY) {
            return Ok(migrate_legacy_state(cached_state));
        }
        return web_db::AiProviderDb::new().await?.load_state().await;
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
}
