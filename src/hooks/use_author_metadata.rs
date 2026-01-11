//! Author Metadata Hook
//!
//! Reusable hook for fetching author metadata with database-first, network-fallback pattern.
//! This reduces code duplication across components that need to display author information.

use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::stores::nostr_client::get_client;

/// Fetch author metadata reactively with database-first, network-fallback pattern.
///
/// # Arguments
/// * `pubkey` - The hex-encoded public key of the author
///
/// # Returns
/// A signal containing the author's metadata, or None if not yet fetched
///
/// # Example
/// ```rust
/// let metadata = use_author_metadata(author_pubkey.clone());
/// let display_name = metadata.read().as_ref()
///     .and_then(|m| m.display_name.clone().or(m.name.clone()))
///     .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
/// ```
pub fn use_author_metadata(pubkey: String) -> Signal<Option<Metadata>> {
    let mut metadata = use_signal(|| None::<Metadata>);

    use_effect(use_reactive!(|pubkey| {
        let pubkey_str = pubkey.clone();

        spawn(async move {
            // Parse the public key
            let pk = match PublicKey::from_hex(&pubkey_str) {
                Ok(pk) => pk,
                Err(e) => {
                    log::warn!("Invalid author pubkey: {}", e);
                    return;
                }
            };

            // Get the client
            let client = match get_client() {
                Some(c) => c,
                None => {
                    log::error!("Client not initialized, cannot fetch author metadata");
                    return;
                }
            };

            // Try database first (instant)
            if let Ok(Some(m)) = client.database().metadata(pk).await {
                metadata.set(Some(m));
                return;
            }

            // Fallback to network with timeout
            if let Ok(Some(m)) = client.fetch_metadata(pk, Duration::from_secs(5)).await {
                metadata.set(Some(m));
            }
        });
    }));

    metadata
}
