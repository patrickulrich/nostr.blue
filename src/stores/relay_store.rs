//! MLS key package relay management (kind 10051)
//!
//! NIP-65 and NIP-17 are handled automatically by nostr-sdk's gossip layer.
//! This module only handles key package relays which gossip doesn't support.

use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

/// Global key package relays for MLS
pub static KEY_PACKAGE_RELAYS: GlobalSignal<Vec<String>> = Signal::global(|| vec![]);

/// Fetch key package relays (kind 10051) for a user
pub async fn fetch_key_package_relays(pubkey: &str) -> std::result::Result<(), String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;

    let pk = PublicKey::parse(pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .author(pk)
        .kind(Kind::Custom(10051))
        .limit(1);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch key package relays: {}", e))?;

    if let Some(event) = events.first() {
        let relays: Vec<String> = event.tags.iter()
            .filter_map(|t| {
                let parts = t.as_slice();
                if parts.first().map(|s| s.as_str()) == Some("relay") {
                    parts.get(1).map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        *KEY_PACKAGE_RELAYS.write() = relays;
        log::debug!("Loaded {} key package relays", KEY_PACKAGE_RELAYS.read().len());
    }

    Ok(())
}

/// Get key package relays for MLS
/// Falls back to defaults if not configured
pub fn get_key_package_relays() -> Vec<String> {
    let relays = KEY_PACKAGE_RELAYS.read().clone();
    if relays.is_empty() {
        vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
            "wss://relay.nostr.band".to_string(),
        ]
    } else {
        relays
    }
}
