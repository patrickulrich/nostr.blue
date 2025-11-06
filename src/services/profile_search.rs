use nostr_sdk::prelude::*;
use dioxus::prelude::*;
use std::time::Duration;

use crate::stores::nostr_client::NOSTR_CLIENT;
use crate::stores::profiles::PROFILE_CACHE;

/// Result type for profile search
#[derive(Clone, Debug)]
pub struct ProfileSearchResult {
    pub pubkey: PublicKey,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
    #[allow(dead_code)]
    pub nip05: Option<String>,
    pub is_contact: bool,
    pub is_thread_participant: bool,
    pub relevance: u32, // Higher = more relevant
}

impl ProfileSearchResult {
    /// Get the display name with fallback logic
    pub fn get_display_name(&self) -> String {
        if let Some(display_name) = &self.display_name {
            if !display_name.is_empty() {
                return display_name.clone();
            }
        }
        if let Some(name) = &self.name {
            if !name.is_empty() {
                return name.clone();
            }
        }
        // Fallback to truncated pubkey
        let hex = self.pubkey.to_hex();
        format!("{}...{}", &hex[..8], &hex[hex.len()-8..])
    }

    /// Get the username (name field) or None
    pub fn get_username(&self) -> Option<String> {
        self.name.clone()
    }
}

/// Search cached profiles synchronously (fast, no relay queries)
///
/// Searches through:
/// 1. Cached profiles from PROFILE_CACHE
/// 2. Prioritizes thread participants (highest priority)
/// 3. Then prioritizes contacts if contact_pubkeys is provided
///
/// Matches on `name` and `display_name` fields (case-insensitive)
/// Returns up to `limit` results sorted by relevance
pub fn search_cached_profiles(
    query: &str,
    limit: usize,
    contact_pubkeys: &[PublicKey],
    thread_pubkeys: &[PublicKey],
) -> Vec<ProfileSearchResult> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut results: Vec<ProfileSearchResult> = Vec::new();

    // Search in cached profiles
    let cache = PROFILE_CACHE.read();

    for (pubkey_str, profile) in cache.iter() {
        let pubkey = match PublicKey::from_hex(pubkey_str) {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        // Check if name or display_name matches
        let name_match = profile.name.as_ref()
            .map(|n| n.to_lowercase().contains(&query_lower))
            .unwrap_or(false);

        let display_name_match = profile.display_name.as_ref()
            .map(|d| d.to_lowercase().contains(&query_lower))
            .unwrap_or(false);

        if !name_match && !display_name_match {
            continue;
        }

        // Calculate relevance score
        let is_contact = contact_pubkeys.contains(&pubkey);
        let is_thread_participant = thread_pubkeys.contains(&pubkey);
        let mut relevance = 0u32;

        // Boost thread participants (highest priority)
        if is_thread_participant {
            relevance += 2000;
        }
        // Boost contacts (second priority)
        else if is_contact {
            relevance += 1000;
        }

        // Exact matches get highest score
        if let Some(name) = &profile.name {
            if name.to_lowercase() == query_lower {
                relevance += 500;
            } else if name.to_lowercase().starts_with(&query_lower) {
                relevance += 100;
            } else if name.to_lowercase().contains(&query_lower) {
                relevance += 50;
            }
        }

        if let Some(display_name) = &profile.display_name {
            if display_name.to_lowercase() == query_lower {
                relevance += 400;
            } else if display_name.to_lowercase().starts_with(&query_lower) {
                relevance += 80;
            } else if display_name.to_lowercase().contains(&query_lower) {
                relevance += 40;
            }
        }

        results.push(ProfileSearchResult {
            pubkey,
            name: profile.name.clone(),
            display_name: profile.display_name.clone(),
            picture: profile.picture.clone(),
            nip05: profile.nip05.clone(),
            is_contact,
            is_thread_participant,
            relevance,
        });
    }

    drop(cache); // Release the lock

    // Sort by relevance (descending)
    results.sort_by(|a, b| b.relevance.cmp(&a.relevance));

    // Limit results
    results.truncate(limit);

    log::debug!("Cached profile search for '{}' returned {} results", query, results.len());
    results
}

