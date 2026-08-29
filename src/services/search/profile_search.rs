use super::search_relays::get_connected_search_relays;
use crate::stores::nostr_client::NOSTR_CLIENT;
use crate::stores::profiles::{
    cache_profile, cache_profile_search_result, get_cached_profile, PROFILE_CACHE,
};
use crate::utils::nip19_urls::parse_profile_id;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

/// Result type for profile search
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileSearchResult {
    pub pubkey: PublicKey,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub picture: Option<String>,
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

/// Successful NIP-05 address resolution: the resolved profile plus any relay
/// hints advertised in the user's nostr.json document.
#[derive(Clone, Debug)]
pub struct Nip05Resolution {
    pub result: ProfileSearchResult,
    pub relays: Vec<RelayUrl>,
}

/// Minimum length for a query fragment to be treated as a hex pubkey prefix.
const HEX_PREFIX_MIN_LEN: usize = 4;

/// True when `query` (lowercased) looks like a hex pubkey fragment.
fn is_hex_fragment(query: &str) -> bool {
    query.len() >= HEX_PREFIX_MIN_LEN
        && query
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Score a profile against `query` (already lowercased). Returns 0 when the
/// profile does not match at all.
///
/// Ranking tiers (higher wins):
/// - exact `name` (500) / exact NIP-05 (450) / exact `display_name` (400)
/// - `name` prefix (300) / `display_name` prefix (280) / NIP-05 prefix (250)
/// - `name` contains (200) / `display_name` contains (180) / NIP-05 contains (150)
/// - hex pubkey prefix (100)
fn score_profile(
    query: &str,
    name: Option<&str>,
    display_name: Option<&str>,
    nip05: Option<&str>,
    pubkey_hex: &str,
) -> u32 {
    let mut score = 0u32;
    let field_score = |field: Option<&str>, exact: u32, prefix: u32, contains: u32| -> u32 {
        let Some(value) = field else {
            return 0;
        };
        let lower = value.to_lowercase();
        if lower == query {
            exact
        } else if lower.starts_with(query) {
            prefix
        } else if lower.contains(query) {
            contains
        } else {
            0
        }
    };
    score = score.max(field_score(name, 500, 300, 200));
    score = score.max(field_score(display_name, 400, 280, 180));
    // NIP-05 matching only for non-hex human queries (a hex fragment would
    // otherwise trivially "contain"-match the @domain of unrelated users).
    if !is_hex_fragment(query) {
        score = score.max(field_score(nip05, 450, 250, 150));
    }
    if is_hex_fragment(query) && pubkey_hex.starts_with(query) {
        score = score.max(100);
    }
    score
}

/// Deterministic tiebreak key: relevance desc, then display/name, then pubkey.
fn result_sort_key(result: &ProfileSearchResult) -> (std::cmp::Reverse<u32>, String, String) {
    (
        std::cmp::Reverse(result.relevance),
        result.get_display_name().to_lowercase(),
        result.pubkey.to_hex(),
    )
}

/// Search cached profiles synchronously (fast, no relay queries)
///
/// Searches through `PROFILE_CACHE`, matching on `name`, `display_name`,
/// `nip05` (case-insensitive) and hex pubkey prefixes. Thread participants
/// and contacts are boosted in ranking.
/// Returns up to `limit` results sorted by relevance with deterministic tiebreaks.
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
    let contacts: HashSet<String> = contact_pubkeys.iter().map(|p| p.to_hex()).collect();
    let thread: HashSet<String> = thread_pubkeys.iter().map(|p| p.to_hex()).collect();
    let mut results: Vec<ProfileSearchResult> = Vec::new();
    let cache = PROFILE_CACHE.read();
    for (pubkey_str, profile) in cache.iter() {
        let base = score_profile(
            &query_lower,
            profile.name.as_deref(),
            profile.display_name.as_deref(),
            profile.nip05.as_deref(),
            pubkey_str,
        );
        if base == 0 {
            continue;
        }
        let is_contact = contacts.contains(pubkey_str);
        let is_thread_participant = thread.contains(pubkey_str);
        let mut relevance = base;
        if is_thread_participant {
            relevance += 2000;
        } else if is_contact {
            relevance += 1000;
        }
        let Ok(pubkey) = PublicKey::from_hex(pubkey_str) else {
            continue;
        };
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
    results.sort_by_key(result_sort_key);
    results.truncate(limit);
    log::debug!(
        "Cached profile search for '{}' returned {} results",
        query,
        results.len()
    );
    results
}

/// Resolve a full NIP-05 address (`user@domain` — the caller MUST enforce the
/// `@` separator, since `Nip05Address::parse` coerces bare domains to `_@domain`)
/// via the domain's well-known nostr.json document.
///
/// On success the resolved profile is written into `PROFILE_CACHE` (minimal
/// metadata) so subsequent cache scans match it.
pub async fn resolve_nip05_profile(
    address: &str,
) -> std::result::Result<Option<Nip05Resolution>, String> {
    let parsed = Nip05Address::parse(address).map_err(|e| e.to_string())?;
    let url = parsed.url().to_string();

    let client = crate::platform::http::http_client().map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("NIP-05 fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body = resp
        .text()
        .await
        .map_err(|e| format!("NIP-05 body read failed: {e}"))?;
    let profile = Nip05Profile::from_raw_json(&parsed, &body)
        .map_err(|e| format!("NIP-05 parse failed: {e}"))?;

    let pubkey = profile.public_key;
    let name = parsed.name().to_string();
    let minimal_metadata = Metadata {
        name: Some(name.clone()),
        nip05: Some(address.to_string()),
        ..Default::default()
    };
    // Seed the cache ONLY when the pubkey has no entry — a resolved
    // name-only stub must never degrade an existing (possibly rich) profile
    // (see `should_cache_search_result` for the general rules).
    if get_cached_profile(&pubkey.to_hex()).is_none() {
        cache_profile(&pubkey.to_hex(), &minimal_metadata, None);
    }

    Ok(Some(Nip05Resolution {
        result: ProfileSearchResult {
            pubkey,
            name: Some(name),
            display_name: None,
            picture: None,
            nip05: Some(address.to_string()),
            is_contact: false,
            is_thread_participant: false,
            relevance: 10_000,
        },
        relays: profile.relays,
    }))
}

/// Build a search result from a kind 0 event, writing it into `PROFILE_CACHE`
/// (guarded: only rich results, only strictly-newer than the incumbent) so
/// subsequent cache scans (and any memo keyed on `PROFILE_CACHE_VERSION`)
/// pick it up. Returns `None` when the event content is not valid metadata.
fn ingest_metadata_event(
    event: &nostr_sdk::Event,
    contacts: &HashSet<String>,
) -> Option<ProfileSearchResult> {
    let metadata = Metadata::from_json(&event.content).ok()?;
    let pubkey_hex = event.pubkey.to_hex();
    cache_profile_search_result(
        &pubkey_hex,
        &metadata,
        Some(event.created_at.as_secs()),
    );
    Some(ProfileSearchResult {
        pubkey: event.pubkey,
        name: metadata.name.clone(),
        display_name: metadata.display_name.clone(),
        picture: metadata.picture.clone(),
        nip05: metadata.nip05.clone(),
        is_contact: contacts.contains(&pubkey_hex),
        is_thread_participant: false,
        relevance: 0,
    })
}

/// Stream NIP-50 profile search results as they arrive from the search relays.
///
/// Subscribes with auto-close (exit after `limit` events or 1200ms idle,
/// 3s hard cap) and fans each matching kind 0 event into `PROFILE_CACHE`
/// (so `use_memo`-based typeahead lists live-fill) plus the `on_event`
/// callback. Receiver is acquired *before* subscribing so no early hit is
/// missed (tokio broadcast only delivers to live receivers).
/// Returns the number of events received.
pub async fn stream_profile_search(
    query: &str,
    limit: usize,
    contacts: &[PublicKey],
    on_event: impl Fn(ProfileSearchResult),
) -> std::result::Result<usize, String> {
    let client = (*NOSTR_CLIENT.read()).clone().ok_or("Nostr client not initialized")?;
    let search_urls = get_connected_search_relays(&client).await;
    if search_urls.is_empty() {
        return Ok(0);
    }
    let contact_set: HashSet<String> = contacts.iter().map(|p| p.to_hex()).collect();

    let filter = Filter::new()
        .kind(Kind::Metadata)
        .search(query)
        .limit(limit.min(50));
    let opts = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::WaitForEvents(limit.min(u16::MAX as usize) as u16))
        .idle_timeout(Some(Duration::from_millis(1200)))
        .timeout(Some(Duration::from_secs(3)));

    let mut receiver = client.notifications();
    let output = client
        .subscribe_to(search_urls, filter, Some(opts))
        .await
        .map_err(|e| format!("Search subscribe failed: {e}"))?;
    for (url, err) in &output.failed {
        log::debug!("Search relay {url} failed to accept subscription: {err}");
    }
    let sub_id = output.val;

    let deadline = crate::platform::timer::sleep(Duration::from_millis(3500));
    tokio::pin!(deadline);
    let mut received = 0usize;
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            recv_result = receiver.recv() => {
                match recv_result {
                    Ok(RelayPoolNotification::Event { subscription_id, event, .. }) if subscription_id == sub_id => {
                        received += 1;
                        let event = *event;
                        if let Some(mut result) = ingest_metadata_event(&event, &contact_set) {
                            // Keep NIP-50 server-side matches (the relay may match
                            // fields like `about` that we do not index locally).
                            result.relevance = 10 + received as u32;
                            on_event(result);
                        }
                    }
                    Ok(RelayPoolNotification::Shutdown) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!("stream_profile_search: lagged, skipped {skipped} events");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Ok(_) => {}
                }
            }
        }
    }
    // Auto-close tears the subscription down (CLOSE + map removal); a late
    // unsubscribe here is a designed no-op.
    let _ = client.unsubscribe(&sub_id).await;
    Ok(received)
}

