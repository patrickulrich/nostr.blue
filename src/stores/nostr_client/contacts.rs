//! Follow/unfollow (kind 3)
//!
//! Functions for managing contacts list (NIP-02).

use std::time::Duration;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

use super::fetching::{get_client, fetch_events_aggregated_outbox};
use super::signals::{HAS_SIGNER, get_contacts_cache, CachedContacts, invalidate_contacts_cache};
use super::types::PublishResult;

/// nostr-sdk pattern: Minimum interval between background refresh spawns (60 seconds)
const BACKGROUND_REFRESH_COOLDOWN_SECS: u64 = 60;

// =============================================================================
// EnrichedContact Type
// =============================================================================

/// Enriched contact with optional relay hint and petname (NIP-02)
/// Follows nostr-sdk Contact pattern and CDK derive conventions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichedContact {
    pub pubkey: String,
    pub relay_url: Option<String>,
    pub petname: Option<String>,
}

impl EnrichedContact {
    /// Create a new contact with just a pubkey (no relay hint or petname)
    pub fn new(pubkey: String) -> Self {
        Self { pubkey, relay_url: None, petname: None }
    }
}

// =============================================================================
// Contact Fetching
// =============================================================================

/// Fetch a user's contact list (kind 3 event)
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
/// Uses a 5-minute cache to speed up repeated calls
/// Returns pubkeys only; for relay hints/petnames, see internal enriched functions
pub async fn fetch_contacts(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    // nostr-sdk pattern: Normalize pubkey to canonical hex for consistent cache keys
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_str)?;

    // Check cache first (5-minute TTL)
    {
        let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(ref mut cached) = *cache {
            if cached.pubkey == normalized_pubkey
               && cached.cached_at.elapsed() < Duration::from_secs(300) {
                log::info!("Contacts cache hit ({} contacts)", cached.contacts.len());
                // Extract pubkeys from enriched contacts for backward compat
                let contacts: Vec<String> = cached.contacts.iter().map(|c| c.pubkey.clone()).collect();

                // nostr-sdk pattern: Check cooldown before spawning background refresh
                let should_refresh = cached.last_refresh_spawned
                    .map(|t| t.elapsed() >= Duration::from_secs(BACKGROUND_REFRESH_COOLDOWN_SECS))
                    .unwrap_or(true);

                if should_refresh {
                    cached.last_refresh_spawned = Some(instant::Instant::now());
                    drop(cache); // Release lock before spawning

                    // Background refresh (don't await) - use normalized key
                    let pk = normalized_pubkey.clone();
                    dioxus::prelude::spawn(async move {
                        let _ = fetch_enriched_contacts_from_relay(pk).await;
                    });
                } else {
                    log::debug!("Skipping background refresh - cooldown not elapsed");
                }

                return Ok(contacts);
            }
        }
    }

    // Cache miss - fetch from relay (pass normalized key)
    fetch_contacts_from_relay(normalized_pubkey).await
}

