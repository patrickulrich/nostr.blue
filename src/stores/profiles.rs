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
    /// `created_at` of the kind-0 event this Profile was parsed from. Drives
    /// freshness decisions (strictly-newer replacement, 24h revalidation).
    /// `None` when unknown (e.g. built from a bare `Metadata`).
    #[allow(clippy::redundant_field_names)]
    pub event_created_at: Option<u64>,
    pub fetched_at: DateTime<Utc>,
    /// When this profile was last revalidated against
    /// indexers/outbox — stamped on BOTH outcomes (newer snapshot found AND
    /// nothing newer). Without it, `needs_revalidation` gated on the kind-0
    /// event's age (almost always >24h for infrequent posters), triggering a
    /// background refetch on every profile view forever.
    pub last_revalidated_at: Option<DateTime<Utc>>,
    /// Raw metadata JSON for preserving unknown fields during updates
    /// This prevents loss of custom metadata fields when updating profile picture/banner
    pub raw_metadata_json: Option<String>,
}
impl Profile {
    /// Display name falling back to `name`, treating empty or whitespace-only
    /// values as unset. Fields are also filtered at construction; this is the
    /// belt-and-braces accessor for `PROFILE_CACHE` consumers.
    pub fn resolved_name(&self) -> Option<String> {
        self.display_name
            .as_ref()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| self.name.as_ref().filter(|n| !n.trim().is_empty()))
            .cloned()
    }
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
    /// Whether this profile should be revalidated against indexers/outbox
    /// in the background. Two distinct axes:
    /// - **Freshness**: `event_created_at` tells whether a newer snapshot
    ///   could exist (only consulted by the strictly-newer replacement
    ///   guard, not here).
    /// - **Check throttling**: this method gates on when we last CHECKED —
    ///   `last_revalidated_at` (stamped on both outcomes) falling back to
    ///   `fetched_at` — so an infrequent poster's profile is checked at
    ///   most once per TTL instead of on every view.
    pub fn needs_revalidation(&self) -> bool {
        let last_check = self.last_revalidated_at.unwrap_or(self.fetched_at);
        Utc::now()
            .signed_duration_since(last_check)
            .num_seconds()
            .max(0)
            >= CACHE_TTL_SECONDS
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
        let resolved = self
            .display_name
            .as_ref()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| self.name.as_ref().filter(|n| !n.trim().is_empty()));
        if let Some(name) = resolved {
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
/// Cap on the per-batch outbox rescue fan-out (see
/// `fetch_profiles_batch_native`): pubkeys the indexers confirmed missing
/// are retried against their own NIP-65 write relays, at most this many per
/// batch so a large feed can't stall the drain on targeted fetches.
const MAX_OUTBOX_RESCUE: usize = 10;
/// Concurrency width for outbox-rescue targeted fetches. Wide enough to hide
/// per-relay connect latency, narrow enough to bound simultaneous ephemeral
/// relay connections (shared with any other concurrent targeted fetchers).
const OUTBOX_RESCUE_CONCURRENCY: usize = 3;
/// Timeout for each outbox-rescue targeted metadata fetch. Shorter than
/// `PROFILE_FETCH_TIMEOUT` because these are single-author, single-kind REQs.
const OUTBOX_RESCUE_TIMEOUT: Duration = Duration::from_secs(6);
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
/// Clear all profile-exhaustion entries, re-enabling fetches for every
/// suppressed pubkey. Called when the indexer state fundamentally changes —
/// indexers (re)connected after a disconnected window, or `INDEXER_RELAYS`
/// repopulated from the user's kind 10086 list — because misses recorded
/// during such windows were likely taken against the wrong or dead relay
/// set, not genuinely missing metadata (issue #374). The periodic sweep
/// (`start_profile_sweep`) re-enqueues the still-missing pubkeys.
pub fn reset_profile_exhaustion() {
    let mut exh = PROFILE_EXHAUSTED.write();
    let cleared = exh.len();
    exh.clear();
    if cleared > 0 {
        log::info!("Reset {cleared} exhausted profile(s) after indexer state change");
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
    if let Some(lud06) = &profile.lud06 {
        metadata = metadata.lud06(lud06);
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
            // `min()` = first under `Ord for Event` (descending created_at,
            // then id) — the newest snapshot regardless of arrival order.
            if let Some(event) = events.into_iter().min() {
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
        event_created_at: None,
        fetched_at: Utc::now(),
        last_revalidated_at: None,
        raw_metadata_json: None,
    }
}
/// Fold multiple kind-0 events down to the newest per author.
///
/// Indexers and the SDK database can return several versions of a
/// replaceable kind 0 (different relays holding different snapshots).
/// `PROFILE_CACHE.put` and the batch `results` map are last-write-wins, so
/// iterating arrival order would let an older version overwrite a newer one.
pub(crate) fn newest_metadata_by_author(events: Vec<Event>) -> Vec<Event> {
    let mut newest: HashMap<nostr::PublicKey, Event> = HashMap::new();
    for event in events {
        match newest.entry(event.pubkey) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(event);
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                // Newest `created_at` wins; same-second ties break on the
                // SMALLER event id, matching the SDK's `Ord for Event`
                // (descending created_at, then ascending id) so the fold is
                // a deterministic total order.
                let replace = event.created_at > slot.get().created_at
                    || (event.created_at == slot.get().created_at
                        && event.id < slot.get().id);
                if replace {
                    slot.insert(event);
                }
            }
        }
    }
    newest.into_values().collect()
}

/// Parse a Kind 0 event into a Profile struct
pub fn parse_profile_event(event: &Event) -> Result<Profile, String> {
    let content = &event.content;
    // Blank content is a valid profile wipe (the author cleared their
    // replaceable kind 0) — parse as an empty profile, not an error. A
    // `Value::Null` makes every field lookup below return `None`, matching
    // Amethyst's blank-content semantics.
    let metadata: serde_json::Value = if content.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(content).map_err(|e| format!("Failed to parse metadata JSON: {}", e))?
    };
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
        // Fall back to the camelCase `displayName` key some clients publish —
        // nostr_sdk's `Metadata` only reads snake_case and parks the camelCase
        // duplicate in `custom`, which would leave the profile nameless.
        display_name: metadata
            .get("display_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                metadata
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            })
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
        event_created_at: Some(event.created_at.as_secs()),
        fetched_at: Utc::now(),
        last_revalidated_at: None,
        raw_metadata_json: Some(content.clone()),
    })
}
/// Resolve the best display name from nostr_sdk `Metadata`, falling back to
/// `name` when `display_name` is unset.
///
/// Empty or whitespace-only values are treated as unset: some clients publish
/// `"display_name": ""`, which serde deserializes to `Some("")` and which
/// would otherwise mask the `name` field (`Option::or` only falls through on
/// `None`, not on `Some("")`).
pub fn display_name_or_name(metadata: &Metadata) -> Option<String> {
    metadata
        .display_name
        .as_ref()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| metadata.name.as_ref().filter(|n| !n.trim().is_empty()))
        .cloned()
        .or_else(|| {
            // Some clients publish only the camelCase `displayName` key,
            // which nostr_sdk parks in `custom` (snake_case fields stay
            // `None`). Fall back to it so those profiles still render a name.
            metadata
                .custom
                .get("displayName")
                .and_then(|v| v.as_str())
                .filter(|n| !n.trim().is_empty())
                .map(|n| n.to_string())
        })
}