/// Search profiles by query string (async, includes relay queries)
///
/// Tracks, in order:
/// 1. Identifier short-circuit: npub/nprofile/hex via `parse_profile_id`
/// 2. Cached profiles from `PROFILE_CACHE`
/// 3. NIP-50 relay search on the search relays (results are written back
///    into `PROFILE_CACHE`)
///
/// Returns up to `limit` results sorted by relevance with deterministic tiebreaks.
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
                // Guarded write (no created_at → fills misses only).
                cache_profile_search_result(&pk.to_hex(), &metadata, None);
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
    let contact_pubkeys = get_contact_pubkeys().await;
    let mut results = search_cached_profiles(query, limit, &contact_pubkeys, &[]);
    if query_relays && query.chars().count() >= 3 && results.len() < limit {
        let query_lower = query.to_lowercase();
        log::debug!("Querying relays for profiles matching: {}", query);
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .search(query)
            .limit(limit.min(100));
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
                let contact_set: HashSet<String> =
                    contact_pubkeys.iter().map(|p| p.to_hex()).collect();
                for event in events.iter() {
                    if results.iter().any(|r| r.pubkey == event.pubkey) {
                        continue;
                    }
                    if let Some(mut result) = ingest_metadata_event(event, &contact_set) {
                        // The relay matched server-side (possibly on fields we do
                        // not index, e.g. `about` or NIP-05) — accept the hit even
                        // when local field matching finds nothing.
                        let local = score_profile(
                            &query_lower,
                            result.name.as_deref(),
                            result.display_name.as_deref(),
                            result.nip05.as_deref(),
                            &event.pubkey.to_hex(),
                        );
                        result.relevance = if result.is_contact {
                            1000
                        } else {
                            10
                        } + local;
                        results.push(result);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to query relays for profiles: {}", e);
            }
        }
    }
    results.sort_by_key(result_sort_key);
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

