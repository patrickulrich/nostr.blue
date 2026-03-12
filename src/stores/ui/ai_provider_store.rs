use serde::{Deserialize, Serialize};

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
use crate::platform::storage;

#[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
const STORAGE_KEY: &str = "nostr_blue_ai_provider_state";
const SHAKESPEARE_PROVIDER_ID: &str = "shakespeare";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AiProviderKind {
    Shakespeare,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiProviderState {
    pub selected_provider_id: String,
    #[serde(default)]
    pub custom_providers: Vec<CustomAiProvider>,
}

impl Default for AiProviderState {
    fn default() -> Self {
        Self {
            selected_provider_id: SHAKESPEARE_PROVIDER_ID.to_string(),
            custom_providers: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderAuth {
    Nip98,
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
    pub fn requires_signer(&self) -> bool {
        matches!(self.auth, ProviderAuth::Nip98)
    }

    pub fn supports_tools(&self) -> bool {
        matches!(self.provider_kind, AiProviderKind::Shakespeare)
    }

    pub fn authentication_label(&self) -> &'static str {
        match self.auth {
            ProviderAuth::Nip98 => "NIP-98",
            ProviderAuth::BearerToken(_) => "API Key",
        }
    }
}

pub fn shakespeare_provider() -> AiProviderConfig {
    AiProviderConfig {
        id: SHAKESPEARE_PROVIDER_ID.to_string(),
        name: "Shakespeare".to_string(),
        base_url: "https://ai.shakespeare.diy/v1".to_string(),
        provider_kind: AiProviderKind::Shakespeare,
        auth: ProviderAuth::Nip98,
        is_builtin: true,
    }
}

pub fn resolve_providers(state: &AiProviderState) -> Vec<AiProviderConfig> {
    let mut providers = vec![shakespeare_provider()];
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

#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
mod web_db {
    use super::AiProviderState;
    use indexed_db_futures::prelude::*;
    use std::future::IntoFuture;
    use std::rc::Rc;
    use wasm_bindgen::JsValue;
    use web_sys::IdbTransactionMode;

    const DB_NAME: &str = "nostr_blue_ai_providers";
    const DB_VERSION: u32 = 1;
    const STORE_SETTINGS: &str = "settings";
    const STATE_KEY: &str = "state";

    #[derive(Clone, Debug)]
    pub struct AiProviderDb {
        db: Rc<IdbDatabase>,
    }

    unsafe impl Send for AiProviderDb {}
    unsafe impl Sync for AiProviderDb {}

    impl AiProviderDb {
        pub async fn new() -> Result<Self, String> {
            let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
                .map_err(|e| format!("Failed to open AI provider database: {:?}", e))?;
            db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
                let db = evt.db();
                if !db.object_store_names().any(|n| n == STORE_SETTINGS) {
                    db.create_object_store(STORE_SETTINGS)?;
                }
                Ok(())
            }));
            let db: IdbDatabase = db_req
                .into_future()
                .await
                .map_err(|e| format!("Failed to open AI provider database: {:?}", e))?;
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
            serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse AI provider state: {}", e))
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
        return web_db::AiProviderDb::new().await?.load_state().await;
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        Ok(storage::get(STORAGE_KEY).unwrap_or_default())
    }
}

pub async fn save_provider_state(state: &AiProviderState) -> Result<(), String> {
    #[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
    {
        return web_db::AiProviderDb::new().await?.save_state(state).await;
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "web", not(feature = "native"))))]
    {
        storage::set(STORAGE_KEY, state)
    }
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
            custom_providers: vec![CustomAiProvider {
                id: "custom".to_string(),
                name: "Custom".to_string(),
                base_url: "https://example.com/v1".to_string(),
                api_key: "secret".to_string(),
                provider_kind: AiProviderKind::OpenAiCompatible,
            }],
        };

        let providers = resolve_providers(&state);
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].id, SHAKESPEARE_PROVIDER_ID);
        assert_eq!(providers[1].id, "custom");
        assert!(matches!(providers[1].auth, ProviderAuth::BearerToken(_)));
    }

    #[test]
    fn only_shakespeare_supports_tools() {
        let providers = resolve_providers(&AiProviderState {
            selected_provider_id: "custom".to_string(),
            custom_providers: vec![CustomAiProvider {
                id: "custom".to_string(),
                name: "Custom".to_string(),
                base_url: "https://example.com/v1".to_string(),
                api_key: "secret".to_string(),
                provider_kind: AiProviderKind::OpenAiCompatible,
            }],
        });

        assert!(providers[0].supports_tools());
        assert!(!providers[1].supports_tools());
    }
}
