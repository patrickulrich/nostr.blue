use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::result::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::platform::storage;
use crate::services::ppq::PPQ_CHAT_BASE_URL;
use crate::stores::nostr_client;
use crate::stores::relay::{wait_for_user_relays, USER_RELAYS_APPLIED};
use crate::stores::ui::sidebar_store::Nip78LoadState;

const STORAGE_KEY: &str = "nostr_blue_ai_provider_state";
const SHAKESPEARE_PROVIDER_ID: &str = "shakespeare";
const PPQ_PROVIDER_ID: &str = "ppq";
const APP_DATA_KIND: u16 = 30078;
const CREDENTIALS_D_TAG: &str = "nostr.blue/ai_credentials";
static PROVIDER_STATE_SAVE_EVENT_ID: AtomicU64 = AtomicU64::new(0);
pub static PROVIDER_STATE_SAVE_EVENT: GlobalSignal<Option<ProviderStateSaveEvent>> =
    Signal::global(|| None);
/// NIP-78 load-state machine for retry gating (mirrors sidebar/reactions pattern).
pub static AI_PROVIDER_STATE: GlobalSignal<Nip78LoadState> =
    Signal::global(Nip78LoadState::default);

#[derive(Default)]
struct PendingProviderStateSave {
    in_flight: bool,
    latest: Option<AiProviderState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AiProviderKind {
    Ppq,
    OpenAiCompatible,
    Anthropic,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomAiProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub provider_kind: AiProviderKind,
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PpqAccountState {
    pub credit_id: String,
    pub api_key: String,
    #[serde(default)]
    pub managed_api_key: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderAuth {
    PpqManaged { api_key: Option<String> },
    BearerToken(String),
    XApiKey(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub provider_kind: AiProviderKind,
    pub auth: ProviderAuth,
    pub is_builtin: bool,
    pub default_model: Option<String>,
}

impl AiProviderConfig {
    pub fn requires_setup(&self) -> bool {
        matches!(&self.auth, ProviderAuth::PpqManaged { api_key } if api_key.as_deref().unwrap_or("").trim().is_empty())
    }

    #[allow(dead_code)]
    pub fn supports_tools(&self) -> bool {
        true
    }

    pub fn authentication_label(&self) -> &'static str {
        match self.auth {
            ProviderAuth::PpqManaged { .. } => "Managed API Key",
            ProviderAuth::BearerToken(_) => "API Key",
            ProviderAuth::XApiKey(_) => "API Key",
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
        default_model: None,
    }
}

pub fn resolve_providers(state: &AiProviderState) -> Vec<AiProviderConfig> {
    let mut providers = vec![ppq_provider(state.ppq_account.as_ref())];
    providers.extend(
        state
            .custom_providers
            .iter()
            .map(|provider| {
                let auth = match provider.provider_kind {
                    AiProviderKind::Anthropic => ProviderAuth::XApiKey(provider.api_key.clone()),
                    AiProviderKind::Ppq | AiProviderKind::OpenAiCompatible => {
                        ProviderAuth::BearerToken(provider.api_key.clone())
                    }
                };
                AiProviderConfig {
                    id: provider.id.clone(),
                    name: provider.name.clone(),
                    base_url: provider.base_url.clone(),
                    provider_kind: provider.provider_kind.clone(),
                    auth,
                    is_builtin: false,
                    default_model: provider.default_model.clone(),
                }
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

static PENDING_RELAY_STATE: OnceLock<Mutex<Option<AiProviderState>>> = OnceLock::new();

fn pending_relay_state() -> &'static Mutex<Option<AiProviderState>> {
    PENDING_RELAY_STATE.get_or_init(|| Mutex::new(None))
}

pub fn clear_relay_state() {
    *pending_relay_state().lock().expect("relay state lock poisoned") = None;
    *AI_PROVIDER_STATE.write() = Nip78LoadState::default();
}

pub async fn sync_provider_state_from_relays() {
    if !crate::stores::auth_store::is_authenticated() {
        return;
    }
    // Guard against duplicate concurrent loads. Allow retry on Failed.
    {
        let state = AI_PROVIDER_STATE.read().clone();
        if state.is_loading() {
            return;
        }
        if matches!(state, Nip78LoadState::Loaded | Nip78LoadState::LoadedDefaults) {
            return;
        }
        *AI_PROVIDER_STATE.write() = Nip78LoadState::Loading;
    }
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            *AI_PROVIDER_STATE.write() = Nip78LoadState::Failed("Client not ready".into());
            return;
        }
    };
    let pubkey = match nostr_client::get_cached_pubkey() {
        Ok(pk) => pk,
        Err(_) => {
            *AI_PROVIDER_STATE.write() = Nip78LoadState::LoadedDefaults;
            return;
        }
    };
    let signer = match client.signer().await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("sync_provider_state: no signer: {}", e);
            *AI_PROVIDER_STATE.write() = Nip78LoadState::Failed(format!("No signer: {e}"));
            return;
        }
    };
    // Gate: ensure the user's NIP-65 outbox relays are in the pool before
    // fetching, so we query the right relays (not the bootstrap set).
    wait_for_user_relays(
        std::time::Duration::from_secs(5),
        "ai_provider_store::sync_provider_state_from_relays",
    )
    .await;
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(CREDENTIALS_D_TAG)
        .limit(1);
    let events = match nostr_client::fetch_events_from_connected_relays_with_client(
        &client,
        filter,
        std::time::Duration::from_secs(10),
    )
    .await
    {
        Ok(e) => e,
        Err(e) => {
            log::warn!("sync_provider_state: fetch failed: {}", e);
            *AI_PROVIDER_STATE.write() = Nip78LoadState::Failed(e);
            return;
        }
    };
    let event = match events.into_iter().next() {
        Some(e) => e,
        None => {
            log::debug!("sync_provider_state: no encrypted event found on relays");
            // Distinguish "user relays not applied" (Failed → retry) from
            // "genuinely no credentials" (LoadedDefaults).
            *AI_PROVIDER_STATE.write() = if !*USER_RELAYS_APPLIED.peek() {
                Nip78LoadState::Failed("User relays not applied, retry needed".into())
            } else {
                Nip78LoadState::LoadedDefaults
            };
            return;
        }
    };
    if event.content.is_empty() {
        *AI_PROVIDER_STATE.write() = Nip78LoadState::LoadedDefaults;
        return;
    }
    let decrypted = match signer.nip44_decrypt(&event.pubkey, &event.content).await {
        Ok(d) => d,
        Err(e) => {
            log::warn!("sync_provider_state: decrypt failed: {}", e);
            *AI_PROVIDER_STATE.write() = Nip78LoadState::Failed(format!("Decrypt: {e}"));
            return;
        }
    };
    match serde_json::from_str::<AiProviderState>(&decrypted) {
        Ok(relay_state) => {
            let migrated = migrate_legacy_state(relay_state);
            log::info!(
                "sync_provider_state: loaded state from relays (provider={}, {} custom providers)",
                migrated.selected_provider_id,
                migrated.custom_providers.len()
            );
            if let Err(e) = cache_provider_state(&migrated) {
                log::warn!("sync_provider_state: failed to cache: {}", e);
            }
            *pending_relay_state().lock().expect("relay state lock poisoned") =
                Some(migrated);
            *AI_PROVIDER_STATE.write() = Nip78LoadState::Loaded;
        }
        Err(e) => {
            log::warn!("sync_provider_state: parse failed: {}", e);
            *AI_PROVIDER_STATE.write() = Nip78LoadState::Failed(format!("Parse: {e}"));
        }
    }
}

async fn save_encrypted_provider_state(state: &AiProviderState) -> Result<(), String> {
    if !crate::stores::auth_store::is_authenticated() {
        return Ok(());
    }
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("No signer: {}", e))?;
    let pubkey = nostr_client::get_cached_pubkey()?;
    let json = serde_json::to_string(state).map_err(|e| format!("Serialize: {}", e))?;
    let encrypted = signer
        .nip44_encrypt(&pubkey, &json)
        .await
        .map_err(|e| format!("Encrypt: {}", e))?;
    let builder = EventBuilder::new(Kind::from(APP_DATA_KIND), encrypted)
        .tag(Tag::identifier(CREDENTIALS_D_TAG));
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder).await?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other(
            "ai_credentials".to_string(),
        ),
        None,
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("save_encrypted_provider_state: published to relay queue");
    Ok(())
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
            let result = save_provider_state(&current_snapshot).await;
            let encrypted_result = save_encrypted_provider_state(&current_snapshot).await;
            if let Err(e) = encrypted_result {
                log::warn!("Failed to save encrypted provider state: {}", e);
            }
            emit_provider_state_save_event(current_snapshot, result);
            next_snapshot = finish_provider_state_save();
        }
    });
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
    let mut state = {
        #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
        {
            if let Ok(cached_state) = storage::get(STORAGE_KEY) {
                migrate_legacy_state(cached_state)
            } else {
                web_db::AiProviderDb::new().await?.load_state().await?
            }
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
        {
            migrate_legacy_state(storage::get(STORAGE_KEY).unwrap_or_default())
        }
    };

    if let Some(relay_state) = pending_relay_state()
        .lock()
        .expect("relay state lock poisoned")
        .take()
    {
        log::info!("load_provider_state: merging relay state into local cache");
        state = relay_state;
        let _ = cache_provider_state(&state);
    }

    Ok(state)
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
                default_model: None,
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
    fn resolves_anthropic_provider_with_x_api_key_auth() {
        let state = AiProviderState {
            selected_provider_id: "anthropic".to_string(),
            selected_model_by_provider: HashMap::new(),
            custom_providers: vec![CustomAiProvider {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                base_url: "https://api.anthropic.com/v1".to_string(),
                api_key: "sk-ant-123".to_string(),
                provider_kind: AiProviderKind::Anthropic,
                default_model: None,
            }],
            ppq_account: None,
        };

        let providers = resolve_providers(&state);
        assert_eq!(providers.len(), 2);
        assert!(matches!(providers[1].auth, ProviderAuth::XApiKey(_)));
        if let ProviderAuth::XApiKey(key) = &providers[1].auth {
            assert_eq!(key, "sk-ant-123");
        }
    }

    #[test]
    fn ppq_provider_requires_setup_without_key() {
        let provider = ppq_provider(None);
        assert!(provider.requires_setup());
        assert!(provider.supports_tools());
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
            default_model: None,
        };
        assert!(provider.supports_tools());
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
                default_model: None,
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
                default_model: None,
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
