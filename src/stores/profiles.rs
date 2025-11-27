use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::{Event, Filter, Kind, PublicKey, FromBech32};
use crate::stores::nostr_client;
use std::time::Duration;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use lru::LruCache;
use chrono::{DateTime, Utc};

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
    pub website: Option<String>,
    pub fetched_at: DateTime<Utc>,
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
        // Fallback to npub prefix (first 12 chars)
        if self.pubkey.len() >= 12 {
            format!("npub1{}...", &self.pubkey[..12])
        } else {
            self.pubkey.clone()
        }
    }

    /// Get the avatar URL, with Dicebear fallback
    pub fn get_avatar_url(&self) -> String {
        if let Some(picture) = &self.picture {
            if !picture.trim().is_empty() && (picture.starts_with("http://") || picture.starts_with("https://")) {
                return picture.clone();
            }
        }
        // Dicebear identicon fallback
        format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", self.pubkey)
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
                return words[0].chars().next().unwrap_or('?').to_uppercase().to_string();
            }
        }
        self.pubkey.chars().next().unwrap_or('?').to_uppercase().to_string()
    }
}

/// Global signal to cache profiles (pubkey -> Profile)
/// LRU cache with max capacity of 5000 profiles to prevent unbounded memory growth
/// Increased from 1000 to better serve power users who follow many accounts
pub static PROFILE_CACHE: GlobalSignal<LruCache<String, Profile>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(5000).unwrap()));

/// Cache TTL in seconds (5 minutes)
const CACHE_TTL_SECONDS: i64 = 300;

/// Get a profile from cache only (synchronous)
pub fn get_profile(pubkey: &str) -> Option<nostr_sdk::Metadata> {
    PROFILE_CACHE.read().peek(pubkey).map(|profile| {
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
    })
}

/// Fetch a profile from relays by pubkey
pub async fn fetch_profile(pubkey: String) -> Result<Profile, String> {
    // Check cache first
    if let Some(cached_profile) = PROFILE_CACHE.read().peek(&pubkey) {
        let age = Utc::now().signed_duration_since(cached_profile.fetched_at);
        if age.num_seconds() < CACHE_TTL_SECONDS {
            log::debug!("Using cached profile for {}", pubkey);
            return Ok(cached_profile.clone());
        }
    }

    log::info!("Fetching profile from database/relays for {}", pubkey);

    let public_key = PublicKey::from_bech32(&pubkey)
        .or_else(|_| PublicKey::from_hex(&pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch Kind 0 metadata events using aggregated query
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .author(public_key)
        .limit(1);

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let profile = parse_profile_event(&event)?;

                // Cache the profile
                PROFILE_CACHE.write().put(pubkey.clone(), profile.clone());

                Ok(profile)
            } else {
                // No profile found, return empty profile
                let profile = Profile {
                    pubkey: pubkey.clone(),
                    name: None,
                    display_name: None,
                    about: None,
                    picture: None,
                    banner: None,
                    nip05: None,
                    lud16: None,
                    website: None,
                    fetched_at: Utc::now(),
                };

                // Cache the empty profile to avoid re-fetching
                PROFILE_CACHE.write().put(pubkey, profile.clone());

                Ok(profile)
            }
        }
        Err(e) => {
            log::error!("Failed to fetch profile: {}", e);

            // Return empty profile on error
            let profile = Profile {
                pubkey: pubkey.clone(),
                name: None,
                display_name: None,
                about: None,
                picture: None,
                banner: None,
                nip05: None,
                lud16: None,
                website: None,
                fetched_at: Utc::now(),
            };

            Ok(profile)
        }
    }
}

/// Parse a Kind 0 event into a Profile struct
fn parse_profile_event(event: &Event) -> Result<Profile, String> {
    let content = &event.content;
    let metadata: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse metadata JSON: {}", e))?;

    Ok(Profile {
        pubkey: event.pubkey.to_string(),
        name: metadata.get("name").and_then(|v| v.as_str()).map(String::from),
        display_name: metadata.get("display_name").and_then(|v| v.as_str()).map(String::from),
        about: metadata.get("about").and_then(|v| v.as_str()).map(String::from),
        picture: metadata.get("picture").and_then(|v| v.as_str()).map(String::from),
        banner: metadata.get("banner").and_then(|v| v.as_str()).map(String::from),
        nip05: metadata.get("nip05").and_then(|v| v.as_str()).map(String::from),
        lud16: metadata.get("lud16").and_then(|v| v.as_str()).map(String::from),
        website: metadata.get("website").and_then(|v| v.as_str()).map(String::from),
        fetched_at: Utc::now(),
    })
}

/// Get a profile from cache (if available)
pub fn get_cached_profile(pubkey: &str) -> Option<Profile> {
    PROFILE_CACHE.read().peek(pubkey).cloned()
}

/// Fetch multiple profiles in a single query (much more efficient than individual fetches)
#[allow(dead_code)]
pub async fn fetch_profiles_batch(pubkeys: Vec<String>) -> Result<HashMap<String, Profile>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    let mut results = HashMap::new();
    let mut missing = Vec::new();

    // Check cache first
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

    // Parse all missing pubkeys
    let authors: Vec<PublicKey> = missing.iter()
        .filter_map(|pk| {
            PublicKey::from_bech32(pk)
                .or_else(|_| PublicKey::from_hex(pk))
                .ok()
        })
        .collect();

    if authors.is_empty() {
        return Ok(results);
    }

    // Single query for all profiles
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .authors(authors);

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            for event in events {
                if let Ok(profile) = parse_profile_event(&event) {
                    PROFILE_CACHE.write().put(profile.pubkey.clone(), profile.clone());
                    results.insert(profile.pubkey.clone(), profile);
                }
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
        // Spawn each fetch concurrently
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
pub async fn fetch_profiles_batch_native(pubkeys: HashSet<PublicKey>) -> Result<HashMap<PublicKey, Profile>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    let mut results = HashMap::new();
    let mut missing = Vec::new();

    // Single lock acquisition for all cache lookups
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

    // Get client once
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    // Step 1: SINGLE batch query to database for all missing profiles (5-10x faster than individual queries)
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .authors(missing.iter().copied());

    match client.database().query(filter).await {
        Ok(database_events) => {
            // Process all database results at once
            for event in database_events {
                if let Ok(profile) = parse_profile_event(&event) {
                    let pk = event.pubkey;
                    PROFILE_CACHE.write().put(profile.pubkey.clone(), profile.clone());
                    results.insert(pk, profile);
                }
            }
        }
        Err(e) => {
            log::warn!("Database batch query failed: {}, will query relays for all", e);
        }
    }

    // Identify profiles still missing (not in database)
    let found_pubkeys: HashSet<PublicKey> = results.keys().copied().collect();
    let still_missing: Vec<PublicKey> = missing.into_iter()
        .filter(|pk| !found_pubkeys.contains(pk))
        .collect();

    // Step 2: Only query relays for profiles not in database
    if !still_missing.is_empty() {
        log::info!("Querying relays for {} profiles not in database", still_missing.len());

        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(still_missing.iter().copied());

        match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
            Ok(events) => {
                for event in events {
                    if let Ok(profile) = parse_profile_event(&event) {
                        let pk = event.pubkey;
                        PROFILE_CACHE.write().put(profile.pubkey.clone(), profile.clone());
                        results.insert(pk, profile);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to fetch profiles from relays: {}", e);
                // Don't return error - we got some results from cache/database
            }
        }
    }

    Ok(results)
}