/// Internal: Fetch enriched contacts from relay with full NIP-02 data
/// Parses p-tags per nostr-sdk pattern: ["p", pubkey, relay_hint?, petname?]
async fn fetch_enriched_contacts_from_relay(pubkey_str: String) -> std::result::Result<Vec<EnrichedContact>, String> {
    // nostr-sdk pattern: Defensive normalization (may already be normalized by caller)
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_str)?;

    // Debug assertion to detect unnecessary double-normalization in debug builds
    debug_assert!(
        pubkey_str == normalized_pubkey || pubkey_str.starts_with("npub"),
        "Unexpected pubkey format: input '{}' normalized to '{}'",
        pubkey_str, normalized_pubkey
    );

    log::info!("Fetching enriched contacts from relay for: {}", normalized_pubkey);

    // Parse pubkey (now guaranteed to be hex)
    use nostr::{PublicKey, Filter, Kind};
    let pubkey = PublicKey::from_hex(&normalized_pubkey)
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
                // Parse full p-tags per NIP-02 format (nostr-sdk pattern)
                // Format: ["p", pubkey, relay_hint?, petname?]
                let contacts: Vec<EnrichedContact> = event.tags.iter()
                    .filter_map(|tag| {
                        let parts: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
                        // nostr-sdk pattern: check tag type and minimum length
                        if parts.first() != Some(&"p") || parts.len() < 2 {
                            return None;
                        }

                        // nostr-sdk pattern: try hex first (most common), then parse for npub
                        // Normalize to canonical lowercase hex for consistent comparisons
                        let normalized_pubkey = match nostr::PublicKey::from_hex(parts[1])
                            .or_else(|_| nostr::PublicKey::parse(parts[1])) {
                            Ok(pk) => pk.to_hex(), // Canonical lowercase hex
                            Err(_) => {
                                log::debug!("Skipping invalid pubkey in contact p-tag: {}", parts[1]);
                                return None;
                            }
                        };

                        // nostr-sdk pattern: extract optional fields by position, check empty
                        Some(EnrichedContact {
                            pubkey: normalized_pubkey,
                            // Position 2: relay hint (nostr-sdk: tag_2)
                            relay_url: parts.get(2)
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                            // Position 3: petname/alias (nostr-sdk: tag_3)
                            petname: parts.get(3)
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                        })
                    })
                    .collect();

                log::info!("Found {} enriched contacts from relay", contacts.len());

                // Update cache with normalized key
                {
                    let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    *cache = Some(CachedContacts {
                        pubkey: normalized_pubkey,
                        contacts: contacts.clone(),
                        cached_at: instant::Instant::now(),
                        last_refresh_spawned: None,
                    });
                }

                Ok(contacts)
            } else {
                log::info!("No contact list found for {}", normalized_pubkey);
                // nostr-sdk cache pattern: Cache empty result to avoid repeated relay queries
                {
                    let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    *cache = Some(CachedContacts {
                        pubkey: normalized_pubkey,
                        contacts: Vec::new(),
                        cached_at: instant::Instant::now(),
                        last_refresh_spawned: None,
                    });
                }
                Ok(Vec::new())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch contacts: {}", e);
            Err(format!("Failed to fetch contacts: {}", e))
        }
    }
}

/// Fetch contacts directly from relay, bypassing cache (backward compatible API)
/// For enriched data with relay hints/petnames, use internal functions
pub(crate) async fn fetch_contacts_from_relay(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    let enriched = fetch_enriched_contacts_from_relay(pubkey_str).await?;
    Ok(enriched.iter().map(|c| c.pubkey.clone()).collect())
}

// =============================================================================
// Contact Publishing
// =============================================================================

/// Internal: Publish enriched contacts preserving relay hints and petnames
/// Uses nostr-sdk EventBuilder::contact_list() pattern
async fn publish_enriched_contacts(contacts: Vec<EnrichedContact>) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    let input_count = contacts.len();
    log::info!("Publishing enriched contact list with {} contacts", input_count);

    use nostr::PublicKey;
    use nostr_sdk::nips::nip02::Contact;

    let mut dropped_pubkeys: Vec<String> = Vec::new();

    let contact_list: Vec<Contact> = contacts
        .into_iter()
        .filter_map(|c| {
            // Parse pubkey (nostr-sdk pattern: try hex first, then parse)
            match PublicKey::from_hex(&c.pubkey)
                .or_else(|_| PublicKey::parse(&c.pubkey)) {
                Ok(pk) => {
                    let mut contact = Contact::new(pk);
                    // Preserve relay hint if present
                    if let Some(relay) = c.relay_url {
                        if let Ok(url) = nostr::RelayUrl::parse(&relay) {
                            contact.relay_url = Some(url);
                        }
                    }
                    // Preserve petname/alias if present
                    if let Some(alias) = c.petname {
                        contact.alias = Some(alias);
                    }
                    Some(contact)
                }
                Err(_) => {
                    dropped_pubkeys.push(c.pubkey);
                    None
                }
            }
        })
        .collect();

    if !dropped_pubkeys.is_empty() {
        log::warn!(
            "Dropped {} invalid pubkeys from contact list: {:?}",
            dropped_pubkeys.len(),
            dropped_pubkeys.iter().take(5).collect::<Vec<_>>() // Show first 5
        );
    }

    log::info!("Publishing {} valid contacts (dropped {} invalid)", contact_list.len(), input_count - contact_list.len());

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

