use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{ThreadedComment, CommentComposer, ClientInitializing, icons::MessageCircleIcon};
use crate::utils::build_thread_tree;
use nostr_sdk::{Event, Filter, Kind, EventId};
use std::time::Duration;

#[component]
pub fn VideoDetail(video_id: String) -> Element {
    let mut video_event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut is_muted = use_signal(|| false);
    let mut comments = use_signal(|| Vec::<Event>::new());
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);

    // Load video on mount - wait for client to be initialized
    use_effect(move || {
        let id = video_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            log::info!("Waiting for client initialization before loading video...");
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match load_video_by_id(&id).await {
                Ok(event) => {
                    video_event.set(Some(event));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Fetch NIP-22 comments for the video
    use_effect(move || {
        let video_data = video_event.read();

        if let Some(event) = video_data.as_ref() {
            let event_id = event.id;

            spawn(async move {
                loading_comments.set(true);

                // Fetch Kind 1111 (NIP-22 Comment) events that reference this video
                let filter = Filter::new()
                    .kind(Kind::Comment)
                    .event(event_id)
                    .limit(500);

                match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                    Ok(mut comment_events) => {
                        comment_events.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                        log::info!("Loaded {} NIP-22 comments for video", comment_events.len());
                        comments.set(comment_events);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch video comments: {}", e);
                    }
                }

                loading_comments.set(false);
            });
        }
    });

    rsx! {
        div {
            class: "min-h-screen bg-black",

            // Header
            div {
                class: "sticky top-0 z-20 bg-black/80 backdrop-blur-sm border-b border-gray-800",
                div {
                    class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: crate::routes::Route::Videos {},
                        class: "hover:bg-white/20 p-2 rounded-full transition text-white",
                        "← Back"
                    }
                    h2 {
                        class: "text-xl font-bold text-white",
                        "Video"
                    }
                }
            }

            // Content
            div {
                class: "max-w-[1200px] mx-auto",

                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && video_event.read().is_none()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial video load (loading + no video, regardless of error state)
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div {
                        class: "flex items-center justify-center h-[80vh] text-white",
                        div {
                            class: "text-center",
                            div {
                                class: "text-6xl mb-4",
                                "⚠️"
                            }
                            p {
                                class: "text-red-400 mb-4",
                                "Error loading video: {err}"
                            }
                            Link {
                                to: crate::routes::Route::Videos {},
                                class: "text-blue-400 hover:underline",
                                "← Back to Videos"
                            }
                        }
                    }
                } else if let Some(event) = video_event.read().as_ref().cloned() {
                    VideoContent {
                        event: event.clone(),
                        is_muted: *is_muted.read(),
                        on_mute_toggle: move |_| {
                            let current = *is_muted.read();
                            is_muted.set(!current);
                        }
                    }

                    // Comments section
                    div {
                        class: "bg-card border border-border rounded-lg p-6 mt-4 max-w-[1200px] mx-auto",
                        div {
                            class: "flex items-center justify-between mb-4",
                            h3 {
                                class: "text-xl font-bold",
                                "Comments"
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center gap-2",
                                onclick: move |_| show_comment_composer.set(true),
                                MessageCircleIcon { class: "w-4 h-4".to_string(), filled: false }
                                span { "Add Comment" }
                            }
                        }

                        {
                            let comment_vec = comments.read().clone();
                            let thread_tree = build_thread_tree(comment_vec, &event.id);

                            rsx! {
                                if *loading_comments.read() {
                                    div {
                                        class: "flex items-center justify-center py-10",
                                        div {
                                            class: "text-center",
                                            div {
                                                class: "animate-spin text-4xl mb-2",
                                                "⚡"
                                            }
                                            p {
                                                class: "text-muted-foreground",
                                                "Loading comments..."
                                            }
                                        }
                                    }
                                } else if thread_tree.is_empty() {
                                    div {
                                        class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                        p { "No comments yet" }
                                        p {
                                            class: "text-sm",
                                            "Be the first to comment!"
                                        }
                                    }
                                } else {
                                    div {
                                        class: "divide-y divide-border",
                                        for node in thread_tree {
                                            ThreadedComment {
                                                node: node.clone(),
                                                depth: 0
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Comment composer modal
                    if *show_comment_composer.read() {
                        CommentComposer {
                            comment_on: event.clone(),
                            parent_comment: None,
                            on_close: move |_| show_comment_composer.set(false),
                            on_success: move |_| {
                                show_comment_composer.set(false);
                                // Refresh comments
                                let event_id = event.id;
                                spawn(async move {
                                    loading_comments.set(true);
                                    let filter = Filter::new()
                                        .kind(Kind::Comment)
                                        .event(event_id)
                                        .limit(500);

                                    if let Ok(mut comment_events) = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
                                        comment_events.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                                        comments.set(comment_events);
                                    }
                                    loading_comments.set(false);
                                });
                            }
                        }
                    }
                } else {
                    // Not found state
                    div {
                        class: "flex items-center justify-center h-[80vh] text-white",
                        div {
                            class: "text-center",
                            div {
                                class: "text-6xl mb-4",
                                "❓"
                            }
                            p {
                                class: "text-gray-400 mb-4",
                                "Video not found"
                            }
                            Link {
                                to: crate::routes::Route::Videos {},
                                class: "text-blue-400 hover:underline",
                                "← Back to Videos"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn VideoContent(event: Event, is_muted: bool, on_mute_toggle: EventHandler<()>) -> Element {
    let video_meta = parse_video_meta(&event);

    rsx! {
        div {
            class: "p-4",

            // Video player
            div {
                class: "relative w-full bg-black rounded-lg overflow-hidden mb-4",
                style: "max-height: 80vh;",

                if let Some(url) = &video_meta.url {
                    video {
                        class: "w-full h-full object-contain",
                        src: "{url}",
                        poster: "{video_meta.thumbnail.clone().unwrap_or_default()}",
                        controls: true,
                        muted: is_muted,
                        autoplay: true,
                        playsinline: true,
                    }
                } else {
                    div {
                        class: "flex items-center justify-center h-96 text-white",
                        div {
                            class: "text-center",
                            div {
                                class: "text-6xl mb-4",
                                "▶"
                            }
                            p {
                                "Video unavailable"
                            }
                        }
                    }
                }
            }

            // Video info
            div {
                class: "bg-card border border-border rounded-lg p-6",

                // Author info
                AuthorInfo {
                    pubkey: event.pubkey.to_string()
                }

                // Title
                if let Some(title) = &video_meta.title {
                    h1 {
                        class: "text-2xl font-bold mb-3",
                        "{title}"
                    }
                }

                // Description
                if !event.content.is_empty() {
                    p {
                        class: "text-muted-foreground mb-4 whitespace-pre-wrap",
                        "{event.content}"
                    }
                }

                // Metadata
                div {
                    class: "flex flex-wrap gap-4 text-sm text-muted-foreground",

                    if let Some(duration) = &video_meta.duration {
                        div {
                            class: "flex items-center gap-2",
                            "⏱️"
                            span {
                                "{duration}"
                            }
                        }
                    }

                    if let Some(dimensions) = &video_meta.dimensions {
                        div {
                            class: "flex items-center gap-2",
                            "📐"
                            span {
                                "{dimensions}"
                            }
                        }
                    }

                    div {
                        class: "flex items-center gap-2",
                        "📅"
                        span {
                            "{format_time_ago(event.created_at.as_u64())}"
                        }
                    }
                }

                // Action buttons
                div {
                    class: "flex gap-3 mt-6 pt-4 border-t border-border",

                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                        "❤️"
                        span {
                            "Like"
                        }
                    }

                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                        "💬"
                        span {
                            "Comment"
                        }
                    }

                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                        "↗️"
                        span {
                            "Share"
                        }
                    }

                    button {
                        class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                        onclick: move |_| on_mute_toggle.call(()),
                        if is_muted {
                            "🔇"
                        } else {
                            "🔊"
                        }
                        span {
                            if is_muted {
                                "Unmute"
                            } else {
                                "Mute"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AuthorInfo(pubkey: String) -> Element {
    use nostr_sdk::{PublicKey, FromBech32};

    let pubkey_clone = pubkey.clone();
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch author's profile metadata
    use_effect(move || {
        let pubkey_str = pubkey_clone.clone();

        spawn(async move {
            let pubkey_parsed = match PublicKey::from_hex(&pubkey_str)
                .or_else(|_| PublicKey::from_bech32(&pubkey_str)) {
                Ok(pk) => pk,
                Err(_) => return,
            };

            let filter = Filter::new()
                .author(pubkey_parsed)
                .kind(Kind::Metadata)
                .limit(1);

            if let Ok(events) = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await {
                if let Some(event) = events.into_iter().next() {
                    if let Ok(metadata) = serde_json::from_str::<nostr_sdk::Metadata>(&event.content) {
                        author_metadata.set(Some(metadata));
                    }
                }
            }
        });
    });

    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            if pubkey.len() > 16 {
                format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
            } else {
                pubkey.clone()
            }
        });

    let profile_image = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    rsx! {
        Link {
            to: crate::routes::Route::Profile { pubkey: pubkey.clone() },
            class: "flex items-center gap-3 mb-4 hover:opacity-80 transition",

            div {
                class: "w-12 h-12 rounded-full bg-gray-700 flex items-center justify-center text-white font-bold overflow-hidden",
                if let Some(img_url) = profile_image {
                    img {
                        src: "{img_url}",
                        alt: "{display_name}",
                        class: "w-full h-full object-cover"
                    }
                } else {
                    "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                }
            }

            div {
                span {
                    class: "font-semibold",
                    "{display_name}"
                }
            }
        }
    }
}

// Video metadata structure
#[derive(Clone, Debug)]
struct VideoMeta {
    url: Option<String>,
    thumbnail: Option<String>,
    title: Option<String>,
    duration: Option<String>,
    dimensions: Option<String>,
}

// Parse NIP-71 video metadata from imeta tags
fn parse_video_meta(event: &Event) -> VideoMeta {
    let mut meta = VideoMeta {
        url: None,
        thumbnail: None,
        title: None,
        duration: None,
        dimensions: None,
    };

    // Parse title tag
    for tag in event.tags.iter() {
        let tag_vec = (*tag).clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") && tag_vec.len() > 1 {
            meta.title = Some(tag_vec[1].clone());
            break;
        }
    }

    // Parse imeta tags
    for tag in event.tags.iter() {
        let tag_vec = (*tag).clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            for field in tag_vec.iter().skip(1) {
                if let Some((key, value)) = field.split_once(' ') {
                    match key {
                        "url" => meta.url = Some(value.to_string()),
                        "image" => meta.thumbnail = Some(value.to_string()),
                        "duration" => meta.duration = Some(value.to_string()),
                        "dim" => meta.dimensions = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }
    }

    meta
}

// Format timestamp as "X ago"
fn format_time_ago(timestamp: u64) -> String {
    // Use js_sys::Date::now() for WASM compatibility
    let now = (js_sys::Date::now() / 1000.0) as u64;

    let diff = now.saturating_sub(timestamp);

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", diff / 60),
        3600..=86399 => format!("{}h ago", diff / 3600),
        86400..=604799 => format!("{}d ago", diff / 86400),
        _ => format!("{}w ago", diff / 604800),
    }
}

// Helper function to load a video by ID
async fn load_video_by_id(video_id: &str) -> Result<Event, String> {
    log::info!("Loading video by ID: {}", video_id);

    // Parse the video ID (could be event ID or naddr)
    let event_id = EventId::parse(video_id)
        .map_err(|e| format!("Invalid video ID: {}", e))?;

    // Create filter for this specific video event
    let filter = Filter::new()
        .id(event_id)
        .kinds([Kind::Custom(21), Kind::Custom(22)]);

    // Fetch the event
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch video: {}", e))?;

    events
        .into_iter()
        .next()
        .ok_or_else(|| "Video not found".to_string())
}
