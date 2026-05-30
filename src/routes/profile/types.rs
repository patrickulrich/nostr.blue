use crate::utils::article_meta::{get_identifier, get_published_at};
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Event as NostrEvent;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
pub enum MediaSubTab {
    Photos,
    Videos,
    Verts,
}

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
pub enum ZapSubTab {
    Sent,
    Received,
}

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
pub enum ProfileTab {
    Posts,
    Replies,
    Articles,
    Media(MediaSubTab),
    Likes,
    Zaps(ZapSubTab),
}

#[derive(Clone, Debug)]
pub struct TabData {
    pub events: Vec<NostrEvent>,
    pub oldest_timestamp: Option<u64>,
    pub has_more: bool,
    pub loaded: bool,
}

#[derive(Clone, Debug)]
pub struct LoadOutcome {
    pub events: Vec<NostrEvent>,
    #[allow(dead_code)]
    pub oldest_cursor: Option<u64>,
    #[allow(dead_code)]
    pub relay_count: usize,
}

impl Default for TabData {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            oldest_timestamp: None,
            has_more: true,
            loaded: false,
        }
    }
}

pub fn default_tab_data_map() -> HashMap<ProfileTab, TabData> {
    let mut map = HashMap::new();
    map.insert(ProfileTab::Posts, TabData::default());
    map.insert(ProfileTab::Replies, TabData::default());
    map.insert(ProfileTab::Articles, TabData::default());
    map.insert(ProfileTab::Media(MediaSubTab::Photos), TabData::default());
    map.insert(ProfileTab::Media(MediaSubTab::Videos), TabData::default());
    map.insert(ProfileTab::Media(MediaSubTab::Verts), TabData::default());
    map.insert(ProfileTab::Likes, TabData::default());
    map.insert(ProfileTab::Zaps(ZapSubTab::Sent), TabData::default());
    map.insert(ProfileTab::Zaps(ZapSubTab::Received), TabData::default());
    map
}

pub fn dedupe_articles_by_address(articles: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut address_map: HashMap<String, NostrEvent> = HashMap::new();
    for article in articles {
        let identifier =
            get_identifier(&article).unwrap_or_else(|| format!("id-{}", article.id.to_hex()));
        let address = format!(
            "{}:{}:{}",
            article.kind.as_u16(),
            article.pubkey.to_hex(),
            identifier,
        );
        address_map
            .entry(address)
            .and_modify(|existing| {
                if get_published_at(&article) > get_published_at(existing) {
                    *existing = article.clone();
                }
            })
            .or_insert(article);
    }
    address_map.into_values().collect()
}

pub fn get_display_name(metadata: &nostr_sdk::Metadata, pubkey: &str) -> String {
    metadata
        .display_name
        .clone()
        .or_else(|| metadata.name.clone())
        .unwrap_or_else(|| {
            if let Some(pk) = crate::utils::nip19_urls::parse_profile_id(pubkey) {
                let hex = pk.to_hex();
                format!("{}...{}", &hex[..8], &hex[hex.len() - 4..])
            } else {
                "Unknown".to_string()
            }
        })
}

pub fn get_username(metadata: &nostr_sdk::Metadata, pubkey: &str) -> String {
    metadata.name.clone().unwrap_or_else(|| {
        if let Some(pk) = crate::utils::nip19_urls::parse_profile_id(pubkey) {
            let npub = pk.to_bech32().expect("to_bech32 is infallible");
            if npub.len() > 18 {
                format!("{}...{}", &npub[..12], &npub[npub.len() - 6..])
            } else {
                npub
            }
        } else {
            "unknown".to_string()
        }
    })
}

pub fn get_avatar_initial(metadata: &nostr_sdk::Metadata) -> String {
    metadata
        .display_name
        .as_ref()
        .or(metadata.name.as_ref())
        .and_then(|n| n.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

pub fn strip_https(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .to_string()
}

pub fn get_empty_state_message(tab: &ProfileTab) -> &'static str {
    match tab {
        ProfileTab::Posts => "No posts yet",
        ProfileTab::Replies => "No replies yet",
        ProfileTab::Articles => "No articles yet",
        ProfileTab::Media(MediaSubTab::Photos) => "No photos yet",
        ProfileTab::Media(MediaSubTab::Videos) => "No videos yet",
        ProfileTab::Media(MediaSubTab::Verts) => "No verts yet",
        ProfileTab::Likes => "No likes yet",
        ProfileTab::Zaps(ZapSubTab::Sent) => "No zaps sent yet",
        ProfileTab::Zaps(ZapSubTab::Received) => "No zaps received yet",
    }
}

pub fn get_empty_state_icon(tab: &ProfileTab) -> &'static str {
    match tab {
        ProfileTab::Posts => "📝",
        ProfileTab::Replies => "💬",
        ProfileTab::Articles => "📄",
        ProfileTab::Media(MediaSubTab::Photos) => "🖼️",
        ProfileTab::Media(MediaSubTab::Videos) => "🎬",
        ProfileTab::Media(MediaSubTab::Verts) => "📱",
        ProfileTab::Likes => "❤️",
        ProfileTab::Zaps(_) => "⚡",
    }
}

pub fn format_timestamp(secs: u64) -> String {
    let days = secs / 86400;
    if days == 0 {
        let hours = secs / 3600;
        if hours == 0 {
            let mins = secs / 60;
            if mins == 0 {
                "just now".to_string()
            } else {
                format!("{mins}m ago")
            }
        } else {
            format!("{hours}h ago")
        }
    } else if days < 30 {
        format!("{days}d ago")
    } else {
        let months = days / 30;
        format!("{months}mo ago")
    }
}