/// Get the user's relay URLs (for nprofile mention hints).
/// Caller (cashu payment requests) is feature-gated; keep on all platforms.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_hex_fragment_detection() {
        assert!(is_hex_fragment("abcd"));
        assert!(is_hex_fragment("abc123"));
        assert!(!is_hex_fragment("abc")); // too short
        assert!(!is_hex_fragment("nostr"));
        assert!(!is_hex_fragment("user@domain.com"));
    }

    #[test]
    fn score_exact_beats_prefix_beats_contains() {
        let hex = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(score_profile("bob", Some("bob"), None, None, hex), 500);
        assert_eq!(score_profile("bo", Some("bob"), None, None, hex), 300);
        assert_eq!(score_profile("ob", Some("bob"), None, None, hex), 200);
        assert_eq!(score_profile("zzz", Some("bob"), None, None, hex), 0);
    }

    #[test]
    fn score_display_name_and_nip05_tiers() {
        let hex = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        // display_name exact/prefix/contains
        assert_eq!(score_profile("zoe", None, Some("Zoe"), None, hex), 400);
        assert_eq!(score_profile("zo", None, Some("Zoe"), None, hex), 280);
        assert_eq!(score_profile("oe", None, Some("Zoe"), None, hex), 180);
        // nip05 exact (query incl. @domain)/prefix/contains
        assert_eq!(score_profile("zoe@nostr.com", None, None, Some("zoe@nostr.com"), hex), 450);
        assert_eq!(score_profile("zoe@no", None, None, Some("zoe@nostr.com"), hex), 250);
        assert_eq!(score_profile("str.com", None, None, Some("zoe@nostr.com"), hex), 150);
    }

    #[test]
    fn score_hex_prefix_matches_pubkey() {
        let hex = "abcdef1234abcdef1234abcdef1234abcdef1234abcdef1234abcdef1234abcdef";
        assert_eq!(score_profile("abcdef12", None, None, None, hex), 100);
        assert_eq!(score_profile("abcdef123", None, None, None, hex), 100);
        assert_eq!(score_profile("ffff0000", None, None, None, hex), 0);
        // hex fragments do not nip05-match the domain of unrelated users
        assert_eq!(score_profile("nostr", None, None, Some("zoe@nostr.com"), hex), 150); // "nostr" is not hex ('s','t','r' invalid) → nip05 contains applies
    }

    #[test]
    fn score_hex_fragment_does_not_nip05_match() {
        let hex = "abcdef1234abcdef1234abcdef1234abcdef1234abcdef1234abcdef1234abcdef";
        // "cafe" is a valid hex fragment; it must NOT substring-match nip05 domains
        let nip05_only = score_profile("cafe", None, None, Some("zoe@cafe.com"), hex);
        assert_eq!(nip05_only, 0);
    }

    #[test]
    fn score_is_case_insensitive() {
        let hex = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        // Callers pass an already-lowercased query; field matching is case-insensitive.
        assert_eq!(score_profile("bob", Some("Bob"), None, None, hex), 500);
        assert_eq!(score_profile("bob", Some("BOB"), None, None, hex), 500);
        assert_eq!(score_profile("bob", Some("bob"), None, None, hex), 500);
    }

    #[test]
    fn deterministic_tiebreak() {
        let key_a = |relevance: u32| -> (std::cmp::Reverse<u32>, String, String) {
            (std::cmp::Reverse(relevance), "alice".to_string(), "aa".to_string())
        };
        let key_b = |relevance: u32| -> (std::cmp::Reverse<u32>, String, String) {
            (std::cmp::Reverse(relevance), "bob".to_string(), "bb".to_string())
        };
        // same relevance → alphabetical by display name
        assert!(key_a(100) < key_b(100));
        // higher relevance wins regardless of name
        assert!(key_b(200) < key_a(100));
    }
}
