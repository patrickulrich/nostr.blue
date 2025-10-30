/// NIP-78: Application Data Storage
/// Stores user settings on Nostr relays using kind 30078 events
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, Kind, Tag, FromBech32};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::stores::{auth_store, nostr_client, theme_store};

/// App settings stored on Nostr via NIP-78
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub theme: String, // "light", "dark", or "system"
    pub relay_urls: Vec<String>,
    #[serde(default)]
    pub version: u32, // Settings schema version
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            relay_urls: vec![
                "wss://relay.damus.io".to_string(),
                "wss://relay.nostr.band".to_string(),
                "wss://nos.lol".to_string(),
            ],
            version: 1,
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
    let client = nostr_client::NOSTR_CLIENT.read()
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

                        // Update global settings
                        SETTINGS.write().clone_from(&settings);
                        SETTINGS_LOADING.write().clone_from(&false);
                        return Ok(());
                    }
                    Err(e) => {
                        log::warn!("Failed to parse settings: {}", e);
                        SETTINGS_ERROR.write().clone_from(&Some(format!("Parse error: {}", e)));
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

    // Check if authenticated
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, skipping Nostr sync");
        return Ok(());
    }

    // Get client
    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    // Serialize settings to JSON
    let content = serde_json::to_string(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    // Build NIP-78 event (kind 30078 with 'd' tag)
    let builder = EventBuilder::new(Kind::from(APP_DATA_KIND), content)
        .tag(Tag::identifier(SETTINGS_D_TAG));

    // Publish to relays
    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish settings: {}", e))?;

    log::info!("Settings saved to Nostr successfully");

    // Update global settings
    SETTINGS.write().clone_from(settings);

    Ok(())
}

/// Update theme and save to Nostr
#[allow(dead_code)] // Called from theme_store.rs
pub async fn update_theme(theme: theme_store::Theme) {
    // Apply theme locally (using internal function to avoid recursion)
    theme_store::set_theme_internal(theme.clone());

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

/// Update relay list and save to Nostr
#[allow(dead_code)]
pub async fn update_relay_list(relay_urls: Vec<String>) {
    let mut settings = SETTINGS.read().clone();
    settings.relay_urls = relay_urls;

    // Save to Nostr
    if let Err(e) = save_settings(&settings).await {
        log::error!("Failed to save relay list: {}", e);
    }
}
