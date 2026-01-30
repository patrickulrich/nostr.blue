/// NIP-78: Application Data Storage
/// Stores user settings on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use gloo_storage::Storage;
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
    pub theme: String, // "light", "dark", or "system"
    // relay_urls removed - now using kind 10002/10050 (NIP-65/NIP-17)
    #[serde(default)]
    pub blossom_servers: Vec<String>, // Blossom media upload servers
    #[serde(default)]
    pub sync_notifications: bool, // Sync notification read status across devices via NIP-78
    #[serde(default)]
    pub payment_method_preference: String, // "nwc_first", "webln_first", "manual_only", "always_ask"
    #[serde(default = "default_mempool_endpoint")]
    pub mempool_endpoint: String, // Custom mempool.space API endpoint
    #[serde(default)]
    pub cashu_wallet_auto_load: bool, // Auto-initialize Cashu wallet on app startup
    #[serde(default)]
    pub version: u32, // Settings schema version
}

fn default_mempool_endpoint() -> String {
    crate::services::mempool::DEFAULT_ENDPOINT.to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            // relay_urls removed - now using kind 10002/10050 (NIP-65/NIP-17)
            blossom_servers: vec![blossom_store::DEFAULT_SERVER.to_string()],
            sync_notifications: false, // Privacy-first: opt-in by default
            payment_method_preference: "nwc_first".to_string(), // Default to NWC if connected
            mempool_endpoint: default_mempool_endpoint(),
            cashu_wallet_auto_load: false, // Opt-in, default off
            version: 5,                    // Incremented for cashu_wallet_auto_load addition
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

// =============================================================================
// localStorage Cache Functions (for instant synchronous loading)
// =============================================================================

/// Load cached settings from localStorage (synchronous)
fn load_cached_settings() -> Option<AppSettings> {
    gloo_storage::LocalStorage::get::<AppSettings>(SETTINGS_LOCAL_STORAGE_KEY).ok()
}

/// Save settings to localStorage cache
fn cache_settings(settings: &AppSettings) {
    let _ = gloo_storage::LocalStorage::set(SETTINGS_LOCAL_STORAGE_KEY, settings);
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

    // Check if authenticated
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, using local settings");
        SETTINGS_LOADING.write().clone_from(&false);
        return Ok(());
    }

    // Get client and pubkey
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

    // Build filter for settings event
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(APP_DATA_KIND))
        .identifier(SETTINGS_D_TAG)
        .limit(1);

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    // Fetch settings event
    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found settings event: {}", event.id);

                // Parse settings from content
                match serde_json::from_str::<AppSettings>(&event.content) {
                    Ok(settings) => {
                        log::info!("Loaded settings from Nostr: {:?}", settings);

                        // Apply theme (use internal to avoid re-publishing)
                        let theme = match settings.theme.as_str() {
                            "light" => theme_store::Theme::Light,
                            "dark" => theme_store::Theme::Dark,
                            _ => theme_store::Theme::System,
                        };
                        theme_store::set_theme_internal(theme);

                        // Update Blossom servers
                        if !settings.blossom_servers.is_empty() {
                            *blossom_store::BLOSSOM_SERVERS.read().data().write() =
                                settings.blossom_servers.clone();
                        }

                        // Cache to localStorage for instant loading next time
                        cache_settings(&settings);

                        // Update global settings
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
            SETTINGS_ERROR
                .write()
                .clone_from(&Some(format!("Fetch error: {}", e)));
        }
    }

    SETTINGS_LOADING.write().clone_from(&false);
    Ok(())
}

/// Save settings to Nostr relays (NIP-78)
pub async fn save_settings(settings: &AppSettings) -> Result<(), String> {
    log::info!("Saving settings to Nostr (NIP-78)...");

    // Check if authenticated
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, skipping Nostr sync");
        return Ok(());
    }

    // Get client
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Create settings with current blossom servers
    let mut settings_to_save = settings.clone();
    settings_to_save.blossom_servers = blossom_store::BLOSSOM_SERVERS.read().data().read().clone();

    // Serialize settings to JSON
    let content = serde_json::to_string(&settings_to_save)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    // Build NIP-78 event (kind 30078 with 'd' tag)
    let builder =
        EventBuilder::new(Kind::from(APP_DATA_KIND), content).tag(Tag::identifier(SETTINGS_D_TAG));

    // Publish to relays
    client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish settings: {}", e))?;

    log::info!("Settings saved to Nostr successfully");

    // Cache to localStorage for instant loading next time
    cache_settings(&settings_to_save);

    // Update global settings
    SETTINGS.write().clone_from(&settings_to_save);

    Ok(())
}

/// Update theme and save to Nostr
#[allow(dead_code)] // Called from theme_store.rs
pub async fn update_theme(theme: theme_store::Theme) {
    // Apply theme locally (using internal function to avoid recursion)
    theme_store::set_theme_internal(theme);

    // Update settings
    let mut settings = SETTINGS.read().clone();
    settings.theme = match theme {
        theme_store::Theme::Light => "light".to_string(),
        theme_store::Theme::Dark => "dark".to_string(),
        theme_store::Theme::System => "system".to_string(),
    };

    // Save to Nostr
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save theme setting: {}", e);
    }
}

/// Update notification sync setting and save to Nostr
pub async fn update_notification_sync(enabled: bool) {
    let mut settings = SETTINGS.read().clone();
    settings.sync_notifications = enabled;

    // Save to Nostr
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save notification sync setting: {}", e);
    }
}

/// Update payment method preference and save to Nostr
pub async fn update_payment_method_preference(preference: String) {
    // Update in-memory state immediately so UI reflects change (Dioxus pattern)
    let settings = {
        let mut w = SETTINGS.write();
        w.payment_method_preference = preference;
        w.clone() // Clone for async save
    }; // Lock released here

    // Async persist to Nostr
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
        // Ensure endpoint doesn't have trailing slash
        endpoint.trim_end_matches('/').to_string()
    };

    // Save to Nostr
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
    // Update in-memory state immediately so UI reflects change (Dioxus pattern)
    let settings = {
        let mut w = SETTINGS.write();
        w.cashu_wallet_auto_load = enabled;
        w.clone() // Clone for async save
    }; // Lock released here

    // Async persist to Nostr first
    if let Err(e) = save_settings(&settings).await {
        log::warn!(
            "Failed to persist Cashu wallet auto-load setting to Nostr: {}",
            e
        );
        // Defensive: cache locally on Nostr failure
        cache_settings(&settings);
    }
}
