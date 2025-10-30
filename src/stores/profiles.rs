use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, FromBech32};
use crate::stores::nostr_client;
use std::time::Duration;
use std::collections::HashMap;
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
pub static PROFILE_CACHE: GlobalSignal<HashMap<String, Profile>> =
    Signal::global(|| HashMap::new());

/// Cache TTL in seconds (5 minutes)
const CACHE_TTL_SECONDS: i64 = 300;

/// Fetch a profile from relays by pubkey
pub async fn fetch_profile(pubkey: String) -> Result<Profile, String> {
    // Check cache first
    if let Some(cached_profile) = PROFILE_CACHE.read().get(&pubkey) {
        let age = Utc::now().signed_duration_since(cached_profile.fetched_at);
        if age.num_seconds() < CACHE_TTL_SECONDS {
            log::debug!("Using cached profile for {}", pubkey);
            return Ok(cached_profile.clone());
        }
    }

    log::info!("Fetching profile from relays for {}", pubkey);

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let public_key = PublicKey::from_bech32(&pubkey)
        .or_else(|_| PublicKey::from_hex(&pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch Kind 0 metadata events
    let filter = Filter::new()
        .kind(Kind::Metadata)
        .author(public_key)
        .limit(1);

    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                let profile = parse_profile_event(&event)?;

                // Cache the profile
                PROFILE_CACHE.write().insert(pubkey.clone(), profile.clone());

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
                PROFILE_CACHE.write().insert(pubkey, profile.clone());

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
    PROFILE_CACHE.read().get(pubkey).cloned()
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
