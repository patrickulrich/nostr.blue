//! Follow/unfollow (kind 3)
//!
//! Functions for managing contacts list (NIP-02).

use std::time::Duration;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::{get_client, fetch_events_aggregated_outbox};
use super::signals::{HAS_SIGNER, get_contacts_cache, CachedContacts, invalidate_contacts_cache};
use super::types::PublishResult;

// =============================================================================
// Contact Fetching
// =============================================================================

/// Fetch a user's contact list (kind 3 event)
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
/// Uses a 5-minute cache to speed up repeated calls
pub async fn fetch_contacts(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    // Check cache first (5-minute TTL)
    {
        let cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(ref cached) = *cache {
            if cached.pubkey == pubkey_str
               && cached.cached_at.elapsed() < Duration::from_secs(300) {
                log::info!("Contacts cache hit ({} contacts)", cached.contacts.len());
                let contacts = cached.contacts.clone();
                drop(cache); // Release lock before spawning

                // Background refresh (don't await)
                let pk = pubkey_str.clone();
                dioxus::prelude::spawn(async move {
                    let _ = fetch_contacts_from_relay(pk).await;
                });

                return Ok(contacts);
            }
        }
    }

    // Cache miss - fetch from relay
    fetch_contacts_from_relay(pubkey_str).await
}

/// Fetch contacts directly from relay, bypassing cache
/// Use this when you need guaranteed fresh data (e.g., after modifications)
pub(crate) async fn fetch_contacts_from_relay(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    log::info!("Fetching contacts from relay for: {}", pubkey_str);

    // Parse pubkey
    use nostr::{PublicKey, Filter, Kind};
    let pubkey = PublicKey::from_hex(&pubkey_str)
        .or_else(|_| PublicKey::parse(&pubkey_str))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Create filter for kind 3 (contact list)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::ContactList)
        .limit(1);

    // Fetch from database/relays using outbox routing for better discovery
    // This routes the query to the author's preferred write relays
    match fetch_events_aggregated_outbox(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            // Select latest event by timestamp (nostr-database pattern)
            if let Some(event) = events.into_iter().max_by_key(|e| e.created_at) {
                // Use SDK's public_keys() method to extract p-tags
                let contacts: Vec<String> = event.tags.public_keys()
                    .map(|pk| pk.to_string())
                    .collect();
                log::info!("Found {} contacts from relay", contacts.len());

                // Update cache
                {
                    let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    *cache = Some(CachedContacts {
                        pubkey: pubkey_str,
                        contacts: contacts.clone(),
                        cached_at: instant::Instant::now(),
                    });
                }

                Ok(contacts)
            } else {
                log::info!("No contact list found");
                Ok(Vec::new())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch contacts: {}", e);
            Err(format!("Failed to fetch contacts: {}", e))
        }
    }
}

// =============================================================================
// Contact Publishing
// =============================================================================

/// Publish a contact list (kind 3 event) with relay feedback
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
pub async fn publish_contacts_tracked(contacts: Vec<String>) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing contact list with {} contacts", contacts.len());

    // Parse contacts into Contact structs for proper NIP-02 compliance
    use nostr::PublicKey;
    use nostr_sdk::nips::nip02::Contact;
    let contact_list: Vec<Contact> = contacts
        .into_iter()
        .filter_map(|contact_str| {
            // Try to parse as hex or NIP-19
            match PublicKey::from_hex(&contact_str)
                .or_else(|_| PublicKey::parse(&contact_str)) {
                Ok(pk) => Some(Contact::new(pk)),
                Err(_) => {
                    log::debug!("Skipping invalid contact pubkey: {}", contact_str);
                    None
                }
            }
        })
        .collect();

    log::info!("Parsed {} valid contacts", contact_list.len());

    // Use EventBuilder::contact_list() for proper NIP-02 compliance
    // This allows for relay URLs and petnames (aliases) to be added in the future
    let builder = nostr::EventBuilder::contact_list(contact_list);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish contact list: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Contact list published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a contact list (kind 3 event)
/// For relay feedback, use publish_contacts_tracked instead
pub async fn publish_contacts(contacts: Vec<String>) -> std::result::Result<String, String> {
    publish_contacts_tracked(contacts)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// Follow/Unfollow Operations
// =============================================================================

/// Follow a user (adds to contact list and publishes)
pub async fn follow_user(pubkey_to_follow: String) -> std::result::Result<(), String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_follow)?;

    // Get current user's pubkey
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Invalidate cache and fetch directly from relay to avoid race with background refresher
    invalidate_contacts_cache();
    let mut contacts = fetch_contacts_from_relay(current_pubkey.clone()).await?;

    // Add new contact if not already following
    if !contacts.contains(&normalized_pubkey) {
        contacts.push(normalized_pubkey.clone());
        log::info!("Following new user: {}", normalized_pubkey);

        // Publish updated contact list
        publish_contacts(contacts).await?;

        // Invalidate cache after successful publish (nostr-sdk pattern)
        invalidate_contacts_cache();
    } else {
        log::info!("Already following: {}", normalized_pubkey);
    }

    Ok(())
}

/// Unfollow a user (removes from contact list and publishes)
pub async fn unfollow_user(pubkey_to_unfollow: String) -> std::result::Result<(), String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_unfollow)?;

    // Get current user's pubkey
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Invalidate cache and fetch directly from relay to avoid race with background refresher
    invalidate_contacts_cache();
    let mut contacts = fetch_contacts_from_relay(current_pubkey.clone()).await?;

    // Remove contact if following
    if let Some(pos) = contacts.iter().position(|x| x == &normalized_pubkey) {
        contacts.remove(pos);
        log::info!("Unfollowing user: {}", normalized_pubkey);

        // Publish updated contact list
        publish_contacts(contacts).await?;

        // Invalidate cache after successful publish (nostr-sdk pattern)
        invalidate_contacts_cache();
    } else {
        log::info!("Not following: {}", normalized_pubkey);
    }

    Ok(())
}

/// Check if current user is following a specific pubkey
pub async fn is_following(pubkey: String) -> std::result::Result<bool, String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    let contacts = fetch_contacts(current_pubkey).await?;
    Ok(contacts.contains(&normalized_pubkey))
}
