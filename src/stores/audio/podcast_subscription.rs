use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::{EventBuilder, Filter, FromBech32, Kind, Tag};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const LIST_KIND: u16 = 30003;
const D_TAG: &str = "podcast-subscriptions";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastSubscription {
    pub podcast_guid: Option<String>,
    pub podcast_id: Option<u64>,
    pub feed_url: Option<String>,
    pub nostr_coordinate: Option<String>,
    pub relay_hint: Option<String>,
    pub title: Option<String>,
    pub image: Option<String>,
}

impl PodcastSubscription {
    pub fn from_rss(
        podcast_guid: String,
        podcast_id: Option<u64>,
        feed_url: Option<String>,
    ) -> Self {
        Self {
            podcast_guid: Some(podcast_guid),
            podcast_id,
            feed_url,
            nostr_coordinate: None,
            relay_hint: None,
            title: None,
            image: None,
        }
    }

    pub fn from_nostr(coordinate: String, relay_hint: Option<String>) -> Self {
        Self {
            podcast_guid: None,
            podcast_id: None,
            feed_url: None,
            nostr_coordinate: Some(coordinate),
            relay_hint,
            title: None,
            image: None,
        }
    }

    pub fn id(&self) -> Option<String> {
        if let Some(ref guid) = self.podcast_guid {
            Some(guid.clone())
        } else if let Some(ref coordinate) = self.nostr_coordinate {
            Some(coordinate.clone())
        } else {
            log::warn!("Invalid subscription: no podcast_guid or nostr_coordinate");
            None
        }
    }

    pub fn numeric_id(&self) -> Option<u64> {
        self.podcast_id
    }

    pub fn is_rss(&self) -> bool {
        self.podcast_guid.is_some()
    }

    pub fn is_nostr(&self) -> bool {
        self.nostr_coordinate.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Store, Default)]
pub struct PodcastSubscriptionState {
    pub subscriptions: Vec<PodcastSubscription>,
    pub loading: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

pub static PODCAST_SUBS: GlobalStore<PodcastSubscriptionState> =
    Global::new(PodcastSubscriptionState::default);

pub fn get_categories() -> Vec<PodcastCategory> {
    vec![
        PodcastCategory::new("Technology", "Coding, gadgets, and digital trends"),
        PodcastCategory::new("Business", "Finance, entrepreneurship, and markets"),
        PodcastCategory::new("News", "Current events and journalism"),
        PodcastCategory::new("Science", "Research, discoveries, and nature"),
        PodcastCategory::new("Education", "Learning and self-improvement"),
        PodcastCategory::new("True Crime", "Investigations and mysteries"),
        PodcastCategory::new("Comedy", "Humor and entertainment"),
        PodcastCategory::new("Sports", "Games, athletes, and competition"),
        PodcastCategory::new("Health", "Wellness and fitness"),
        PodcastCategory::new("Society", "Culture and social issues"),
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub struct PodcastCategory {
    pub name: &'static str,
    pub description: &'static str,
}

impl PodcastCategory {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        Self { name, description }
    }
}

pub async fn fetch_subscriptions() -> Result<Vec<PodcastSubscription>, String> {
    log::info!("Fetching podcast subscriptions from Nostr (NIP-51 Kind 30003)...");
    {
        let mut state = PODCAST_SUBS.write();
        state.loading = true;
        state.error = None;
    }
    if !auth_store::is_authenticated() {
        log::info!("Not authenticated, no subscriptions to fetch");
        let mut state = PODCAST_SUBS.write();
        state.loading = false;
        state.loaded = true;
        return Ok(Vec::new());
    }
    let client = match nostr_client::NOSTR_CLIENT.read().as_ref() {
        Some(c) => c.clone(),
        None => {
            let err = "Client not initialized".to_string();
            log::warn!("{}", err);
            let mut state = PODCAST_SUBS.write();
            state.loading = false;
            state.loaded = true;
            state.error = Some(err.clone());
            return Err(err);
        }
    };
    let pubkey_str = auth_store::AUTH_STATE.read()
        .pubkey
        .clone()
        .ok_or_else(|| {
            let err = "No pubkey".to_string();
            log::warn!("{}", err);
            {
                let mut state = PODCAST_SUBS.write();
                state.loading = false;
                state.loaded = true;
                state.error = Some(err.clone());
            }
            err
        })?;
    let pubkey = match nostr_sdk::PublicKey::from_bech32(&pubkey_str)
        .or_else(|_| nostr_sdk::PublicKey::from_hex(&pubkey_str))
    {
        Ok(pk) => pk,
        Err(e) => {
            let err = format!("Invalid pubkey: {}", e);
            log::warn!("{}", err);
            let mut state = PODCAST_SUBS.write();
            state.loading = false;
            state.loaded = true;
            state.error = Some(err.clone());
            return Err(err);
        }
    };
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(LIST_KIND))
        .identifier(D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    let subscriptions = match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Found podcast subscriptions event: {}", event.id);
                parse_subscription_event(&event)
            } else {
                log::info!("No podcast subscriptions found on Nostr");
                Vec::new()
            }
        }
        Err(e) => {
            let error_msg = format!("Fetch error: {}", e);
            log::warn!("Failed to fetch subscriptions: {}", e);
            {
                let mut state = PODCAST_SUBS.write();
                state.error = Some(error_msg.clone());
                state.loading = false;
            }
            return Err(error_msg);
        }
    };
    {
        let mut state = PODCAST_SUBS.write();
        state.subscriptions = subscriptions.clone();
        state.loading = false;
        state.loaded = true;
    }
    #[cfg(feature = "mobile_platform")]
    {
        if let Ok(subs_json) = serde_json::to_string(&subscriptions) {
            let _ = crate::platform::android_media::save_browse_cache("subscriptions", &subs_json);
        }
    }
    Ok(subscriptions)
}

