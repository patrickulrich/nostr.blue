use crate::stores::nostr_client;
use crate::utils::format::truncate_pubkey;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use lru::LruCache;
use nostr_sdk::{Event, Filter, FromBech32, Kind, PublicKey};
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
    /// Get the avatar URL, with Dicebear fallback
    pub fn get_avatar_url(&self) -> String {
        if let Some(picture) = &self.picture {
            if !picture.trim().is_empty()
                && (picture.starts_with("http://") || picture.starts_with("https://"))
            {
                return picture.clone();
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
/// Cache TTL in seconds (24 hours)
/// Increased from 5 minutes to reduce network requests for stable profile data
const CACHE_TTL_SECONDS: i64 = 24 * 60 * 60;
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
/// Internal function to fetch profile from relays and update cache
async fn fetch_profile_from_relays(pubkey: &str) -> Result<Profile, String> {
    log::info!("Fetching profile from database/relays for {}", pubkey);
    let public_key = PublicKey::from_bech32(pubkey)
        .or_else(|_| PublicKey::from_hex(pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .author(public_key)
        .limit(1);
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let profile = parse_profile_event(&event)?;
                PROFILE_CACHE
                    .write()
                    .put(pubkey.to_string(), profile.clone());
                Ok(profile)
            } else {
                let profile = Profile {
                    pubkey: pubkey.to_string(),
                    name: None,
                    display_name: None,
                    about: None,
                    picture: None,
                    banner: None,
                    nip05: None,
                    lud16: None,
                    website: None,
                    bot: None,
                    birthday: None,
                    fetched_at: Utc::now(),
                    raw_metadata_json: None,
                };
                PROFILE_CACHE
                    .write()
                    .put(pubkey.to_string(), profile.clone());
                Ok(profile)
            }
        }
        Err(e) => {
            log::error!("Failed to fetch profile: {}", e);
            let profile = Profile {
                pubkey: pubkey.to_string(),
                name: None,
                display_name: None,
                about: None,
                picture: None,
                banner: None,
                nip05: None,
                lud16: None,
                website: None,
                bot: None,
                birthday: None,
                fetched_at: Utc::now(),
                raw_metadata_json: None,
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
            for event in events {
                if let Ok(profile) = parse_profile_event(&event) {
                    PROFILE_CACHE
                        .write()
                        .put(profile.pubkey.clone(), profile.clone());
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
            "Querying relays for {} profiles not in database",
            still_missing.len()
        );
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(still_missing.iter().copied());
        match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
            Ok(events) => {
                for event in events {
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
                log::error!("Failed to fetch profiles from relays: {}", e);
            }
        }
    }
    Ok(results)
}
