use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, FromBech32, JsonUtil};
use crate::stores::nostr_client::{self, get_client, fetch_events_aggregated};
use crate::components::{ClientInitializing, LiveStreamPlayer, LiveChat, ZapModal};
use crate::routes::Route;
use crate::components::live_stream_card::{parse_live_stream_event, StreamStatus};
use std::time::Duration;

#[component]
pub fn LiveStreamDetail(note_id: String) -> Element {
    let mut stream_event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut show_zap_modal = use_signal(|| false);

    // Parse the note_id to extract pubkey and d_tag
    // Format: 30311:pubkey:dtag
    let parts: Vec<&str> = note_id.split(':').collect();
    let (author_pubkey, d_tag) = if parts.len() == 3 {
        (parts[1].to_string(), parts[2].to_string())
    } else {
        (String::new(), String::new())
    };

    // Fetch the livestream event
    use_effect(use_reactive((&note_id, &*nostr_client::CLIENT_INITIALIZED.read()), move |(nid, client_initialized)| {
        if !client_initialized {
            return;
        }

        spawn(async move {
            loading.set(true);
            error.set(None);

            let parts: Vec<&str> = nid.split(':').collect();
            if parts.len() != 3 {
                error.set(Some("Invalid stream identifier".to_string()));
                loading.set(false);
                return;
            }

            let kind_str = parts[0];
            let pubkey_str = parts[1];
            let d_tag = parts[2];

            if let Ok(pubkey) = PublicKey::parse(pubkey_str) {
                if let Ok(kind_num) = kind_str.parse::<u16>() {
                    let filter = Filter::new()
                        .kind(Kind::from(kind_num))
                        .author(pubkey)
                        .custom_tag(
                            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::D),
                            d_tag
                        )
                        .limit(1);

                    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                        Ok(events) => {
                            if let Some(event) = events.into_iter().next() {
                                stream_event.set(Some(event));
                            } else {
                                error.set(Some("Stream not found".to_string()));
                            }
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(format!("Failed to fetch stream: {}", e)));
                            loading.set(false);
                        }
                    }
                } else {
                    error.set(Some("Invalid kind number".to_string()));
                    loading.set(false);
                }
            } else {
                error.set(Some("Invalid public key".to_string()));
                loading.set(false);
            }
        });
    }));

    // Fetch author metadata
    use_effect(use_reactive(&author_pubkey, move |pk| {
        if pk.is_empty() {
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

    rsx! {
        div {
            class: "min-h-screen bg-background",

            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if *loading.read() {
                div {
                    class: "flex items-center justify-center h-screen",
                    div {
                        class: "w-12 h-12 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "flex flex-col items-center justify-center h-screen gap-4",
                    div {
                        class: "text-xl text-muted-foreground",
                        "{err}"
                    }
                    Link {
                        to: Route::VideosLive {},
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition",
                        "← Back to Live Streams"
                    }
                }
            } else if let Some(event) = stream_event.read().as_ref() {
                if let Some(stream_meta) = parse_live_stream_event(event) {
                    div {
                        class: "h-screen flex flex-col",

                        // Header
                        div {
                            class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                            div {
                                class: "px-6 py-4 flex items-center gap-4",
                                Link {
                                    to: Route::VideosLive {},
                                    class: "hover:bg-accent p-2 rounded-full transition",
                                    crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                                }
                                div {
                                    class: "flex-1",
                                    h1 {
                                        class: "text-xl font-bold",
                                        "{stream_meta.title.clone().unwrap_or_else(|| \"Untitled Stream\".to_string())}"
                                    }
                                }
                                if stream_meta.status == StreamStatus::Live {
                                    div {
                                        class: "flex items-center gap-2 bg-red-600 text-white text-sm font-bold px-3 py-1 rounded",
                                        div {
                                            class: "w-2 h-2 bg-white rounded-full animate-pulse"
                                        }
                                        "LIVE"
                                    }
                                }
                            }
                        }

                        // Main content area: player + chat
                        div {
                            class: "flex-1 flex flex-col lg:flex-row overflow-hidden",

                            // Left side: Video player and info
                            div {
                                class: "flex-1 flex flex-col overflow-y-auto",

                                // Video player
                                div {
                                    class: "bg-black",
                                    if let Some(streaming_url) = &stream_meta.streaming_url {
                                        if !streaming_url.is_empty() {
                                            LiveStreamPlayer {
                                                stream_url: streaming_url.clone()
                                            }
                                        } else {
                                            div {
                                                class: "aspect-video flex flex-col items-center justify-center text-white gap-3",
                                                svg {
                                                    class: "w-16 h-16 text-gray-500",
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
                                                div {
                                                    class: "text-lg",
                                                    if stream_meta.status == StreamStatus::Planned {
                                                        "Stream hasn't started yet"
                                                    } else if stream_meta.status == StreamStatus::Ended {
                                                        "Stream has ended"
                                                    } else {
                                                        "No stream URL available"
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        div {
                                            class: "aspect-video flex flex-col items-center justify-center text-white gap-3",
                                            svg {
                                                class: "w-16 h-16 text-gray-500",
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
                                            div {
                                                class: "text-lg",
                                                if stream_meta.status == StreamStatus::Planned {
                                                    "Stream hasn't started yet"
                                                } else if stream_meta.status == StreamStatus::Ended {
                                                    "Stream has ended"
                                                } else {
                                                    "Stream URL not configured"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Stream info
                                div {
                                    class: "p-6 space-y-4",

                                    // Author info and actions
                                    div {
                                        class: "flex items-start gap-4",
                                        Link {
                                            to: Route::Profile { pubkey: author_pubkey.clone() },
                                            class: "flex-shrink-0",
                                            if let Some(pic_url) = author_metadata.read().as_ref().and_then(|m| m.picture.clone()) {
                                                img {
                                                    src: "{pic_url}",
                                                    class: "w-12 h-12 rounded-full object-cover",
                                                    alt: "Avatar",
                                                    loading: "lazy"
                                                }
                                            } else {
                                                div {
                                                    class: "w-12 h-12 rounded-full bg-blue-600 flex items-center justify-center text-white font-bold",
                                                    "?"
                                                }
                                            }
                                        }
                                        div {
                                            class: "flex-1",
                                            Link {
                                                to: Route::Profile { pubkey: author_pubkey.clone() },
                                                class: "font-semibold hover:underline",
                                                if let Some(ref metadata) = *author_metadata.read() {
                                                    "{metadata.display_name.clone().or_else(|| metadata.name.clone()).unwrap_or_else(|| \"Unknown\".to_string())}"
                                                } else {
                                                    "Loading..."
                                                }
                                            }
                                            if let Some(viewers) = stream_meta.current_participants {
                                                div {
                                                    class: "text-sm text-muted-foreground flex items-center gap-1 mt-1",
                                                    svg {
                                                        class: "w-4 h-4",
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
                                                    "{viewers} watching"
                                                }
                                            }
                                        }
                                        button {
                                            class: "px-4 py-2 bg-yellow-500 hover:bg-yellow-600 text-white font-medium rounded-lg transition flex items-center gap-2",
                                            onclick: move |_| show_zap_modal.set(true),
                                            crate::components::icons::ZapIcon {
                                                class: "w-4 h-4".to_string(),
                                                filled: false
                                            }
                                            "Zap"
                                        }
                                    }

                                    // Description
                                    if let Some(summary) = &stream_meta.summary {
                                        if !summary.is_empty() {
                                            div {
                                                class: "text-sm whitespace-pre-wrap",
                                                "{summary}"
                                            }
                                        }
                                    }

                                    // Tags
                                    if !stream_meta.tags.is_empty() {
                                        div {
                                            class: "flex flex-wrap gap-2",
                                            for tag in &stream_meta.tags {
                                                Link {
                                                    to: Route::VideosLiveTag { tag: tag.clone() },
                                                    class: "text-sm bg-accent hover:bg-accent/80 text-primary px-3 py-1 rounded transition",
                                                    "#{tag}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Right side: Live chat
                            div {
                                class: "w-full lg:w-96 h-96 lg:h-auto border-t lg:border-t-0 lg:border-l border-border",
                                LiveChat {
                                    stream_author_pubkey: author_pubkey.clone(),
                                    stream_d_tag: d_tag.clone()
                                }
                            }
                        }
                    }

                    // Zap modal
                    if *show_zap_modal.read() {
                        ZapModal {
                            recipient_pubkey: author_pubkey.clone(),
                            recipient_name: author_metadata.read().as_ref()
                                .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
                                .unwrap_or_else(|| "Stream Host".to_string()),
                            lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                            lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                            event_id: Some(event.id.to_string()),
                            on_close: move |_| {
                                show_zap_modal.set(false);
                            }
                        }
                    }
                } else {
                    div {
                        class: "text-center py-20 text-muted-foreground",
                        "Failed to parse stream data"
                    }
                }
            }
        }
    }
}
