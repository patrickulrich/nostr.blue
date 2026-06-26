use crate::stores::nostr_client;
use crate::utils::format::truncate_pubkey;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use lru::LruCache;
use nostr_sdk::{Event, Filter, FromBech32, Kind, Metadata, PublicKey};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::time::Duration;
/// Birthday information per NIP-24
/// Each field is optional to allow partial dates
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Birthday {
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
}
impl Birthday {
    /// Format birthday for display (e.g., "January 15" or "January 15, 1990")
    #[allow(dead_code)]
    pub fn format_display(&self) -> Option<String> {
        let months = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        match (self.month, self.day, self.year) {
            (Some(m), Some(d), Some(y)) if (1..=12).contains(&m) => {
                Some(format!("{} {}, {}", months[(m - 1) as usize], d, y))
            }
            (Some(m), Some(d), None) if (1..=12).contains(&m) => {
                Some(format!("{} {}", months[(m - 1) as usize], d))
            }
            (Some(m), None, Some(y)) if (1..=12).contains(&m) => {
                Some(format!("{} {}", months[(m - 1) as usize], y))
            }
            (Some(m), None, None) if (1..=12).contains(&m) => {
                Some(months[(m - 1) as usize].to_string())
            }
            (None, None, Some(y)) => Some(y.to_string()),
            _ => None,
        }
    }
}
/// User profile metadata from Kind 0 events
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub pubkey: String,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub banner: Option<String>,
    pub nip05: Option<String>,
    pub lud16: Option<String>,
    pub lud06: Option<String>,
    pub website: Option<String>,
    /// Whether this account is a bot (NIP-24)
    pub bot: Option<bool>,
    /// Birthday information (NIP-24)
    pub birthday: Option<Birthday>,
    pub fetched_at: DateTime<Utc>,
    /// Raw metadata JSON for preserving unknown fields during updates
    /// This prevents loss of custom metadata fields when updating profile picture/banner
    pub raw_metadata_json: Option<String>,
}
impl Profile {
    /// Get the display name, falling back to name or truncated pubkey
    pub fn get_display_name(&self) -> String {
        if let Some(display_name) = &self.display_name {
            if !display_name.trim().is_empty() {
                return display_name.clone();
            }
        }
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }
        truncate_pubkey(&self.pubkey)
    }
    /// Read the market-spec `payment_preference` from the kind-0 metadata content
    /// (`manual` | `ecash` | `lud16`). Returns None when unset (defaults to `manual`).
    pub fn payment_preference(&self) -> Option<String> {
        let json = self.raw_metadata_json.as_ref()?;
        let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
        parsed
            .get("payment_preference")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
    }
    /// Get the avatar URL, with Dicebear fallback
    pub fn get_avatar_url(&self) -> String {
        if let Some(picture) = &self.picture {
            if !picture.trim().is_empty() {
                if picture.starts_with("https://") {
                    return picture.clone();
                } else if let Some(stripped) = picture.strip_prefix("http://") {
                    return format!("https://{}", stripped);
                }
            }
        }
        format!(
            "https://api.dicebear.com/7.x/identicon/svg?seed={}",
            self.pubkey
        )
    }
    /// Get initials for avatar placeholder (first char of pubkey)
    #[allow(dead_code)]
    pub fn get_initials(&self) -> String {
        if let Some(name) = &self.display_name.as_ref().or(self.name.as_ref()) {
            let words: Vec<&str> = name.split_whitespace().collect();
            if words.len() >= 2 {
                let first = words[0].chars().next().unwrap_or('?');
                let second = words[1].chars().next().unwrap_or('?');
                return format!("{}{}", first, second).to_uppercase();
            } else if !words.is_empty() {
                return words[0]
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
            }
        }
        self.pubkey
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string()
    }
}
/// Global signal to cache profiles (pubkey -> Profile)
/// LRU cache with max capacity of 5000 profiles to prevent unbounded memory growth
/// Increased from 1000 to better serve power users who follow many accounts
pub static PROFILE_CACHE: GlobalSignal<LruCache<String, Profile>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(5000).unwrap()));
/// Bumped every time a profile is inserted into `PROFILE_CACHE` (or a
/// background batch completes). Components drive their `use_memo` re-runs off
/// this signal so they react to cache mutations without polling or spawn
/// chains. See `note_card.rs` for the consumer pattern.
pub static PROFILE_CACHE_VERSION: GlobalSignal<u64> = Signal::global(|| 0);
/// Pending pubkeys requested by components (e.g. NoteCards) that don't yet
/// have a cached profile. Drained on a 200ms debounce by a top-level effect
/// in the app shell, which calls `fetch_profiles_batch_native` once for the
/// whole batch. This collapses the per-NoteCard N+1 REQ pattern into a single
/// batched REQ.
pub static PROFILE_REQUEST_QUEUE: GlobalSignal<HashSet<String>> =
    Signal::global(HashSet::new);
