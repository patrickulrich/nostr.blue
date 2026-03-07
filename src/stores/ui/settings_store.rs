/// NIP-78: Application Data Storage
/// Stores user settings on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use crate::platform::storage;
use nostr_sdk::{EventBuilder, Filter, FromBech32, Kind, Tag};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::stores::blossom_store::BlossomServersStoreStoreExt;
use crate::stores::{auth_store, blossom_store, nostr_client, theme_store};
/// localStorage key for settings cache
const SETTINGS_LOCAL_STORAGE_KEY: &str = "nostr_blue_settings";
/// App settings stored on Nostr via NIP-78
/// Note: Relay configuration is now stored via NIP-65 (kind 10002) and NIP-17 (kind 10050)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub theme: String,
    #[serde(default)]
    pub blossom_servers: Vec<String>,
    #[serde(default)]
    pub sync_notifications: bool,
    #[serde(default)]
    pub payment_method_preference: String,
    #[serde(default = "default_mempool_endpoint")]
    pub mempool_endpoint: String,
    #[serde(default)]
    pub cashu_wallet_auto_load: bool,
    #[serde(default)]
    pub version: u32,
}
fn default_mempool_endpoint() -> String {
    crate::services::mempool::DEFAULT_ENDPOINT.to_string()
}
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            blossom_servers: vec![blossom_store::DEFAULT_SERVER.to_string()],
            sync_notifications: false,
            payment_method_preference: "nwc_first".to_string(),
            mempool_endpoint: default_mempool_endpoint(),
            cashu_wallet_auto_load: false,
            version: 5,
        }
    }
}
/// NIP-78 kind for arbitrary custom app data
const APP_DATA_KIND: u16 = 30078;
/// D tag identifier for nostr.blue settings
const SETTINGS_D_TAG: &str = "nostr.blue/settings";
/// Global settings state
pub static SETTINGS: GlobalSignal<AppSettings> = Signal::global(AppSettings::default);
pub static SETTINGS_LOADING: GlobalSignal<bool> = Signal::global(|| false);
pub static SETTINGS_ERROR: GlobalSignal<Option<String>> = Signal::global(|| None);
/// Load cached settings from localStorage (synchronous)
fn load_cached_settings() -> Option<AppSettings> {
    storage::get::<AppSettings>(SETTINGS_LOCAL_STORAGE_KEY).ok()
}
/// Save settings to localStorage cache
fn cache_settings(settings: &AppSettings) {
    let _ = storage::set(SETTINGS_LOCAL_STORAGE_KEY, settings);
}
/// Initialize settings from localStorage cache (synchronous, for instant UI)
/// Call this during app init BEFORE async client initialization
pub fn init_settings_from_cache() {
    match load_cached_settings() {
        Some(cached) => {
            log::info!("Initialized settings from localStorage cache");
            *SETTINGS.write() = cached;
        }
        None => {
            log::debug!("No settings cache found or failed to parse");
        }
    }
}
/// Load settings from Nostr relays (NIP-78)
pub async fn load_settings() -> Result<(), String> {
    log::info!("Loading settings from Nostr (NIP-78)...");
    SETTINGS_LOADING.write().clone_from(&true);
    SETTINGS_ERROR.write().clone_from(&None);
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, using local settings");
        SETTINGS_LOADING.write().clone_from(&false);
        return Ok(());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let auth = auth_store::AUTH_STATE.read();
    let pubkey_str = auth.pubkey.as_ref().ok_or("No pubkey")?;
    let pubkey = nostr_sdk::PublicKey::from_bech32(pubkey_str)
        .or_else(|_| nostr_sdk::PublicKey::from_hex(pubkey_str))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(SETTINGS_D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found settings event: {}", event.id);
                match serde_json::from_str::<AppSettings>(&event.content) {
                    Ok(settings) => {
                        log::info!("Loaded settings from Nostr: {:?}", settings);
                        let theme = match settings.theme.as_str() {
                            "light" => theme_store::Theme::Light,
                            "dark" => theme_store::Theme::Dark,
                            _ => theme_store::Theme::System,
                        };
                        theme_store::set_theme_internal(theme);
                        if !settings.blossom_servers.is_empty() {
                            *blossom_store::BLOSSOM_SERVERS.read().data().write() = settings
                                .blossom_servers
                                .clone();
                        }
                        cache_settings(&settings);
                        SETTINGS.write().clone_from(&settings);
                        SETTINGS_LOADING.write().clone_from(&false);
                        return Ok(());
                    }
                    Err(e) => {
                        log::warn!("Failed to parse settings: {}", e);
                        SETTINGS_ERROR
                            .write()
                            .clone_from(&Some(format!("Parse error: {}", e)));
                    }
                }
            } else {
                log::info!("No settings found on Nostr, using defaults");
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch settings: {}", e);
            SETTINGS_ERROR.write().clone_from(&Some(format!("Fetch error: {}", e)));
        }
    }
    SETTINGS_LOADING.write().clone_from(&false);
    Ok(())
}
/// Save settings to Nostr relays (NIP-78)
pub async fn save_settings(settings: &AppSettings) -> Result<(), String> {
    log::info!("Saving settings to Nostr (NIP-78)...");
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, skipping Nostr sync");
        return Ok(());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let mut settings_to_save = settings.clone();
    settings_to_save.blossom_servers = blossom_store::BLOSSOM_SERVERS
        .read()
        .data()
        .read()
        .clone();
    let content = serde_json::to_string(&settings_to_save)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    let builder = EventBuilder::new(Kind::from(APP_DATA_KIND), content)
        .tag(Tag::identifier(SETTINGS_D_TAG));
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish settings: {}", e))?;
    log::info!("Settings saved to Nostr successfully");
    cache_settings(&settings_to_save);
    SETTINGS.write().clone_from(&settings_to_save);
    Ok(())
}
/// Update theme and save to Nostr
#[allow(dead_code)]
pub async fn update_theme(theme: theme_store::Theme) {
    theme_store::set_theme_internal(theme);
    let mut settings = SETTINGS.read().clone();
    settings.theme = match theme {
        theme_store::Theme::Light => "light".to_string(),
        theme_store::Theme::Dark => "dark".to_string(),
        theme_store::Theme::System => "system".to_string(),
    };
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save theme setting: {}", e);
    }
}
/// Update notification sync setting and save to Nostr
pub async fn update_notification_sync(enabled: bool) {
    let mut settings = SETTINGS.read().clone();
    settings.sync_notifications = enabled;
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save notification sync setting: {}", e);
    }
}
/// Update payment method preference and save to Nostr
pub async fn update_payment_method_preference(preference: String) {
    let settings = {
        let mut w = SETTINGS.write();
        w.payment_method_preference = preference;
        w.clone()
    };
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save payment method preference: {}", e);
    }
}
/// Get current mempool endpoint (returns default if empty)
pub fn get_mempool_endpoint() -> String {
    let settings = SETTINGS.read();
    if settings.mempool_endpoint.is_empty() {
        default_mempool_endpoint()
    } else {
        settings.mempool_endpoint.clone()
    }
}
/// Update mempool endpoint and save to Nostr
pub async fn update_mempool_endpoint(endpoint: String) {
    let mut settings = SETTINGS.read().clone();
    settings.mempool_endpoint = if endpoint.is_empty() {
        default_mempool_endpoint()
    } else {
        endpoint.trim_end_matches('/').to_string()
    };
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save mempool endpoint: {}", e);
    }
}
/// Reset mempool endpoint to default
pub async fn reset_mempool_endpoint() {
    update_mempool_endpoint(default_mempool_endpoint()).await;
}
/// Update Cashu wallet auto-load setting and save to Nostr
pub async fn update_cashu_wallet_auto_load(enabled: bool) {
    let settings = {
        let mut w = SETTINGS.write();
        w.cashu_wallet_auto_load = enabled;
        w.clone()
    };
    if let Err(e) = save_settings(&settings).await {
        log::warn!("Failed to persist Cashu wallet auto-load setting to Nostr: {}", e);
        cache_settings(&settings);
    }
}
