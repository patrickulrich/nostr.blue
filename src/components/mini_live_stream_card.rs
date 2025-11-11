use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, PublicKey, Filter, Kind, JsonUtil};
use crate::routes::Route;
use crate::stores::nostr_client;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum StreamStatus {
    Planned,
    Live,
    Ended,
}

impl StreamStatus {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "live" => StreamStatus::Live,
            "ended" => StreamStatus::Ended,
            _ => StreamStatus::Planned,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LiveStreamMeta {
    pub d_tag: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub image: Option<String>,
    pub streaming_url: Option<String>,
    pub status: StreamStatus,
    pub current_participants: Option<u32>,
    pub starts: Option<nostr_sdk::Timestamp>,
    pub tags: Vec<String>,
    /// The actual creator/streamer (from p tag), if present
    pub creator_pubkey: Option<String>,
}

/// Parse NIP-53 Kind 30311 live streaming event
pub fn parse_live_stream_event(event: &NostrEvent) -> Option<LiveStreamMeta> {
    let mut meta = LiveStreamMeta {
        d_tag: String::new(),
        title: None,
        summary: None,
        image: None,
        streaming_url: None,
        status: StreamStatus::Planned,
        current_participants: None,
        starts: None,
        tags: Vec::new(),
        creator_pubkey: None,
    };

    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if let Some(tag_name) = tag_vec.first().map(|s| s.as_str()) {
            match tag_name {
                "d" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.d_tag = value.to_string();
                    }
                }
                "p" => {
                    // Get the first p tag as the creator/streamer
                    if meta.creator_pubkey.is_none() {
                        if let Some(value) = tag_vec.get(1) {
                            meta.creator_pubkey = Some(value.to_string());
                        }
                    }
                }
                "title" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.title = Some(value.to_string());
                    }
                }
                "summary" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.summary = Some(value.to_string());
                    }
                }
                "image" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.image = Some(value.to_string());
                    }
                }
                "streaming" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.streaming_url = Some(value.to_string());
                    }
                }
                "status" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.status = StreamStatus::from_str(value);
                    }
                }
                "current_participants" => {
                    if let Some(value) = tag_vec.get(1) {
                        if let Ok(count) = value.parse::<u32>() {
                            meta.current_participants = Some(count);
                        }
                    }
                }
                "starts" => {
                    if let Some(value) = tag_vec.get(1) {
                        if let Ok(ts) = value.parse::<i64>() {
                            // Only convert to u64 if timestamp is non-negative
                            if ts >= 0 {
                                if let Ok(timestamp_u64) = u64::try_from(ts) {
                                    meta.starts = Some(nostr_sdk::Timestamp::from(timestamp_u64));
                                }
                            }
                            // Negative timestamps are ignored
                        }
                    }
                }
                "t" => {
                    if let Some(value) = tag_vec.get(1) {
                        meta.tags.push(value.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    // Only return if we have a d_tag (required)
    if meta.d_tag.is_empty() {
        None
    } else {
        Some(meta)
    }
}

#[component]
pub fn MiniLiveStreamCard(event: NostrEvent) -> Element {
    let stream_meta = match parse_live_stream_event(&event) {
        Some(meta) => meta,
        None => return rsx! { div { class: "hidden" } }
    };

    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Use creator pubkey from p tag if available, otherwise fall back to event publisher
    let author_pubkey = stream_meta.creator_pubkey.clone()
        .unwrap_or_else(|| event.pubkey.to_string());

    // Create naddr for the livestream (still uses event publisher for fetching)
    let naddr = format!("30311:{}:{}", event.pubkey, stream_meta.d_tag);

    // Fetch author metadata
    use_effect(use_reactive(&author_pubkey, move |pubkey_str| {
        spawn(async move {
            if let Ok(pubkey) = PublicKey::parse(&pubkey_str) {
                let filter = Filter::new()
                    .author(pubkey)
                    .kind(Kind::Metadata)
                    .limit(1);

                if let Ok(events) = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await {
                    if let Some(event) = events.into_iter().next() {
                        if let Ok(metadata) = nostr_sdk::Metadata::from_json(&event.content) {
                            author_metadata.set(Some(metadata));
                        }
                    }
                }
            }
        });
    }));

    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            let pk = event.pubkey.to_string();
            if pk.len() > 16 {
                format!("{}...{}", &pk[..8], &pk[pk.len()-8..])
            } else {
                pk
            }
        });

    // Format timestamp as "X ago"
    let format_time_ago = |timestamp: u64| -> String {
        let now = (js_sys::Date::now() / 1000.0) as u64;
        let diff = now.saturating_sub(timestamp);

        match diff {
            0..=59 => "just now".to_string(),
            60..=3599 => format!("{}m ago", diff / 60),
            3600..=86399 => format!("{}h ago", diff / 3600),
            86400..=604799 => format!("{}d ago", diff / 86400),
            _ => format!("{}w ago", diff / 604800),
        }
    };

    rsx! {
        div {
            class: "group cursor-pointer",

            Link {
                to: Route::LiveStreamDetail { note_id: naddr.clone() },

                div {
                    class: "relative aspect-video bg-muted rounded-lg overflow-hidden mb-3",

                    // Show thumbnail if available
                    if let Some(img_url) = &stream_meta.image {
                        img {
                            src: "{img_url}",
                            alt: "{stream_meta.title.as_deref().unwrap_or(\"Live Stream\")}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                        }
                    } else {
                        div {
                            class: "w-full h-full flex items-center justify-center bg-gray-800",
                            svg {
                                class: "w-12 h-12 text-gray-600",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                                }
                            }
                        }
                    }

                    // LIVE badge (top-left)
                    if stream_meta.status == StreamStatus::Live {
                        div {
                            class: "absolute top-2 left-2 bg-red-600 text-white text-xs font-bold px-2 py-1 rounded flex items-center gap-1",
                            div {
                                class: "w-1.5 h-1.5 bg-white rounded-full animate-pulse"
                            }
                            "LIVE"
                        }
                    } else if stream_meta.status == StreamStatus::Planned {
                        div {
                            class: "absolute top-2 left-2 bg-blue-600 text-white text-xs font-bold px-2 py-1 rounded",
                            "UPCOMING"
                        }
                    }

                    // Viewer count badge (bottom-right)
                    if stream_meta.status == StreamStatus::Live {
                        if let Some(viewers) = stream_meta.current_participants {
                            div {
                                class: "absolute bottom-2 right-2 bg-black/80 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
                                svg {
                                    class: "w-3 h-3",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                                    }
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                                    }
                                }
                                "{viewers}"
                            }
                        }
                    }
                }

                // Stream info
                div {
                    if let Some(title) = &stream_meta.title {
                        h3 {
                            class: "font-semibold line-clamp-2 mb-1 group-hover:text-primary transition",
                            "{title}"
                        }
                    }

                    p {
                        class: "text-sm text-muted-foreground mb-1",
                        "{display_name}"
                    }

                    p {
                        class: "text-xs text-muted-foreground",
                        "{format_time_ago(event.created_at.as_secs())}"
                    }
                }
            }
        }
    }
}