fn parse_subscription_event(event: &nostr_sdk::Event) -> Vec<PodcastSubscription> {
    let mut subscriptions = Vec::new();
    for tag in event.tags.iter() {
        let tag_vec: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
        match tag_vec.as_slice() {
            ["i", identifier] | ["i", identifier, _] => {
                if let Some(guid) = identifier.strip_prefix("podcast:guid:") {
                    subscriptions.push(PodcastSubscription::from_rss(guid.to_string(), None, None));
                }
            }
            ["r", podcast_id_str] => {
                log::warn!(
                    "Found legacy r tag with numeric ID: {}. Consider re-subscribing for NIP-73 compliance.",
                    podcast_id_str
                );
            }
            ["a", coordinate] => {
                subscriptions.push(PodcastSubscription::from_nostr(
                    coordinate.to_string(),
                    None,
                ));
            }
            ["a", coordinate, relay_hint] => {
                subscriptions.push(PodcastSubscription::from_nostr(
                    coordinate.to_string(),
                    Some(relay_hint.to_string()),
                ));
            }
            _ => {}
        }
    }
    subscriptions
}

pub async fn add_rss_subscription(
    podcast_guid: &str,
    podcast_id: Option<u64>,
    feed_url: Option<&str>,
) -> Result<(), String> {
    log::info!(
        "Adding RSS subscription: guid={}, id={:?}",
        podcast_guid,
        podcast_id
    );
    if is_subscribed(podcast_guid) {
        return Err("Already subscribed to this podcast".to_string());
    }
    let mut subs = PODCAST_SUBS.read().subscriptions.clone();
    subs.push(PodcastSubscription::from_rss(
        podcast_guid.to_string(),
        podcast_id,
        feed_url.map(String::from),
    ));
    publish_subscriptions(&subs).await?;
    {
        let mut state = PODCAST_SUBS.write();
        state.subscriptions = subs.clone();
    }
    #[cfg(feature = "mobile_platform")]
    {
        if let Ok(subs_json) = serde_json::to_string(&subs) {
            let _ = crate::platform::android_media::save_browse_cache("subscriptions", &subs_json);
        }
    }
    Ok(())
}