/// Cache a profile built from nostr_sdk `Metadata` (e.g. fetched by the
/// profile viewer's indexer/outbox race) so NoteCards and repeat visits
/// share it instead of re-fetching. Also clears any exhaustion entry so the
/// pubkey is immediately retryable by the batch queue.
///
/// Mirrors the insert pattern of the batch fetchers: put, drop the write
/// guard, then bump the version (`bump_cache_version` documents the
/// `AlreadyBorrowed` hazard of overlapping the two).
pub fn cache_profile(pubkey: &str, metadata: &Metadata, event_created_at: Option<u64>) {
    let mut profile = metadata_to_profile(pubkey.to_string(), metadata);
    profile.event_created_at = event_created_at;
    PROFILE_CACHE.write().put(profile.pubkey.clone(), profile);
    PROFILE_EXHAUSTED.write().remove(pubkey);
    bump_cache_version();
}

/// Stamp `last_revalidated_at` on a cached profile after a completed
/// revalidation check that found nothing newer (the overwhelmingly common
/// outcome for infrequent posters). Without the stamp,
/// `needs_revalidation` would trigger the background indexer/outbox race
/// on every profile view. Replacements via `cache_profile` don't need
/// this — their fresh `fetched_at` already throttles.
pub fn mark_profile_revalidated(pubkey: &str) {
    let mut cache = PROFILE_CACHE.write();
    let stamped = if let Some(profile) = cache.get_mut(pubkey) {
        profile.last_revalidated_at = Some(Utc::now());
        true
    } else {
        false
    };
    drop(cache);
    if stamped {
        bump_cache_version();
    }
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
        event_created_at: None,
        fetched_at: Utc::now(),
        last_revalidated_at: None,
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
            // The DB can hold several kind-0 versions per author (it stores
            // everything it ever ingested); fold to the newest before caching
            // so an older snapshot can't overwrite a newer one.
            for event in newest_metadata_by_author(database_events.into_iter().collect()) {
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
                    // Indexers may return multiple versions per author;
                    // fold to the newest (strictly-greater created_at) before
                    // the single parse+put per author so last-write-wins
                    // caching can't regress to an older snapshot. Kind 10002 /
                    // 10050 events are partitioned out first — the fold is
                    // keyed by pubkey and would otherwise drop them.
                    let (metadata_events, other_events): (Vec<Event>, Vec<Event>) = events
                        .into_iter()
                        .partition(|event| event.kind == Kind::Metadata);
                    for event in newest_metadata_by_author(metadata_events) {
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
                    for event in other_events {
                        if event.kind == Kind::RelayList {
                            crate::stores::relay::coverage::record_relay_list_from_event(&event);
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
        // Outbox rescue: indexers are not authoritative — a pubkey the
        // indexers confirmed missing may still publish its kind 0 on its own
        // NIP-65 write relays. Retry the still-missing pubkeys there via
        // `fetch_metadata_targeted` (three-tier relay resolver + ephemeral
        // connect + targeted fetch). Bounded by MAX_OUTBOX_RESCUE per batch
        // and run concurrently at OUTBOX_RESCUE_CONCURRENCY width so a
        // sequential chain of 5-8s fetches can't stall the queue drain.
        // Errored chunks are excluded: they stay retryable via the normal
        // indexer path rather than being punished here.
        let rescue_candidates: Vec<PublicKey> = still_missing
            .iter()
            .filter(|pk| {
                let hex = pk.to_string();
                !found_hex.contains(&hex) && !errored_hex.contains(&hex)
            })
            .take(MAX_OUTBOX_RESCUE)
            .copied()
            .collect();
        if !rescue_candidates.is_empty() {
            log::info!(
                "Outbox rescue: fetching metadata for {} profiles missing from indexers",
                rescue_candidates.len()
            );
            use futures::stream::{self, StreamExt};
            let rescued: Vec<(PublicKey, Metadata, u64)> = stream::iter(rescue_candidates)
                .map(|pk| async move {
                    let hex = pk.to_hex();
                    match nostr_client::fetch_metadata_targeted(&hex, OUTBOX_RESCUE_TIMEOUT).await {
                        Ok(Some((metadata, created_at))) => Some((pk, metadata, created_at)),
                        Ok(None) => None,
                        Err(e) => {
                            log::debug!("Outbox rescue failed for {hex}: {e}");
                            None
                        }
                    }
                })
                .buffer_unordered(OUTBOX_RESCUE_CONCURRENCY)
                .filter_map(|r| async { r })
                .collect()
                .await;
            for (pk, metadata, created_at) in rescued {
                let mut profile = metadata_to_profile(pk.to_string(), &metadata);
                profile.event_created_at = Some(created_at);
                found_hex.insert(profile.pubkey.clone());
                PROFILE_CACHE.write().put(profile.pubkey.clone(), profile.clone());
                results.insert(pk, profile);
                inserted += 1;
            }
        }
        // Mark pubkeys a *successful* fetch confirmed have no metadata as
        // exhausted (retry later). Errored pubkeys and found ones are
        // excluded so transient failures stay retryable. Rescued pubkeys land
        // in `found_hex` above, so they are neither exhausted nor re-counted.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_or_name_prefers_real_display_name() {
        let metadata = Metadata::new().name("alice").display_name("Alice Q");
        assert_eq!(
            display_name_or_name(&metadata),
            Some("Alice Q".to_string())
        );
    }

    #[test]
    fn display_name_or_name_falls_through_empty_display_name() {
        // Real-world shape: some clients publish `"display_name": ""`, which
        // deserializes to `Some("")` and would otherwise mask `name`.
        let metadata = Metadata::new().name("Chiefmonkey").display_name("");
        assert_eq!(
            display_name_or_name(&metadata),
            Some("Chiefmonkey".to_string())
        );
    }

    #[test]
    fn display_name_or_name_falls_through_whitespace_display_name() {
        let metadata = Metadata::new().name("bob").display_name("   ");
        assert_eq!(display_name_or_name(&metadata), Some("bob".to_string()));
    }

    #[test]
    fn display_name_or_name_returns_none_when_both_empty() {
        let metadata = Metadata::new().name("").display_name("  ");
        assert_eq!(display_name_or_name(&metadata), None);
    }

    #[test]
    fn display_name_or_name_returns_none_for_empty_metadata() {
        assert_eq!(display_name_or_name(&Metadata::new()), None);
    }

    #[test]
    fn metadata_to_profile_filters_empty_name_fields() {
        let metadata = Metadata::new().name("alice").display_name("");
        let profile = metadata_to_profile("pk".to_string(), &metadata);
        assert_eq!(profile.display_name, None);
        assert_eq!(profile.name, Some("alice".to_string()));
        // The empty display_name must not mask the name on the Profile path.
        assert_eq!(profile.get_display_name(), "alice");
        assert_eq!(profile.event_created_at, None);
    }

    #[test]
    fn display_name_or_name_falls_back_to_camel_case_display_name() {
        // Profiles whose client publishes only the camelCase key: nostr_sdk
        // parks `displayName` in `custom`, leaving snake_case fields None.
        let metadata = Metadata::new().custom_field("displayName", "Camel Kid");
        assert_eq!(
            display_name_or_name(&metadata),
            Some("Camel Kid".to_string())
        );
        // A real snake_case name wins over the camelCase duplicate.
        let both = Metadata::new()
            .name("snake")
            .custom_field("displayName", "camel");
        assert_eq!(display_name_or_name(&both), Some("snake".to_string()));
        // Empty camelCase values are ignored.
        let empty_camel = Metadata::new().custom_field("displayName", "  ");
        assert_eq!(display_name_or_name(&empty_camel), None);
    }

    fn test_metadata_event(pubkey: PublicKey, created_at: u64, content: &str) -> Event {
        use nostr_sdk::prelude::*;
        Event::new(
            EventId::all_zeros(),
            pubkey,
            Timestamp::from_secs(created_at),
            Kind::Metadata,
            [],
            content.to_string(),
            Signature::from_slice(&[0u8; 64]).expect("dummy signature"),
        )
    }

    #[test]
    fn parse_profile_event_reads_camel_case_display_name_fallback() {
        let keys = PublicKey::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        let event = test_metadata_event(
            keys,
            1_700_000_000,
            r#"{"displayName":"Camel Kid","about":"hi"}"#,
        );
        let profile = parse_profile_event(&event).unwrap();
        assert_eq!(profile.display_name.as_deref(), Some("Camel Kid"));
        assert_eq!(profile.event_created_at, Some(1_700_000_000));
    }

    #[test]
    fn parse_profile_event_blank_content_is_wipe_not_error() {
        let keys = PublicKey::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        let event = test_metadata_event(keys, 1_700_000_000, "   ");
        let profile = parse_profile_event(&event).unwrap();
        assert_eq!(profile.name, None);
        assert_eq!(profile.display_name, None);
        assert_eq!(profile.picture, None);
        assert_eq!(profile.event_created_at, Some(1_700_000_000));
    }

    #[test]
    fn newest_metadata_by_author_keeps_newest_version() {
        let keys = PublicKey::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        // Delivery order: oldest first — the fold must still pick the newest.
        let events = vec![
            test_metadata_event(keys, 100, r#"{"name":"old"}"#),
            test_metadata_event(keys, 300, r#"{"name":"newest"}"#),
            test_metadata_event(keys, 200, r#"{"name":"mid"}"#),
        ];
        let folded = newest_metadata_by_author(events);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].created_at.as_secs(), 300);
        let parsed = parse_profile_event(&folded[0]).unwrap();
        assert_eq!(parsed.name.as_deref(), Some("newest"));

        // Different authors are kept separately.
        let other = PublicKey::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000002",
        )
        .unwrap();
        let mixed = vec![
            test_metadata_event(keys, 100, "{}"),
            test_metadata_event(other, 500, "{}"),
        ];
        assert_eq!(newest_metadata_by_author(mixed).len(), 2);
    }
}