/// Search profiles by query string (async, includes relay queries)
///
/// Searches through:
/// 1. User's contact list (prioritized)
/// 2. Cached profiles from PROFILE_CACHE
/// 3. Optionally queries relays if query_relays is true
///
/// Matches on `name` and `display_name` fields (case-insensitive)
/// Returns up to `limit` results sorted by relevance
pub async fn search_profiles(
    query: &str,
    limit: usize,
    query_relays: bool,
) -> Result<Vec<ProfileSearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    // Get the Nostr client
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };

    // Fetch contact list
    let contact_pubkeys = match client.get_contact_list_public_keys(Duration::from_secs(5)).await {
        Ok(pubkeys) => {
            log::debug!("Found {} contacts", pubkeys.len());
            pubkeys
        }
        Err(e) => {
            log::warn!("Failed to fetch contact list: {}", e);
            Vec::new()
        }
    };

    // Search cached profiles first (no thread participants for general search)
    let mut results = search_cached_profiles(query, limit, &contact_pubkeys, &[]);

    // Query relays for additional profiles if requested and query is long enough
    if query_relays && query.len() >= 3 && results.len() < limit {
        let query_lower = query.to_lowercase();
        log::debug!("Querying relays for profiles matching: {}", query);

        // Try NIP-50 search first
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .search(query)
            .limit(20);

        match client.fetch_events(filter, Duration::from_secs(3)).await {
            Ok(events) => {
                log::debug!("Found {} metadata events from relays", events.len());

                for event in events {
                    // Parse metadata
                    if let Ok(metadata) = Metadata::from_json(&event.content) {
                        let pubkey = event.pubkey;

                        // Skip if already in results
                        if results.iter().any(|r| r.pubkey == pubkey) {
                            continue;
                        }

                        // Check if matches query
                        let name_match = metadata.name.as_ref()
                            .map(|n| n.to_lowercase().contains(&query_lower))
                            .unwrap_or(false);

                        let display_name_match = metadata.display_name.as_ref()
                            .map(|d| d.to_lowercase().contains(&query_lower))
                            .unwrap_or(false);

                        if !name_match && !display_name_match {
                            continue;
                        }

                        let is_contact = contact_pubkeys.contains(&pubkey);
                        let is_thread_participant = false; // Relay results won't know about thread context
                        let mut relevance = if is_contact { 1000 } else { 10 };

                        // Calculate relevance (lower than cached results)
                        if let Some(name) = &metadata.name {
                            if name.to_lowercase() == query_lower {
                                relevance += 200;
                            } else if name.to_lowercase().starts_with(&query_lower) {
                                relevance += 50;
                            } else {
                                relevance += 20;
                            }
                        }

                        results.push(ProfileSearchResult {
                            pubkey,
                            name: metadata.name.clone(),
                            display_name: metadata.display_name.clone(),
                            picture: metadata.picture.clone(),
                            nip05: metadata.nip05.clone(),
                            is_contact,
                            is_thread_participant,
                            relevance,
                        });
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to query relays for profiles: {}", e);
            }
        }
    }

    // Sort by relevance (descending)
    results.sort_by(|a, b| b.relevance.cmp(&a.relevance));

    // Limit results
    results.truncate(limit);

    log::debug!("Profile search for '{}' returned {} results", query, results.len());
    Ok(results)
}

/// Get contact list public keys
pub async fn get_contact_pubkeys() -> Vec<PublicKey> {
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Vec::new(),
    };

    match client.get_contact_list_public_keys(Duration::from_secs(5)).await {
        Ok(pubkeys) => pubkeys,
        Err(e) => {
            log::warn!("Failed to fetch contact list: {}", e);
            Vec::new()
        }
    }
}

/// Get the user's relay URLs for creating nprofile mentions
#[allow(dead_code)]
pub async fn get_user_relays() -> Vec<String> {
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return get_default_relays(),
    };

    // Get connected relays from the pool
    let relays = client.pool().relays().await;
    let relay_urls: Vec<String> = relays
        .into_iter()
        .map(|(url, _)| url.to_string())
        .take(3) // Limit to 3 relay hints
        .collect();

    if relay_urls.is_empty() {
        get_default_relays()
    } else {
        relay_urls
    }
}

/// Get default relay URLs
#[allow(dead_code)]
fn get_default_relays() -> Vec<String> {
    vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
        "wss://relay.snort.social".to_string(),
    ]
}