/// Publish a contact list (kind 3 event) with relay feedback
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
/// Note: This creates contacts without relay hints/petnames. For preserving
/// existing metadata, use follow_user/unfollow_user which work with enriched data.
#[allow(dead_code)]
pub async fn publish_contacts_tracked(contacts: Vec<String>) -> std::result::Result<PublishResult, String> {
    let enriched: Vec<EnrichedContact> = contacts
        .into_iter()
        .map(EnrichedContact::new)
        .collect();
    publish_enriched_contacts(enriched).await
}

/// Publish a contact list (kind 3 event)
/// For relay feedback, use publish_contacts_tracked instead
#[allow(dead_code)]
pub async fn publish_contacts(contacts: Vec<String>) -> std::result::Result<String, String> {
    publish_contacts_tracked(contacts)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// Follow/Unfollow Operations
// =============================================================================

/// Follow a user (adds to contact list and publishes)
/// Preserves relay hints and petnames of existing contacts
pub async fn follow_user(pubkey_to_follow: String) -> std::result::Result<(), String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_follow)?;

    // Get current user's pubkey
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Pre-publish invalidation: forces fresh relay fetch to avoid races with background refresher
    invalidate_contacts_cache();
    let mut contacts = fetch_enriched_contacts_from_relay(current_pubkey.clone()).await?;

    // Check if already following (compare by pubkey only)
    if !contacts.iter().any(|c| c.pubkey == normalized_pubkey) {
        // Add new contact with no relay hint/petname
        contacts.push(EnrichedContact::new(normalized_pubkey.clone()));
        log::info!("Following new user: {}", normalized_pubkey);

        // Publish preserving existing contacts' metadata
        publish_enriched_contacts(contacts).await?;

        // Post-publish invalidation: clears stale cache so subsequent reads see updated list
        invalidate_contacts_cache();
    } else {
        log::info!("Already following: {}", normalized_pubkey);
    }

    Ok(())
}

/// Unfollow a user (removes from contact list and publishes)
/// Preserves relay hints and petnames of remaining contacts
pub async fn unfollow_user(pubkey_to_unfollow: String) -> std::result::Result<(), String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_unfollow)?;

    // Get current user's pubkey
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Invalidate cache and fetch directly from relay to avoid race with background refresher
    invalidate_contacts_cache();
    let mut contacts = fetch_enriched_contacts_from_relay(current_pubkey.clone()).await?;

    // Find and remove by pubkey (preserves other contacts' metadata)
    if let Some(pos) = contacts.iter().position(|c| c.pubkey == normalized_pubkey) {
        contacts.remove(pos);
        log::info!("Unfollowing user: {}", normalized_pubkey);

        // Publish preserving remaining contacts' metadata
        publish_enriched_contacts(contacts).await?;

        // Invalidate cache after successful publish (nostr-sdk pattern)
        invalidate_contacts_cache();
    } else {
        log::info!("Not following: {}", normalized_pubkey);
    }

    Ok(())
}

/// Check if current user is following a specific pubkey
///
/// Uses the cached `fetch_contacts()` function with a 5-minute TTL.
///
/// # Note
/// This may return stale results if contacts changed elsewhere (e.g., from
/// another client). For guaranteed fresh data, call `fetch_contacts_from_relay()`
/// directly to bypass the cache.
///
/// # Return values
/// - `Ok(true)` if the user is following the pubkey
/// - `Ok(false)` if the user is not following the pubkey
/// - `Err(...)` if not logged in or pubkey is invalid
pub async fn is_following(pubkey: String) -> std::result::Result<bool, String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    let contacts = fetch_contacts(current_pubkey).await?;
    Ok(contacts.contains(&normalized_pubkey))
}