/// Cooldown after which an exhausted pubkey becomes eligible for retry again.
const PROFILE_EXHAUSTED_COOLDOWN: Duration = Duration::from_secs(300); // 5 min
/// Max indexer-fetch attempts before a pubkey is considered exhausted.
const PROFILE_EXHAUSTED_MAX_ATTEMPTS: u8 = 2;
/// Pubkeys whose metadata fetch returned no event from the indexer relays.
/// Maps `pubkey -> (attempts, last_attempt)`. After
/// `PROFILE_EXHAUSTED_MAX_ATTEMPTS` attempts a pubkey is skipped by
/// `queue_profile_request` until `PROFILE_EXHAUSTED_COOLDOWN` elapses, so we
/// don't hammer the indexers for pubkeys that genuinely have no kind 0 (and
/// still retry later in case of a race or a late publish). Mirrors Wisp's
/// `exhaustedProfiles` dead-list.
pub static PROFILE_EXHAUSTED: GlobalSignal<HashMap<String, (u8, instant::Instant)>> =
    Signal::global(HashMap::new);
/// The most recent set of feed-author pubkeys, updated by
/// `prefetch_author_metadata` after each feed page load. Used by the
/// periodic profile sweep (`start_profile_sweep`) as a safety net to
/// re-enqueue any pubkeys whose metadata is still missing — catching
/// profiles that were missed by the event-driven queue due to races,
/// timeouts, or component unmounts. Modelled after Wisp's
/// `sweepMissingProfiles` which iterates the full feed state.
pub static RECENT_FEED_PUBKEYS: GlobalSignal<HashSet<String>> = Signal::global(HashSet::new);
/// Default timeout for kind 0 metadata REQs. 10s accounts for cold WASM
/// starts where indexer TLS handshakes take 3-5s each, and for large batch
/// chunks (200 authors) where the indexer needs time to process. Wisp uses
/// 15s for EOSE waits; 10s is a reasonable middle ground.
const PROFILE_FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Increment the cache version. Callers should invoke this after any insert
/// into `PROFILE_CACHE` so memoized readers re-evaluate. Uses `with_mut` to
/// avoid the RHS-then-LHS borrow-aliasing panic on
/// `dioxus-signals-0.7.9/src/global/mod.rs:100` that occurs when `.peek()`
/// and `.write()` overlap on the same signal.
pub fn bump_cache_version() {
    PROFILE_CACHE_VERSION.with_mut(|v| *v = v.wrapping_add(1));
}
/// Returns true if a pubkey is within its exhaustion cooldown (too many
/// failed indexer fetches recently) and should not be re-queued.
pub fn is_profile_exhausted(pubkey: &str) -> bool {
    if let Some((attempts, last)) = PROFILE_EXHAUSTED.peek().get(pubkey) {
        if *attempts >= PROFILE_EXHAUSTED_MAX_ATTEMPTS
            && last.elapsed() < PROFILE_EXHAUSTED_COOLDOWN
        {
            return true;
        }
    }
    false
}
/// Bump the exhaustion counter for pubkeys whose metadata was not returned by
/// the indexers. Clears the entry for pubkeys that *were* found.
fn update_exhaustion(found: &HashSet<String>, not_found: impl Iterator<Item = String>) {
    let mut exh = PROFILE_EXHAUSTED.write();
    let now = instant::Instant::now();
    for pk in found {
        exh.remove(pk);
    }
    for pk in not_found {
        let entry = exh.entry(pk).or_insert((0, now));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = now;
    }
}
/// Enqueue a pubkey for batched metadata fetching. Bumps the cache version
/// so the app-shell drain effect fires. Skips pubkeys that are within their
/// exhaustion cooldown (recent repeated indexer misses) to avoid hammering the
/// indexers for pubkeys that genuinely have no kind 0.
pub fn queue_profile_request(pubkey: String) {
    if is_profile_exhausted(&pubkey) {
        return;
    }
    let mut q = PROFILE_REQUEST_QUEUE.write();
    if q.insert(pubkey) {
        drop(q);
        bump_cache_version();
    }
}
/// Drain pending requests, fetching missing profiles in a single batched REQ.
/// Called by the app-shell `use_effect` (see `src/routes/mod.rs`). Safe to
/// call from anywhere; an empty queue is a no-op.
pub async fn drain_profile_queue() {
    let pending: HashSet<String> = std::mem::take(&mut *PROFILE_REQUEST_QUEUE.write());
    if pending.is_empty() {
        return;
    }
    let mut pubkeys: HashSet<PublicKey> = HashSet::new();
    for pk_str in &pending {
        if let Ok(pk) = PublicKey::from_bech32(pk_str).or_else(|_| PublicKey::from_hex(pk_str)) {
            pubkeys.insert(pk);
        }
    }
    if pubkeys.is_empty() {
        return;
    }
    if let Err(e) = fetch_profiles_batch_native(pubkeys).await {
        log::warn!("drain_profile_queue batch fetch failed: {e}");
    }
    bump_cache_version();
}
/// Cache TTL in seconds (24 hours)
/// Increased from 5 minutes to reduce network requests for stable profile data
pub(crate) const CACHE_TTL_SECONDS: i64 = 24 * 60 * 60;
/// Convert a Profile to nostr_sdk Metadata, populating all known fields.
pub fn profile_to_metadata(profile: &Profile) -> nostr_sdk::Metadata {
    let mut metadata = nostr_sdk::Metadata::new();
    if let Some(name) = &profile.name {
        metadata = metadata.name(name);
    }
    if let Some(display_name) = &profile.display_name {
        metadata = metadata.display_name(display_name);
    }
    if let Some(about) = &profile.about {
        metadata = metadata.about(about);
    }
    if let Some(picture) = &profile.picture {
        if let Ok(url) = nostr_sdk::Url::parse(picture) {
            metadata = metadata.picture(url);
        }
    }
    if let Some(banner) = &profile.banner {
        if let Ok(url) = nostr_sdk::Url::parse(banner) {
            metadata = metadata.banner(url);
        }
    }
    if let Some(website) = &profile.website {
        if let Ok(url) = nostr_sdk::Url::parse(website) {
            metadata = metadata.website(url);
        }
    }
    if let Some(nip05) = &profile.nip05 {
        metadata = metadata.nip05(nip05);
    }
    if let Some(lud16) = &profile.lud16 {
        metadata = metadata.lud16(lud16);
    }
    metadata
}