pub async fn add_nostr_subscription(
    coordinate: &str,
    relay_hint: Option<&str>,
) -> Result<(), String> {
    log::info!("Adding Nostr subscription: {}", coordinate);
    if is_subscribed(coordinate) {
        return Err("Already subscribed to this podcast".to_string());
    }
    let mut subs = PODCAST_SUBS.read().subscriptions.clone();
    subs.push(PodcastSubscription::from_nostr(
        coordinate.to_string(),
        relay_hint.map(String::from),
    ));
    publish_subscriptions(&subs).await?;
    {
        let mut state = PODCAST_SUBS.write();
        state.subscriptions = subs.clone();
    }
    #[cfg(feature = "mobile_platform")]
    {
        if let Ok(subs_json) = serde_json::to_string(&subs) {
            let _ = crate::platform::android_media::save_browse_cache("subscriptions", &subs_json);
        }
    }
    Ok(())
}

pub async fn remove_subscription(id: &str) -> Result<(), String> {
    log::info!("Removing subscription: {}", id);
    let mut subs = PODCAST_SUBS.read().subscriptions.clone();
    subs.retain(|s| s.id().as_deref() != Some(id));
    publish_subscriptions(&subs).await?;
    {
        let mut state = PODCAST_SUBS.write();
        state.subscriptions = subs.clone();
    }
    #[cfg(feature = "mobile_platform")]
    {
        if let Ok(subs_json) = serde_json::to_string(&subs) {
            let _ = crate::platform::android_media::save_browse_cache("subscriptions", &subs_json);
        }
    }
    Ok(())
}

async fn publish_subscriptions(subscriptions: &[PodcastSubscription]) -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    if !nostr_client::has_signer() {
        return Err("No signer available".to_string());
    }
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let mut tags = vec![
        Tag::identifier(D_TAG),
        Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("title")),
            vec!["My Podcast Subscriptions".to_string()],
        ),
    ];
    let has_rss = subscriptions.iter().any(|s| s.podcast_guid.is_some());
    if has_rss {
        tags.push(Tag::custom(
            nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("k")),
            vec!["podcast:guid".to_string()],
        ));
    }
    for sub in subscriptions {
        if let Some(ref guid) = sub.podcast_guid {
            tags.push(Tag::custom(
                nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("i")),
                vec![format!("podcast:guid:{}", guid)],
            ));
        } else if let Some(ref coordinate) = sub.nostr_coordinate {
            let mut a_values = vec![coordinate.clone()];
            if let Some(ref relay) = sub.relay_hint {
                a_values.push(relay.clone());
            }
            tags.push(Tag::custom(
                nostr_sdk::TagKind::Custom(std::borrow::Cow::Borrowed("a")),
                a_values,
            ));
        }
    }
    let builder = EventBuilder::new(Kind::from(LIST_KIND), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("podcast".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Podcast subscriptions saved to Nostr successfully");
    Ok(())
}

pub fn get_subscriptions() -> Vec<PodcastSubscription> {
    PODCAST_SUBS.read().subscriptions.clone()
}

pub fn get_rss_subscriptions() -> Vec<PodcastSubscription> {
    PODCAST_SUBS
        .read()
        .subscriptions
        .iter()
        .filter(|s| s.is_rss())
        .cloned()
        .collect()
}

pub fn get_nostr_podcasts() -> Vec<String> {
    PODCAST_SUBS
        .read()
        .subscriptions
        .iter()
        .filter_map(|s| s.nostr_coordinate.clone())
        .collect()
}

pub fn is_subscribed(id: &str) -> bool {
    PODCAST_SUBS
        .read()
        .subscriptions
        .iter()
        .any(|s| s.id().as_deref() == Some(id))
}

pub fn is_loading() -> bool {
    PODCAST_SUBS.read().loading
}

#[allow(dead_code)]
pub fn is_loaded() -> bool {
    PODCAST_SUBS.read().loaded
}

#[allow(dead_code)]
pub fn get_error() -> Option<String> {
    PODCAST_SUBS.read().error.clone()
}
