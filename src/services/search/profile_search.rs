use super::search_relays::get_connected_search_relays;
use crate::stores::nostr_client::NOSTR_CLIENT;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::nip19_urls::parse_profile_id;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Result type for profile search
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileSearchResult {
    pub pubkey: PublicKey,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
    #[allow(dead_code)]
    pub nip05: Option<String>,
    pub is_contact: bool,
    pub is_thread_participant: bool,
    pub relevance: u32,
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
        let hex = self.pubkey.to_hex();
        format!("{}...{}", &hex[..8], &hex[hex.len() - 8..])
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
    let cache = PROFILE_CACHE.read();
    for (pubkey_str, profile) in cache.iter() {
        let pubkey = match PublicKey::from_hex(pubkey_str) {
            Ok(pk) => pk,
            Err(_) => continue,
        };
        let name_match = profile
            .name
            .as_ref()
            .map(|n| n.to_lowercase().contains(&query_lower))
            .unwrap_or(false);
        let display_name_match = profile
            .display_name
            .as_ref()
            .map(|d| d.to_lowercase().contains(&query_lower))
            .unwrap_or(false);
        if !name_match && !display_name_match {
            continue;
        }
        let is_contact = contact_pubkeys.contains(&pubkey);
        let is_thread_participant = thread_pubkeys.contains(&pubkey);
        let mut relevance = 0u32;
        if is_thread_participant {
            relevance += 2000;
        } else if is_contact {
            relevance += 1000;
        }
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
    drop(cache);
    results.sort_by_key(|b| std::cmp::Reverse(b.relevance));
    results.truncate(limit);
    log::debug!(
        "Cached profile search for '{}' returned {} results",
        query,
        results.len()
    );
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
) -> std::result::Result<Vec<ProfileSearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    if let Some(pk) = parse_profile_id(query) {
        let client_opt = (*NOSTR_CLIENT.read()).clone();
        if let Some(client) = client_opt {
            if let Ok(Some(metadata)) = client
                .fetch_metadata(pk, Duration::from_secs(3))
                .await
            {
                let contact_pubkeys = client
                    .get_contact_list_public_keys(Duration::from_secs(3))
                    .await
                    .unwrap_or_default();
                return Ok(vec![ProfileSearchResult {
                    pubkey: pk,
                    name: metadata.name.clone(),
                    display_name: metadata.display_name.clone(),
                    picture: metadata.picture.clone(),
                    nip05: metadata.nip05.clone(),
                    is_contact: contact_pubkeys.contains(&pk),
                    is_thread_participant: false,
                    relevance: 10000,
                }]);
            }
        }
        return Ok(vec![ProfileSearchResult {
            pubkey: pk,
            name: None,
            display_name: None,
            picture: None,
            nip05: None,
            is_contact: false,
            is_thread_participant: false,
            relevance: 5000,
        }]);
    }

    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Err("Nostr client not initialized".to_string()),
    };
    let contact_pubkeys = match client
        .get_contact_list_public_keys(Duration::from_secs(5))
        .await
    {
        Ok(pubkeys) => {
            log::debug!("Found {} contacts", pubkeys.len());
            pubkeys
        }
        Err(e) => {
            log::warn!("Failed to fetch contact list: {}", e);
            Vec::new()
        }
    };
    let mut results = search_cached_profiles(query, limit, &contact_pubkeys, &[]);
    if query_relays && query.len() >= 3 && results.len() < limit {
        let query_lower = query.to_lowercase();
        log::debug!("Querying relays for profiles matching: {}", query);
        let filter = Filter::new().kind(Kind::Metadata).search(query).limit(20);
        let search_urls = get_connected_search_relays(&client).await;
        let fetch_result = if search_urls.is_empty() {
            client.fetch_events(filter, Duration::from_secs(3)).await
        } else {
            client
                .fetch_events_from(search_urls, filter, Duration::from_secs(3))
                .await
        };
        match fetch_result {
            Ok(events) => {
                log::debug!("Found {} metadata events from relays", events.len());
                for event in events {
                    if let Ok(metadata) = Metadata::from_json(&event.content) {
                        let pubkey = event.pubkey;
                        if results.iter().any(|r| r.pubkey == pubkey) {
                            continue;
                        }
                        let name_match = metadata
                            .name
                            .as_ref()
                            .map(|n| n.to_lowercase().contains(&query_lower))
                            .unwrap_or(false);
                        let display_name_match = metadata
                            .display_name
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&query_lower))
                            .unwrap_or(false);
                        if !name_match && !display_name_match {
                            continue;
                        }
                        let is_contact = contact_pubkeys.contains(&pubkey);
                        let is_thread_participant = false;
                        let mut relevance = if is_contact { 1000 } else { 10 };
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
    results.sort_by_key(|b| std::cmp::Reverse(b.relevance));
    results.truncate(limit);
    log::debug!(
        "Profile search for '{}' returned {} results",
        query,
        results.len()
    );
    Ok(results)
}
/// Get contact list public keys
pub async fn get_contact_pubkeys() -> Vec<PublicKey> {
    let client_opt = (*NOSTR_CLIENT.read()).clone();
    let client = match client_opt {
        Some(c) => c,
        None => return Vec::new(),
    };
    if let Some(pubkey_str) = crate::stores::auth_store::get_pubkey() {
        if let Ok(pk) = PublicKey::from_hex(&pubkey_str) {
            if let Ok(pubkeys) = client.database().contacts_public_keys(pk).await {
                if !pubkeys.is_empty() {
                    log::debug!(
                        "Loaded {} contact pubkeys from SDK database",
                        pubkeys.len()
                    );
                    return pubkeys.into_iter().collect();
                }
            }
        }
    }
    match client
        .get_contact_list_public_keys(Duration::from_secs(5))
        .await
    {
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
    let relays = client.pool().relays().await;
    let relay_urls: Vec<String> = relays
        .into_keys()
        .map(|url| url.to_string())
        .take(3)
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