/// Get a profile from cache only (synchronous)
pub fn get_profile(pubkey: &str) -> Option<nostr_sdk::Metadata> {
    PROFILE_CACHE
        .peek()
        .peek(pubkey)
        .map(profile_to_metadata)
}
/// Fetch a profile from relays by pubkey
/// Returns cached profile immediately if available (even if stale),
/// and spawns a background refresh if stale
pub async fn fetch_profile(pubkey: String) -> Result<Profile, String> {
    if let Some(cached_profile) = PROFILE_CACHE.read().peek(&pubkey) {
        let age = Utc::now().signed_duration_since(cached_profile.fetched_at);
        if age.num_seconds() < CACHE_TTL_SECONDS {
            log::debug!("Using cached profile for {}", pubkey);
            return Ok(cached_profile.clone());
        }
        log::debug!("Profile {} is stale, refreshing in background", pubkey);
        let pk = pubkey.clone();
        let cached = cached_profile.clone();
        spawn(async move {
            let _ = fetch_profile_from_relays(&pk).await;
        });
        return Ok(cached);
    }
    fetch_profile_from_relays(&pubkey).await
}
/// Internal function to fetch profile and update cache.
///
/// Query path: SDK database (local) → indexer relays. We skip the general
/// relay step (`fetch_events_aggregated` → `client.fetch_events`) because
/// general/user relays frequently don't have kind 0 for arbitrary pubkeys,
/// and waiting for the empty response wastes up to `PROFILE_FETCH_TIMEOUT`
/// before the indexer fallback fires. The SDK database already ingests
/// metadata from all active subscriptions, so anything the general relays
/// would have returned is already in the DB. Indexers aggregate everyone's
/// metadata, making them the correct source for DB misses.
async fn fetch_profile_from_relays(pubkey: &str) -> Result<Profile, String> {
    log::info!("Fetching profile from database/indexers for {}", pubkey);
    let public_key = PublicKey::from_bech32(pubkey)
        .or_else(|_| PublicKey::from_hex(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    // 1. Check the SDK local database first (instant, no network).
    if let Some(client) = nostr_client::get_client() {
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .author(public_key)
            .limit(1);
        match client.database().query(filter).await {
            Ok(db_events) => {
                if let Some(event) = db_events.into_iter().next() {
                    let profile = parse_profile_event(&event)?;
                    PROFILE_CACHE
                        .write()
                        .put(pubkey.to_string(), profile.clone());
                    bump_cache_version();
                    return Ok(profile);
                }
            }
            Err(e) => {
                log::warn!("Database query failed for profile {}: {}", pubkey, e);
            }
        }
    }
    // 2. DB miss → go straight to indexer relays (skip general relay fetch).
    fetch_profile_from_indexers(pubkey, public_key).await
}
async fn fetch_profile_from_indexers(
    pubkey: &str,
    public_key: PublicKey,
) -> Result<Profile, String> {
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => return Ok(empty_profile(pubkey)),
    };
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .author(public_key)
        .limit(1);
    // Delegate to the centralized indexer helper. Indexers are DISCOVERY-only
    // and `can_read()` includes DISCOVERY, so `fetch_events_from` works without
    // needing a READ flag (and without polluting broadcast subscriptions).
    match crate::stores::relay::nip65::fetch_events_from_indexers(
        &client,
        filter,
        PROFILE_FETCH_TIMEOUT,
    )
    .await
    {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let profile = parse_profile_event(&event)?;
                PROFILE_CACHE
                    .write()
                    .put(pubkey.to_string(), profile.clone());
                bump_cache_version();
                log::info!("Fetched profile for {} from indexer relays", pubkey);
                Ok(profile)
            } else {
                Ok(empty_profile(pubkey))
            }
        }
        Err(e) => {
            log::warn!("Indexer profile fetch also failed for {}: {}", pubkey, e);
            Ok(empty_profile(pubkey))
        }
    }
}
fn empty_profile(pubkey: &str) -> Profile {
    Profile {
        pubkey: pubkey.to_string(),
        name: None,
        display_name: None,
        about: None,
        picture: None,
        banner: None,
        nip05: None,
        lud16: None,
        lud06: None,
        website: None,
        bot: None,
        birthday: None,
        fetched_at: Utc::now(),
        raw_metadata_json: None,
    }
}
/// Parse a Kind 0 event into a Profile struct
pub fn parse_profile_event(event: &Event) -> Result<Profile, String> {
    let content = &event.content;
    let metadata: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse metadata JSON: {}", e))?;
    let bot = metadata.get("bot").and_then(|v| {
        if let Some(b) = v.as_bool() {
            Some(b)
        } else if let Some(s) = v.as_str() {
            match s.to_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            }
        } else {
            None
        }
    });
    let birthday = metadata.get("birthday").and_then(|v| {
        if v.is_object() {
            let year = v.get("year").and_then(|y| y.as_u64()).map(|y| y as u16);
            let month = v.get("month").and_then(|m| m.as_u64()).map(|m| m as u8);
            let day = v.get("day").and_then(|d| d.as_u64()).map(|d| d as u8);
            if year.is_some() || month.is_some() || day.is_some() {
                Some(Birthday { year, month, day })
            } else {
                None
            }
        } else {
            None
        }
    });
    Ok(Profile {
        pubkey: event.pubkey.to_string(),
        name: metadata
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        display_name: metadata
            .get("display_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        about: metadata
            .get("about")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        picture: metadata
            .get("picture")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        banner: metadata
            .get("banner")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        nip05: metadata
            .get("nip05")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        lud16: metadata
            .get("lud16")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        lud06: metadata
            .get("lud06")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        website: metadata
            .get("website")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        bot,
        birthday,
        fetched_at: Utc::now(),
        raw_metadata_json: Some(content.clone()),
    })
}
/// Convert a nostr_sdk `Metadata` into a `Profile`.
/// Always returns a Profile (even with no name/display_name) so it can be
/// cached and avoid repeated fetch attempts.
pub fn metadata_to_profile(pubkey: String, metadata: &Metadata) -> Profile {
    let name = metadata
        .name
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();
    let display_name = metadata
        .display_name
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned();
    let bot = metadata.custom.get("bot").and_then(|v| {
        if let Some(b) = v.as_bool() {
            Some(b)
        } else if let Some(s) = v.as_str() {
            match s.to_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            }
        } else {
            None
        }
    });
    let birthday = metadata.custom.get("birthday").and_then(|v| {
        if v.is_object() {
            let year = v.get("year").and_then(|y| y.as_u64()).map(|y| y as u16);
            let month = v.get("month").and_then(|m| m.as_u64()).map(|m| m as u8);
            let day = v.get("day").and_then(|d| d.as_u64()).map(|d| d as u8);
            if year.is_some() || month.is_some() || day.is_some() {
                Some(Birthday { year, month, day })
            } else {
                None
            }
        } else {
            None
        }
    });
    Profile {
        pubkey,
        name,
        display_name,
        about: metadata
            .about
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        picture: metadata
            .picture
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        banner: metadata
            .banner
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        nip05: metadata
            .nip05
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        lud16: metadata
            .lud16
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        lud06: metadata
            .lud06
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        website: metadata
            .website
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned(),
        bot,
        birthday,
        fetched_at: Utc::now(),
        raw_metadata_json: None,
    }
}
/// Get a profile from cache (if available)
pub fn get_cached_profile(pubkey: &str) -> Option<Profile> {
    PROFILE_CACHE.read().peek(pubkey).cloned()
}
/// Fetch multiple profiles in a single query (much more efficient than individual fetches)
#[allow(dead_code)]
pub async fn fetch_profiles_batch(
    pubkeys: Vec<String>,
) -> Result<HashMap<String, Profile>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }
    let mut results = HashMap::new();
    let mut missing = Vec::new();
    for pk in &pubkeys {
        if let Some(cached) = PROFILE_CACHE.read().peek(pk) {
            let age = Utc::now().signed_duration_since(cached.fetched_at);
            if age.num_seconds() < CACHE_TTL_SECONDS {
                results.insert(pk.clone(), cached.clone());
                continue;
            }
        }
        missing.push(pk.clone());
    }
    if missing.is_empty() {
        return Ok(results);
    }
    log::info!("Batch fetching {} profiles", missing.len());
    let authors: Vec<PublicKey> = missing
        .iter()
        .filter_map(|pk| {
            PublicKey::from_bech32(pk)
                .or_else(|_| PublicKey::from_hex(pk))
                .ok()
        })
        .collect();
    if authors.is_empty() {
        return Ok(results);
    }
    let filter = Filter::new().kind(Kind::Metadata).authors(authors);
    match nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut inserted = 0u32;
            for event in events {
                if let Ok(profile) = parse_profile_event(&event) {
                    PROFILE_CACHE
                        .write()
                        .put(profile.pubkey.clone(), profile.clone());
                    results.insert(profile.pubkey.clone(), profile);
                    inserted += 1;
                }
            }
            if inserted > 0 {
                bump_cache_version();
            }
            Ok(results)
        }
        Err(e) => {
            log::error!("Failed to batch fetch profiles: {}", e);
            Err(format!("Failed to batch fetch profiles: {}", e))
        }
    }
}
/// Prefetch multiple profiles (useful for loading conversation lists)
#[allow(dead_code)]
pub async fn prefetch_profiles(pubkeys: Vec<String>) {
    for pubkey in pubkeys {
        spawn(async move {
            let _ = fetch_profile(pubkey).await;
        });
    }
}
/// Optimized batch profile fetcher that works with PublicKey directly
///
/// This function is optimized to:
/// 1. Work with PublicKey natively (no string conversions)
/// 2. Use single lock for cache lookups
/// 3. Query database directly before hitting relays
/// 4. Only fetch from relays what's truly missing
pub async fn fetch_profiles_batch_native(
    pubkeys: HashSet<PublicKey>,
) -> Result<HashMap<PublicKey, Profile>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }
    let mut results = HashMap::new();
    let mut missing = Vec::new();
    {
        let cache = PROFILE_CACHE.read();
        for &pk in &pubkeys {
            let pk_str = pk.to_string();
            if let Some(cached) = cache.peek(&pk_str) {
                let age = Utc::now().signed_duration_since(cached.fetched_at);
                if age.num_seconds() < CACHE_TTL_SECONDS {
                    results.insert(pk, cached.clone());
                    continue;
                }
            }
            missing.push(pk);
        }
    }
    if missing.is_empty() {
        return Ok(results);
    }
    log::info!("Batch fetching {} profiles (optimized path)", missing.len());
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .authors(missing.iter().copied());
    match client.database().query(filter).await {
        Ok(database_events) => {
            for event in database_events {
                if let Ok(profile) = parse_profile_event(&event) {
                    let pk = event.pubkey;
                    PROFILE_CACHE
                        .write()
                        .put(profile.pubkey.clone(), profile.clone());
                    results.insert(pk, profile);
                }
            }
        }
        Err(e) => {
            log::warn!(
                "Database batch query failed: {}, will query relays for all",
                e
            );
        }
    }
    let found_pubkeys: HashSet<PublicKey> = results.keys().copied().collect();
    let still_missing: Vec<PublicKey> = missing
        .into_iter()
        .filter(|pk| !found_pubkeys.contains(pk))
        .collect();
    if !still_missing.is_empty() {
        log::info!(
            "Querying indexer relays for {} profiles not in database",
            still_missing.len()
        );
        let mut inserted = 0u32;
        let mut found_hex: HashSet<String> = HashSet::new();
        // Pubkeys whose chunk ERRORED (indexers not yet connected, network
        // failure, etc.). These must NOT be marked exhausted — exhaustion is
        // only for pubkeys a *successful* fetch confirmed have no kind 0.
        // Marking an error as exhaustion would suppress retries on a cold
        // start where indexers aren't connected yet.
        let mut errored_hex: HashSet<String> = HashSet::new();
        // Chunk authors to stay well under relay truncation limits. 200
        // matches Wisp's `MAX_AUTHORS_PER_FILTER` ceiling.
        for chunk in still_missing.chunks(200) {
            if chunk.is_empty() {
                continue;
            }
            // Fetch kind 0 (metadata) + kind 10002 (relay list) + kind 10050
            // (DM inbox) in one REQ. Indexers are profile-directory relays
            // that store exactly these kinds; we route through the dedicated
            // `fetch_events_from_indexers` helper because indexer relays are
            // DISCOVERY-only and invisible to `client.fetch_events()` (which
            // targets only READ-flagged relays).
            let filter = Filter::new()
                .kinds([Kind::Metadata, Kind::RelayList, Kind::InboxRelays])
                .authors(chunk.iter().copied());
            match crate::stores::relay::nip65::fetch_events_from_indexers(
                &client,
                filter,
                PROFILE_FETCH_TIMEOUT,
            )
            .await
            {
                Ok(events) => {
                    for event in events {
                        match event.kind {
                            Kind::Metadata => {
                                if let Ok(profile) = parse_profile_event(&event) {
                                    let pk = event.pubkey;
                                    found_hex.insert(profile.pubkey.clone());
                                    PROFILE_CACHE
                                        .write()
                                        .put(profile.pubkey.clone(), profile.clone());
                                    results.insert(pk, profile);
                                    inserted += 1;
                                }
                            }
                            // Build the outbox coverage map from kind 10002 so
                            // future fetches can route to each author's write
                            // relays. Kind 10050 (DM inbox) is cached in the
                            // SDK database for later DM addressing.
                            Kind::RelayList => {
                                crate::stores::relay::coverage::record_relay_list_from_event(
                                    &event,
                                );
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Indexer batch profile fetch failed for chunk: {e}");
                    // Record these as errored (retryable), NOT exhausted.
                    for pk in chunk {
                        errored_hex.insert(pk.to_string());
                    }
                }
            }
        }
        // Mark pubkeys a *successful* fetch confirmed have no metadata as
        // exhausted (retry later). Errored pubkeys and found ones are
        // excluded so transient failures stay retryable.
        let not_found = still_missing
            .iter()
            .map(|pk| pk.to_string())
            .filter(|pk| !found_hex.contains(pk) && !errored_hex.contains(pk));
        update_exhaustion(&found_hex, not_found);
        if inserted > 0 {
            bump_cache_version();
        }
    }
    Ok(results)
}

/// Safety-net sweep: re-enqueue pubkeys from the recent feed whose metadata
/// is still missing. Also clears expired entries from `PROFILE_EXHAUSTED` so
/// they become eligible for retry. Modelled after Wisp's
/// `sweepMissingProfiles` (`MetadataFetcher.kt:333-351`) which runs
/// periodically after startup to catch profiles missed by the event-driven
/// queue.
pub async fn sweep_profiles() {
    let feed_pubkeys = RECENT_FEED_PUBKEYS.peek().clone();
    if feed_pubkeys.is_empty() {
        return;
    }
    let mut enqueued = 0u32;
    for pk in &feed_pubkeys {
        if PROFILE_CACHE.peek().peek(pk).is_none() {
            queue_profile_request(pk.clone());
            enqueued += 1;
        }
    }
    // Clear exhausted entries whose cooldown has elapsed so they become
    // retryable for the next drain cycle.
    let now = instant::Instant::now();
    let expired: Vec<String> = PROFILE_EXHAUSTED
        .peek()
        .iter()
        .filter(|(_, (attempts, last))| {
            *attempts >= PROFILE_EXHAUSTED_MAX_ATTEMPTS
                && now.duration_since(*last) >= PROFILE_EXHAUSTED_COOLDOWN
        })
        .map(|(pk, _)| pk.clone())
        .collect();
    if !expired.is_empty() {
        let mut exh = PROFILE_EXHAUSTED.write();
        for pk in &expired {
            exh.remove(pk);
        }
        log::debug!(
            "sweep_profiles: cleared {} expired exhausted entries",
            expired.len()
        );
    }
    if enqueued > 0 {
        log::debug!("sweep_profiles: re-enqueued {} missing pubkeys", enqueued);
    }
}

/// Start the periodic profile sweep safety net. Called once from
/// `warmup_profiles_from_network` after the initial metadata backfill.
/// Schedule matches Wisp'sStartupCoordinator.kt:258-271`: eager at 5s/15s/30s,
/// then every 120s. Uses `spawn_forever` so it survives route changes.
pub fn start_profile_sweep() {
    use dioxus::prelude::spawn;
    spawn(async move {
        // Eager phase: catch profiles missed during the initial load.
        for delay in [5u64, 10, 15] {
            crate::platform::timer::sleep(Duration::from_secs(delay)).await;
            sweep_profiles().await;
        }
        // Steady state: periodic safety net.
        loop {
            crate::platform::timer::sleep(Duration::from_secs(120)).await;
            sweep_profiles().await;
        }
    });
}
