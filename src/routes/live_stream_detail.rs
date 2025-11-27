use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind, PublicKey, FromBech32};
use crate::components::{LiveStreamPlayer, LiveChat, StreamStatus};
use crate::components::live_stream_card::{parse_live_stream_event, LiveStreamMeta};
use crate::components::icons::ArrowLeftIcon;
use crate::routes::Route;
use crate::stores::nostr_client::{fetch_events_aggregated, CLIENT_INITIALIZED};
use crate::stores::profiles;
use std::time::Duration;

#[component]
pub fn LiveStreamDetail(note_id: String) -> Element {
    // Parse naddr format: "30311:pubkey:dtag" or just use note_id directly
    let parsed_naddr = use_memo(move || parse_naddr(&note_id));

    // Stream state
    let mut stream_event = use_signal(|| None::<nostr_sdk::Event>);
    let mut stream_meta = use_signal(|| None::<LiveStreamMeta>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Get author metadata from profile store (use host from p tag if available)
    let author_metadata = use_memo(move || {
        if let Some(meta) = stream_meta.read().as_ref() {
            // Use host_pubkey from p tag if available
            if let Some(host_pk) = &meta.host_pubkey {
                return profiles::get_profile(host_pk);
            }
        }
        // Fall back to event publisher
        let (pubkey_str, _) = parsed_naddr.read().clone();
        profiles::get_profile(&pubkey_str)
    });

    // Fetch stream event
    use_effect(use_reactive(
        (&*CLIENT_INITIALIZED.read(), &parsed_naddr),
        move |(client_ready, _naddr)| {
            if !client_ready {
                return;
            }

            let (author_pk, dtag) = parsed_naddr.read().clone();

            spawn(async move {
                loading.set(true);
                error.set(None);

                // Parse author pubkey
                let pubkey = match PublicKey::parse(&author_pk) {
                    Ok(pk) => pk,
                    Err(e) => {
                        error.set(Some(format!("Invalid public key: {}", e)));
                        loading.set(false);
                        return;
                    }
                };

                // Build filter for Kind 30311 with author + d-tag
                let filter = Filter::new()
                    .kind(Kind::from(30311))
                    .author(pubkey)
                    .custom_tag(
                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::D),
                        &dtag
                    )
                    .limit(1);

                // Fetch stream event
                match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(events) => {
                        if let Some(event) = events.first() {
                            // Parse stream metadata
                            if let Some(meta) = parse_live_stream_event(event) {
                                // Fetch host profile (use p tag if available, otherwise publisher)
                                let profile_to_fetch = meta.host_pubkey.clone()
                                    .unwrap_or_else(|| author_pk.clone());

                                stream_meta.set(Some(meta));
                                stream_event.set(Some(event.clone()));
                                loading.set(false);

                                // Fetch creator/author metadata
                                let _ = profiles::fetch_profile(profile_to_fetch).await;
                            } else {
                                error.set(Some("Failed to parse stream metadata".to_string()));
                                loading.set(false);
                            }
                        } else {
                            error.set(Some("Stream not found".to_string()));
                            loading.set(false);
                        }
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to load stream: {}", e)));
                        loading.set(false);
                    }
                }
            });
        }
    ));

    // Handle refresh
    let handle_refresh = move |_| {
        loading.set(true);
        error.set(None);

        let (author_pk, dtag) = parsed_naddr.peek().clone();

        spawn(async move {
            if let Ok(pubkey) = PublicKey::parse(&author_pk) {
                let filter = Filter::new()
                    .kind(Kind::from(30311))
                    .author(pubkey)
                    .custom_tag(
                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::D),
                        &dtag
                    )
                    .limit(1);

                match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(events) => {
                        if let Some(event) = events.first() {
                            if let Some(meta) = parse_live_stream_event(event) {
                                stream_meta.set(Some(meta));
                                stream_event.set(Some(event.clone()));
                                loading.set(false);
                            }
                        }
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to refresh: {}", e)));
                        loading.set(false);
                    }
                }
            }
        });
    };

    rsx! {
        div {
            class: "flex flex-col h-screen overflow-hidden",

            // Header
            div {
                class: "flex-shrink-0 bg-background/95 backdrop-blur-sm border-b border-border p-4",
                div {
                    class: "flex items-center gap-4",

                    // Back button
                    Link {
                        to: Route::VideosLive {},
                        class: "p-2 hover:bg-accent rounded-lg transition-colors",
                        ArrowLeftIcon { class: "w-6 h-6".to_string() }
                    }

                    // Title and status
                    div {
                        class: "flex-1 min-w-0",
                        if let Some(meta) = stream_meta.read().as_ref() {
                            div {
                                class: "flex items-center gap-2",
                                h1 {
                                    class: "text-xl font-bold truncate",
                                    "{meta.title.as_deref().unwrap_or(\"Live Stream\")}"
                                }
                                // Status badge
                                {render_status_badge(&meta.status)}
                            }
                        } else {
                            h1 {
                                class: "text-xl font-bold",
                                "Live Stream"
                            }
                        }
                    }

                    // Refresh button
                    button {
                        class: "p-2 hover:bg-accent rounded-lg transition-colors",
                        onclick: handle_refresh,
                        disabled: *loading.read(),
                        svg {
                            class: if *loading.read() { "w-6 h-6 animate-spin" } else { "w-6 h-6" },
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                            }
                        }
                    }
                }
            }

            // Main content
            div {
                class: "flex-1 min-h-0 overflow-hidden",

                // Loading state
                if *loading.read() && error.read().is_none() {
                    div {
                        class: "flex items-center justify-center h-full",
                        div {
                            class: "flex flex-col items-center gap-4",
                            div {
                                class: "w-12 h-12 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                            }
                            p {
                                class: "text-muted-foreground",
                                "Loading stream..."
                            }
                        }
                    }
                }

                // Error state
                else if let Some(error_msg) = error.read().as_ref() {
                    div {
                        class: "flex items-center justify-center h-full",
                        div {
                            class: "text-center p-6 max-w-md",
                            svg {
                                class: "w-16 h-16 text-red-500 mx-auto mb-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                                }
                            }
                            h2 {
                                class: "text-2xl font-bold mb-2",
                                "Error Loading Stream"
                            }
                            p {
                                class: "text-muted-foreground mb-4",
                                "{error_msg}"
                            }
                            button {
                                class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition-colors",
                                onclick: handle_refresh,
                                "Try Again"
                            }
                        }
                    }
                }

                // Stream content
                else if let Some(meta) = stream_meta.read().as_ref() {
                    div {
                        class: "flex flex-col lg:flex-row h-full overflow-hidden",

                        // Left: Player and info
                        div {
                            class: "flex-1 flex flex-col overflow-y-auto min-h-0",
                            div {
                                class: "p-4 space-y-4",

                                // Video player
                                if let Some(stream_url) = &meta.streaming_url {
                                    if !stream_url.is_empty() {
                                        LiveStreamPlayer {
                                            stream_url: stream_url.clone(),
                                            poster: meta.image.clone(),
                                            autoplay: true
                                        }
                                    } else {
                                        {render_no_stream_placeholder(&meta.status)}
                                    }
                                } else {
                                    {render_no_stream_placeholder(&meta.status)}
                                }

                                // Stream info
                                div {
                                    class: "space-y-4",

                                    // Author info
                                    if let Some(event) = stream_event.read().as_ref() {
                                        div {
                                            class: "flex items-center gap-3",
                                            // Avatar
                                            if let Some(metadata) = author_metadata.read().as_ref() {
                                                if let Some(picture) = &metadata.picture {
                                                    img {
                                                        src: "{picture}",
                                                        class: "w-12 h-12 rounded-full object-cover",
                                                        alt: "Author avatar"
                                                    }
                                                } else {
                                                    div {
                                                        class: "w-12 h-12 bg-accent rounded-full flex items-center justify-center",
                                                        span { class: "text-lg", "👤" }
                                                    }
                                                }
                                            } else {
                                                div {
                                                    class: "w-12 h-12 bg-accent rounded-full flex items-center justify-center",
                                                    span { class: "text-lg", "👤" }
                                                }
                                            }

                                            // Author name and stats
                                            div {
                                                class: "flex-1",
                                                {
                                                    // Get host-aware identifier (use p tag host if available, otherwise event publisher)
                                                    let event_pubkey = event.pubkey.to_string();
                                                    let author_identifier = meta.host_pubkey.as_ref()
                                                        .map(|s| s.as_str())
                                                        .unwrap_or(&event_pubkey);

                                                    rsx! {
                                                        div {
                                                            class: "font-semibold",
                                                            if let Some(metadata) = author_metadata.read().as_ref() {
                                                                if let Some(name) = &metadata.name {
                                                                    "{name}"
                                                                } else {
                                                                    "{truncate_pubkey(author_identifier)}"
                                                                }
                                                            } else {
                                                                "{truncate_pubkey(author_identifier)}"
                                                            }
                                                        }
                                                    }
                                                }
                                                if let Some(viewers) = meta.current_participants {
                                                    div {
                                                        class: "text-sm text-muted-foreground",
                                                        "{viewers} watching"
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Description
                                    if let Some(summary) = &meta.summary {
                                        if !summary.is_empty() {
                                            div {
                                                class: "text-sm text-foreground",
                                                "{summary}"
                                            }
                                        }
                                    }

                                    // Tags
                                    if !meta.tags.is_empty() {
                                        div {
                                            class: "flex flex-wrap gap-2",
                                            for tag in meta.tags.iter() {
                                                Link {
                                                    key: "{tag}",
                                                    to: Route::VideosLiveTag { tag: tag.clone() },
                                                    class: "px-3 py-1 bg-accent hover:bg-accent/80 rounded-full text-sm transition-colors",
                                                    "#{tag}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Right: Chat
                        div {
                            class: "lg:w-96 border-t lg:border-t-0 lg:border-l border-border h-full min-h-0",
                            if let Some(_event) = stream_event.read().as_ref() {
                                {
                                    let (author_pk, dtag) = parsed_naddr.peek().clone();
                                    rsx! {
                                        LiveChat {
                                            stream_author_pubkey: author_pk,
                                            stream_d_tag: dtag
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Parse naddr format - supports both NIP-19 bech32 and "30311:pubkey:dtag" formats
fn parse_naddr(note_id: &str) -> (String, String) {
    // First try to decode as NIP-19 bech32 naddr
    if let Ok(nip19) = nostr_sdk::nips::nip19::Nip19::from_bech32(note_id) {
        if let nostr_sdk::nips::nip19::Nip19::Coordinate(coord) = nip19 {
            return (coord.public_key.to_hex(), coord.identifier.clone());
        }
    }

    // Fall back to colon-split logic for non-bech32 formats
    let parts: Vec<&str> = note_id.split(':').collect();

    if parts.len() >= 3 {
        // Full naddr format: "30311:pubkey:dtag"
        (parts[1].to_string(), parts[2].to_string())
    } else {
        // Fallback: treat as pubkey:dtag
        let parts: Vec<&str> = note_id.splitn(2, ':').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            // Invalid format, return empty
            (String::new(), String::new())
        }
    }
}

/// Render status badge
fn render_status_badge(status: &StreamStatus) -> Element {
    match status {
        StreamStatus::Live => rsx! {
            span {
                class: "px-2 py-1 bg-red-500 text-white text-xs font-bold rounded uppercase",
                "LIVE"
            }
        },
        StreamStatus::Planned => rsx! {
            span {
                class: "px-2 py-1 bg-blue-500 text-white text-xs font-bold rounded uppercase",
                "Upcoming"
            }
        },
        StreamStatus::Ended => rsx! {
            span {
                class: "px-2 py-1 bg-gray-500 text-white text-xs font-bold rounded uppercase",
                "Ended"
            }
        },
    }
}

/// Render placeholder when stream URL is not available
fn render_no_stream_placeholder(status: &StreamStatus) -> Element {
    let (message, icon) = match status {
        StreamStatus::Planned => ("Stream has not started yet", "📅"),
        StreamStatus::Ended => ("Stream has ended", "🎬"),
        StreamStatus::Live => ("Stream URL not available", "❌"),
    };

    rsx! {
        div {
            class: "relative w-full aspect-video bg-black rounded-lg overflow-hidden flex items-center justify-center",
            div {
                class: "text-center p-6",
                div {
                    class: "text-6xl mb-4",
                    "{icon}"
                }
                p {
                    class: "text-white text-lg",
                    "{message}"
                }
            }
        }
    }
}

/// Truncate public key for display
fn truncate_pubkey(pubkey: &str) -> String {
    if pubkey.len() > 16 {
        format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
    } else {
        pubkey.to_string()
    }
}
