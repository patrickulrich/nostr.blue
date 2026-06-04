//! Follow/unfollow (kind 3)
//!
//! Functions for managing contacts list (NIP-02).
use super::fetching::{fetch_events_aggregated, get_client};
use super::signals::{
    get_cache_generation, get_contacts_cache, invalidate_contacts_cache, CachedContacts, HAS_SIGNER,
};
use super::types::PublishResult;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};
/// nostr-sdk pattern: Minimum interval between background refresh spawns (60 seconds)
const BACKGROUND_REFRESH_COOLDOWN_SECS: u64 = 60;
/// Kind 3 (contact list) fetch timeout. Smaller payloads than feed events, fast
/// relays typically respond in <2s, so 5s is plenty.
const CONTACTS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// In-flight dedup: collapses concurrent `fetch_contacts` callers for the
/// same pubkey into a single network round-trip. Cold-start paths in
/// `run_post_login_init`, `load_following_feed_streaming`, and the home route
/// all hit this concurrently without it. Stores the `Result` so all waiters
/// observe the same error/success outcome.
type FetchContactsResult = std::result::Result<Vec<String>, String>;
type InFlightContactsMap =
    Arc<Mutex<HashMap<String, Arc<OnceCell<FetchContactsResult>>>>>;

static IN_FLIGHT_CONTACTS: Lazy<InFlightContactsMap> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));
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
        Self {
            pubkey,
            relay_url: None,
            petname: None,
        }
    }
}
/// Fetch a user's contact list (kind 3 event)
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
/// Uses a 5-minute cache to speed up repeated calls
/// Returns pubkeys only; for relay hints/petnames, see internal enriched functions
///
/// In-flight dedup: concurrent callers for the same pubkey share a single
/// network round-trip via `tokio::sync::OnceCell`. Cold-start paths in
/// `run_post_login_init` and the home feed loader race this without firing
/// multiple relays.
pub async fn fetch_contacts(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_str)?;
    {
        let mut cache = get_contacts_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(ref mut cached) = *cache {
            if cached.pubkey == normalized_pubkey
                && cached.cached_at.elapsed() < Duration::from_secs(300)
            {
                log::info!("Contacts cache hit ({} contacts)", cached.contacts.len());
                let contacts: Vec<String> =
                    cached.contacts.iter().map(|c| c.pubkey.clone()).collect();
                let should_refresh = cached
                    .last_refresh_spawned
                    .map(|t| t.elapsed() >= Duration::from_secs(BACKGROUND_REFRESH_COOLDOWN_SECS))
                    .unwrap_or(true);
                if should_refresh {
                    cached.last_refresh_spawned = Some(instant::Instant::now());
                    drop(cache);
                    let start_gen = get_cache_generation();
                    let pk = normalized_pubkey.clone();
                    dioxus::prelude::spawn(async move {
                        let _ = fetch_enriched_contacts_from_relay_with_gen(pk, start_gen).await;
                    });
                } else {
                    log::debug!("Skipping background refresh - cooldown not elapsed");
                }
                return Ok(contacts);
            }
        }
    }

    // In-flight dedup: grab-or-init a OnceCell keyed on the normalized pubkey.
    // The first caller does the network work; concurrent callers await the
    // same future. The cell is removed on completion so a later call can
    // refetch.
    let cell = {
        let mut map = IN_FLIGHT_CONTACTS.lock().await;
        map.entry(normalized_pubkey.clone())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone()
    };

    let pk_for_init = normalized_pubkey.clone();
    let result = cell
        .get_or_init(|| async move { fetch_contacts_from_relay(pk_for_init).await })
        .await
        .clone();

    {
        let mut map = IN_FLIGHT_CONTACTS.lock().await;
        if let Some(c) = map.get(&normalized_pubkey) {
            if Arc::ptr_eq(c, &cell) {
                map.remove(&normalized_pubkey);
            }
        }
    }

    result
}
/// Internal: Fetch enriched contacts from relay with full NIP-02 data
/// Parses p-tags per nostr-sdk pattern: ["p", pubkey, relay_hint?, petname?]
async fn fetch_enriched_contacts_from_relay(
    pubkey_str: String,
) -> std::result::Result<Vec<EnrichedContact>, String> {
    fetch_enriched_contacts_from_relay_impl(pubkey_str, None).await
}
/// Internal: Fetch enriched contacts with generation check for background refresh
/// Dioxus pattern: Check generation before write to prevent stale overwrites (memory_cache.rs:45-88)
async fn fetch_enriched_contacts_from_relay_with_gen(
    pubkey_str: String,
    start_gen: u64,
) -> std::result::Result<Vec<EnrichedContact>, String> {
    fetch_enriched_contacts_from_relay_impl(pubkey_str, Some(start_gen)).await
}
/// Internal implementation with optional generation check
async fn fetch_enriched_contacts_from_relay_impl(
    pubkey_str: String,
    start_gen: Option<u64>,
) -> std::result::Result<Vec<EnrichedContact>, String> {
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_str)?;
    debug_assert!(
        pubkey_str.eq_ignore_ascii_case(&normalized_pubkey) || pubkey_str.starts_with("npub"),
        "Unexpected pubkey format: input '{}' normalized to '{}'",
        pubkey_str,
        normalized_pubkey,
    );
    log::info!(
        "Fetching enriched contacts from relay for: {}",
        normalized_pubkey
    );
    use nostr::{Filter, Kind, PublicKey};
    let pubkey =
        PublicKey::from_hex(&normalized_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::ContactList)
        .limit(1);
    match fetch_events_aggregated(filter, CONTACTS_FETCH_TIMEOUT).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().max_by_key(|e| e.created_at) {
                if let Err(e) = event.verify() {
                    log::warn!("Contact list event failed verification: {}", e);
                    return Err(format!("Invalid contact list event: {}", e));
                }
                let contacts: Vec<EnrichedContact> = event
                    .tags
                    .iter()
                    .filter_map(|tag| {
                        let parts = tag.as_slice();
                        if parts.first().map(|s| s.as_str()) != Some("p") || parts.len() < 2 {
                            return None;
                        }
                        let pubkey_str = parts[1].as_str();
                        let normalized_pubkey = match nostr::PublicKey::from_hex(pubkey_str)
                            .or_else(|_| nostr::PublicKey::parse(pubkey_str))
                        {
                            Ok(pk) => pk.to_hex(),
                            Err(_) => {
                                log::debug!(
                                    "Skipping invalid pubkey in contact p-tag: {}",
                                    pubkey_str
                                );
                                return None;
                            }
                        };
                        Some(EnrichedContact {
                            pubkey: normalized_pubkey,
                            relay_url: parts
                                .get(2)
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                            petname: parts
                                .get(3)
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string()),
                        })
                    })
                    .collect();
                log::info!("Found {} enriched contacts from relay", contacts.len());
                let current_gen = get_cache_generation();
                if let Some(gen) = start_gen {
                    if gen != current_gen {
                        log::debug!(
                            "Discarding stale background refresh (gen {} vs current {})",
                            gen,
                            current_gen
                        );
                        return Ok(contacts);
                    }
                }
                {
                    let mut cache = get_contacts_cache()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let existing_refresh = cache.as_ref().and_then(|c| c.last_refresh_spawned);
                    *cache = Some(CachedContacts {
                        pubkey: normalized_pubkey,
                        contacts: contacts.clone(),
                        cached_at: instant::Instant::now(),
                        last_refresh_spawned: existing_refresh,
                        generation: current_gen,
                    });
                }
                Ok(contacts)
            } else {
                log::info!("No contact list found for {}", normalized_pubkey);
                let current_gen = get_cache_generation();
                if let Some(gen) = start_gen {
                    if gen != current_gen {
                        log::debug!(
                            "Discarding stale background refresh (gen {} vs current {})",
                            gen,
                            current_gen
                        );
                        return Ok(Vec::new());
                    }
                }
                {
                    let mut cache = get_contacts_cache()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let existing_refresh = cache.as_ref().and_then(|c| c.last_refresh_spawned);
                    *cache = Some(CachedContacts {
                        pubkey: normalized_pubkey,
                        contacts: Vec::new(),
                        cached_at: instant::Instant::now(),
                        last_refresh_spawned: existing_refresh,
                        generation: current_gen,
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
pub(crate) async fn fetch_contacts_from_relay(
    pubkey_str: String,
) -> std::result::Result<Vec<String>, String> {
    let enriched = fetch_enriched_contacts_from_relay(pubkey_str).await?;
    Ok(enriched.iter().map(|c| c.pubkey.clone()).collect())
}
/// Internal: Publish enriched contacts preserving relay hints and petnames
/// Uses nostr-sdk EventBuilder::contact_list() pattern
async fn publish_enriched_contacts(
    contacts: Vec<EnrichedContact>,
) -> std::result::Result<PublishResult, String> {
    let _client = get_client().ok_or("Client not initialized")?;
    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }
    let input_count = contacts.len();
    log::info!(
        "Publishing enriched contact list with {} contacts",
        input_count
    );
    use nostr::PublicKey;
    use nostr_sdk::nips::nip02::Contact;
    let mut dropped_pubkeys: Vec<String> = Vec::new();
    let contact_list: Vec<Contact> = contacts
        .into_iter()
        .filter_map(|c| {
            match PublicKey::from_hex(&c.pubkey).or_else(|_| PublicKey::parse(&c.pubkey)) {
                Ok(pk) => {
                    let mut contact = Contact::new(pk);
                    if let Some(relay) = c.relay_url {
                        match nostr::RelayUrl::parse(&relay) {
                            Ok(url) => contact.relay_url = Some(url),
                            Err(e) => {
                                log::debug!(
                                    "Invalid relay URL '{}' in contact, skipping: {}",
                                    relay,
                                    e
                                );
                            }
                        }
                    }
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
            dropped_pubkeys.iter().take(5).collect::<Vec<_>>()
        );
    }
    log::info!(
        "Publishing {} valid contacts (dropped {} invalid)",
        contact_list.len(),
        input_count - contact_list.len()
    );
    let builder = nostr::EventBuilder::contact_list(contact_list);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign contact list: {}", e))?;
    let event_id = event.id.to_hex();
    let queue_id = crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Contacts,
        None,
        std::collections::HashMap::new(),
    ).await;
    let result = PublishResult::queued(queue_id, event_id);
    log::info!("Contact list queued: {}", result.event_id);
    Ok(result)
}
/// Publish a contact list (kind 3 event) with relay feedback
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
/// Note: This creates contacts without relay hints/petnames. For preserving
/// existing metadata, use follow_user/unfollow_user which work with enriched data.
#[allow(dead_code)]
pub async fn publish_contacts_tracked(
    contacts: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let enriched: Vec<EnrichedContact> = contacts.into_iter().map(EnrichedContact::new).collect();
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
/// Follow a user (adds to contact list and publishes)
/// Preserves relay hints and petnames of existing contacts
pub async fn follow_user(pubkey_to_follow: String) -> std::result::Result<(), String> {
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_follow)?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    invalidate_contacts_cache();
    let mut contacts = fetch_enriched_contacts_from_relay(current_pubkey.clone()).await?;
    if !contacts.iter().any(|c| c.pubkey == normalized_pubkey) {
        contacts.push(EnrichedContact::new(normalized_pubkey.clone()));
        log::info!("Following new user: {}", normalized_pubkey);
        publish_enriched_contacts(contacts).await?;
        invalidate_contacts_cache();
    } else {
        log::info!("Already following: {}", normalized_pubkey);
    }
    Ok(())
}
/// Unfollow a user (removes from contact list and publishes)
/// Preserves relay hints and petnames of remaining contacts
pub async fn unfollow_user(pubkey_to_unfollow: String) -> std::result::Result<(), String> {
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_unfollow)?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    invalidate_contacts_cache();
    let mut contacts = fetch_enriched_contacts_from_relay(current_pubkey.clone()).await?;
    let original_len = contacts.len();
    contacts.retain(|c| c.pubkey != normalized_pubkey);
    if contacts.len() < original_len {
        log::info!(
            "Unfollowing user: {} (removed {} entries)",
            normalized_pubkey,
            original_len - contacts.len()
        );
        publish_enriched_contacts(contacts).await?;
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
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let contacts = fetch_contacts(current_pubkey).await?;
    Ok(contacts.contains(&normalized_pubkey))
}
/// Batch follow multiple users in a single contact list publish
/// Returns the number of newly followed users (skips already-followed)
pub async fn follow_users_batch(
    pubkeys_to_follow: Vec<String>,
) -> std::result::Result<usize, String> {
    if pubkeys_to_follow.is_empty() {
        return Ok(0);
    }
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    invalidate_contacts_cache();
    let mut contacts = fetch_enriched_contacts_from_relay(current_pubkey).await?;
    let mut existing: std::collections::HashSet<String> =
        contacts.iter().map(|c| c.pubkey.clone()).collect();
    let mut new_count = 0;
    for pk in pubkeys_to_follow {
        match crate::utils::nip19::normalize_pubkey(&pk) {
            Ok(normalized) => {
                if existing.insert(normalized.clone()) {
                    contacts.push(EnrichedContact::new(normalized));
                    new_count += 1;
                }
            }
            Err(e) => {
                log::warn!("Skipping invalid pubkey '{}': {}", pk, e);
            }
        }
    }
    if new_count > 0 {
        publish_enriched_contacts(contacts).await?;
        invalidate_contacts_cache();
    }
    Ok(new_count)
}
