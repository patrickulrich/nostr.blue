use dioxus::prelude::*;
use dioxus::events::MouseData;
use nostr_sdk::{Event as NostrEvent, PublicKey, Filter, Kind, FromBech32, Timestamp, JsonUtil};
use crate::routes::Route;
use crate::stores::nostr_client::{get_client, CLIENT_INITIALIZED};
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
    pub starts: Option<Timestamp>,
    pub tags: Vec<String>,
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
                            // Validate timestamp is non-negative to avoid wraparound
                            if ts >= 0 {
                                meta.starts = Some(Timestamp::from(ts as u64));
                            } else {
                                log::warn!("Negative timestamp {} in starts tag, skipping", ts);
                            }
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
pub fn LiveStreamCard(event: NostrEvent) -> Element {
    let stream_meta = match parse_live_stream_event(&event) {
        Some(meta) => meta,
        None => return rsx! { div { class: "hidden" } }
    };

    // Clone values for closures
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let author_pubkey_display = author_pubkey.clone();
    let created_at = event.created_at;

    // Create naddr for the livestream
    let naddr = format!("30311:{}:{}", author_pubkey, stream_meta.d_tag);

    // State for author profile
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch author profile
    use_effect(use_reactive((&author_pubkey_for_fetch, &*CLIENT_INITIALIZED.read()), move |(pk, client_initialized)| {
        // Short-circuit until client is ready
        if !client_initialized {
            return;
        }

        spawn(async move {
            if let Ok(pubkey) = PublicKey::from_bech32(&pk).or_else(|_| PublicKey::parse(&pk)) {
                if let Some(client) = get_client() {
                    let filter = Filter::new()
                        .kind(Kind::Metadata)
                        .author(pubkey)
                        .limit(1);

                    match client.fetch_events(filter, Duration::from_secs(5)).await {
                        Ok(events) => {
                            if let Some(event) = events.first() {
                                if let Ok(metadata) = nostr_sdk::Metadata::from_json(&event.content) {
                                    author_metadata.set(Some(metadata));
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to fetch author metadata: {}", e);
                        }
                    }
                }
            }
        });
    }));

    // Get author display info
    let author_name = if let Some(ref metadata) = *author_metadata.read() {
        metadata.display_name.clone()
            .or_else(|| metadata.name.clone())
            .unwrap_or_else(|| format!("{}...{}", &author_pubkey_display[..8], &author_pubkey_display[author_pubkey_display.len()-4..]))
    } else {
        format!("{}...{}", &author_pubkey_display[..8], &author_pubkey_display[author_pubkey_display.len()-4..])
    };

    let author_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    rsx! {
        div {
            class: "border-b border-border hover:bg-accent/5 transition",

            // Author header
            div {
                class: "p-4 flex items-center gap-3",
                Link {
                    to: Route::Profile { pubkey: author_pubkey.clone() },
                    class: "flex items-center gap-3 flex-1",
                    if let Some(pic_url) = author_picture {
                        img {
                            src: "{pic_url}",
                            class: "w-12 h-12 rounded-full object-cover",
                            alt: "Avatar",
                            loading: "lazy"
                        }
                    } else {
                        div {
                            class: "w-12 h-12 rounded-full bg-blue-600 flex items-center justify-center text-white font-bold",
                            "{author_name.chars().next().unwrap_or('?').to_ascii_uppercase()}"
                        }
                    }
                    div {
                        class: "flex-1",
                        div {
                            class: "font-semibold",
                            "{author_name}"
                        }
                        div {
                            class: "text-sm text-muted-foreground",
                            "{created_at.to_human_datetime()}"
                        }
                    }
                }
            }

            // Stream thumbnail with LIVE badge
            Link {
                to: Route::LiveStreamDetail { note_id: naddr.clone() },
                div {
                    class: "relative bg-black cursor-pointer group",
                    if let Some(img_url) = &stream_meta.image {
                        img {
                            src: "{img_url}",
                            class: "w-full aspect-video object-cover group-hover:opacity-90 transition",
                            alt: "Stream thumbnail",
                            loading: "lazy"
                        }
                    } else {
                        div {
                            class: "w-full aspect-video bg-gray-800 flex items-center justify-center",
                            svg {
                                class: "w-24 h-24 text-gray-600",
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

                    // LIVE badge (top left)
                    if stream_meta.status == StreamStatus::Live {
                        div {
                            class: "absolute top-2 left-2 bg-red-600 text-white text-xs font-bold px-3 py-1 rounded flex items-center gap-1",
                            div {
                                class: "w-2 h-2 bg-white rounded-full animate-pulse"
                            }
                            "LIVE"
                        }
                    } else if stream_meta.status == StreamStatus::Planned {
                        div {
                            class: "absolute top-2 left-2 bg-blue-600 text-white text-xs font-bold px-3 py-1 rounded",
                            "UPCOMING"
                        }
                    }

                    // Viewer count (bottom right)
                    if stream_meta.status == StreamStatus::Live {
                        if let Some(viewers) = stream_meta.current_participants {
                            div {
                                class: "absolute bottom-2 right-2 bg-black/75 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
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
            }

            // Title and description
            div {
                class: "p-4",
                if let Some(title_text) = &stream_meta.title {
                    h3 {
                        class: "font-bold text-lg mb-2",
                        "{title_text}"
                    }
                }
                if let Some(summary_text) = &stream_meta.summary {
                    if !summary_text.is_empty() {
                        p {
                            class: "text-sm text-muted-foreground whitespace-pre-wrap line-clamp-2",
                            "{summary_text}"
                        }
                    }
                }

                // Tags
                if !stream_meta.tags.is_empty() {
                    div {
                        class: "flex flex-wrap gap-2 mt-3",
                        for tag in &stream_meta.tags {
                            Link {
                                to: Route::VideosLiveTag { tag: tag.clone() },
                                class: "text-xs bg-accent hover:bg-accent/80 text-primary px-2 py-1 rounded transition",
                                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                "#{tag}"
                            }
                        }
                    }
                }
            }
        }
    }
}
