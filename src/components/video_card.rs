use dioxus::prelude::*;
use nostr_sdk::{Event, PublicKey, Filter, Kind, FromBech32, JsonUtil};
use crate::routes::Route;
use crate::stores::nostr_client::{publish_reaction, get_client, HAS_SIGNER};
use crate::stores::bookmarks;
use crate::stores::signer::SIGNER_INFO;
use crate::components::icons::{HeartIcon, MessageCircleIcon, BookmarkIcon, ZapIcon};
use crate::components::ZapModal;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct VideoMeta {
    pub url: String,
    pub mime_type: Option<String>,
    pub duration: Option<f64>,
    pub dim: Option<(u32, u32)>,
    pub thumbnail: Option<String>,
    pub fallback_urls: Vec<String>,
}

/// Parse imeta tags from NIP-71 video events
pub fn parse_video_imeta_tags(event: &Event) -> Vec<VideoMeta> {
    let mut videos = Vec::new();

    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            let mut video = VideoMeta {
                url: String::new(),
                mime_type: None,
                duration: None,
                dim: None,
                thumbnail: None,
                fallback_urls: Vec::new(),
            };

            // Parse imeta tag fields
            for field in tag_vec.iter().skip(1) {
                if let Some((key, value)) = field.split_once(' ') {
                    match key {
                        "url" => video.url = value.to_string(),
                        "m" => video.mime_type = Some(value.to_string()),
                        "duration" => {
                            if let Ok(dur) = value.parse::<f64>() {
                                video.duration = Some(dur);
                            }
                        }
                        "dim" => {
                            if let Some((w, h)) = value.split_once('x') {
                                if let (Ok(width), Ok(height)) = (w.parse(), h.parse()) {
                                    video.dim = Some((width, height));
                                }
                            }
                        }
                        "image" => {
                            if video.thumbnail.is_none() {
                                video.thumbnail = Some(value.to_string());
                            }
                        }
                        "fallback" => {
                            video.fallback_urls.push(value.to_string());
                        }
                        _ => {}
                    }
                }
            }

            // Only add if we have a URL
            if !video.url.is_empty() {
                videos.push(video);
            }
        }
    }

    videos
}

/// Get the title from NIP-71 video events
pub fn get_video_title(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") {
            return tag_vec.get(1).map(|s| s.to_string());
        }
    }
    None
}

/// Format duration in seconds to MM:SS or HH:MM:SS
fn format_duration(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

#[component]
pub fn VideoCard(event: Event) -> Element {
    let videos = parse_video_imeta_tags(&event);
    let title = get_video_title(&event);
    let description = &event.content;

    // Clone values for closures
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let author_pubkey_for_like = author_pubkey.clone();
    let author_pubkey_display = author_pubkey.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();

    // State for interactions
    let mut is_liking = use_signal(|| false);
    let mut is_liked = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_memo);
    let has_signer = *HAS_SIGNER.read();

    // State for counts
    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    // State for author profile
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // State for zap modal
    let mut show_zap_modal = use_signal(|| false);

    // Get first video or return early if none
    if videos.is_empty() {
        return rsx! {
            div { class: "hidden" }
        }
    }

    let first_video = &videos[0];

    // Fetch counts (similar to PhotoCard)
    use_effect(use_reactive(&event_id_counts, move |event_id_for_counts| {
        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                Ok(id) => id,
                Err(_) => return,
            };

            // Fetch reply count
            let reply_filter = Filter::new()
                .kinds(vec![Kind::TextNote, Kind::Comment])
                .event(event_id_parsed)
                .limit(500);

            if let Ok(replies) = client.fetch_events(reply_filter, Duration::from_secs(5)).await {
                reply_count.set(replies.len());
            }

            // Fetch like count
            let like_filter = Filter::new()
                .kind(Kind::Reaction)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(likes) = client.fetch_events(like_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());
                let mut user_has_liked = false;

                if let Some(ref user_pk) = current_user_pubkey {
                    for like in likes.iter() {
                        if like.pubkey.to_string() == *user_pk {
                            user_has_liked = true;
                            break;
                        }
                    }
                }

                like_count.set(likes.len());
                is_liked.set(user_has_liked);
            }

            // Fetch zap amount
            let zap_filter = Filter::new()
                .kind(Kind::from(9735))
                .event(event_id_parsed)
                .limit(500);

            if let Ok(zaps) = client.fetch_events(zap_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());
                let mut user_has_zapped = false;

                let total_sats: u64 = zaps.iter().filter_map(|zap_event| {
                    // Check if this zap is from the current user
                    // Per NIP-57: The uppercase P tag contains the pubkey of the zap sender
                    if let Some(ref user_pk) = current_user_pubkey {
                        // Method 1: Try to get sender from uppercase "P" tag (most common)
                        let mut zap_sender_pubkey = zap_event.tags.iter().find_map(|tag| {
                            let tag_vec = tag.clone().to_vec();
                            if tag_vec.len() >= 2 && tag_vec.first()?.as_str() == "P" {
                                Some(tag_vec.get(1)?.as_str().to_string())
                            } else {
                                None
                            }
                        });

                        // Method 2: Fallback - parse description tag (contains zap request JSON)
                        if zap_sender_pubkey.is_none() {
                            zap_sender_pubkey = zap_event.tags.iter().find_map(|tag| {
                                let tag_vec = tag.clone().to_vec();
                                if tag_vec.first()?.as_str() == "description" {
                                    let zap_request_json = tag_vec.get(1)?.as_str();
                                    if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                                        // The pubkey field in the zap request is the sender
                                        return zap_request.get("pubkey")
                                            .and_then(|p| p.as_str())
                                            .map(|s| s.to_string());
                                    }
                                }
                                None
                            });
                        }

                        if let Some(zap_sender) = zap_sender_pubkey {
                            if zap_sender == *user_pk {
                                user_has_zapped = true;
                            }
                        }
                    }

                    zap_event.tags.iter().find_map(|tag| {
                        let tag_vec = tag.clone().to_vec();
                        if tag_vec.first()?.as_str() == "description" {
                            let zap_request_json = tag_vec.get(1)?.as_str();
                            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                                if let Some(tags) = zap_request.get("tags").and_then(|t| t.as_array()) {
                                    for tag_array in tags {
                                        if let Some(tag_vals) = tag_array.as_array() {
                                            if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                                                if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
                                                    if let Ok(millisats) = amount_str.parse::<u64>() {
                                                        return Some(millisats / 1000);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        None
                    })
                }).sum();

                zap_amount_sats.set(total_sats);
                is_zapped.set(user_has_zapped);
            }
        });
    }));

    // Fetch author's profile metadata
    use_effect(use_reactive(&author_pubkey_for_fetch, move |pubkey_str| {
        spawn(async move {
            let pubkey = match PublicKey::from_hex(&pubkey_str)
                .or_else(|_| PublicKey::from_bech32(&pubkey_str)) {
                Ok(pk) => pk,
                Err(_) => return,
            };

            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            let filter = Filter::new()
                .author(pubkey)
                .kind(Kind::Metadata)
                .limit(1);

            if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await {
                if let Some(event) = events.into_iter().next() {
                    if let Ok(metadata) = nostr_sdk::Metadata::from_json(&event.content) {
                        author_metadata.set(Some(metadata));
                    }
                }
            }
        });
    }));

    // Handle like action
    let handle_like = move |_| {
        if *is_liking.read() || !has_signer {
            return;
        }

        let event_id_clone = event_id_like.clone();
        let author_pk = author_pubkey_for_like.clone();
        is_liking.set(true);

        spawn(async move {
            match publish_reaction(event_id_clone, author_pk, "+".to_string()).await {
                Ok(_) => {
                    is_liked.set(true);
                    let current_count = *like_count.read();
                    like_count.set(current_count + 1);
                }
                Err(e) => {
                    log::error!("Failed to publish like: {}", e);
                }
            }

            is_liking.set(false);
        });
    };

    // Handle bookmark action
    let handle_bookmark = move |_| {
        if *is_bookmarking.read() || !has_signer {
            return;
        }

        let event_id_clone = event_id_bookmark.clone();
        let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);

        is_bookmarking.set(true);

        spawn(async move {
            let result = if currently_bookmarked {
                bookmarks::unbookmark_event(event_id_clone).await
            } else {
                bookmarks::bookmark_event(event_id_clone).await
            };

            match result {
                Ok(_) => {
                    log::info!("Bookmark toggled successfully");
                }
                Err(e) => {
                    log::error!("Failed to toggle bookmark: {}", e);
                }
            }
            is_bookmarking.set(false);
        });
    };

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

    let formatted_duration = first_video.duration.map(format_duration);

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
                            alt: "Avatar"
                        }
                    } else {
                        div {
                            class: "w-12 h-12 rounded-full bg-blue-600 flex items-center justify-center text-white font-bold",
                            "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
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

            // Video player
            div {
                class: "relative bg-black",
                video {
                    class: "w-full max-h-[600px] object-contain",
                    controls: true,
                    preload: "metadata",
                    poster: first_video.thumbnail.as_deref(),
                    source {
                        src: "{first_video.url}",
                        r#type: first_video.mime_type.as_deref().unwrap_or("video/mp4")
                    }
                    // Fallback sources
                    for fallback_url in &first_video.fallback_urls {
                        source {
                            src: "{fallback_url}"
                        }
                    }
                    "Your browser does not support the video tag."
                }

                // Duration badge (bottom right corner)
                if let Some(dur) = &formatted_duration {
                    div {
                        class: "absolute bottom-2 right-2 bg-black/75 text-white text-xs px-2 py-1 rounded",
                        "{dur}"
                    }
                }
            }

            // Title and description
            div {
                class: "p-4",
                if let Some(title_text) = &title {
                    h3 {
                        class: "font-bold text-lg mb-2",
                        "{title_text}"
                    }
                }
                if !description.is_empty() {
                    p {
                        class: "text-sm whitespace-pre-wrap",
                        "{description}"
                    }
                }
            }

            // Action buttons
            div {
                class: "px-4 pb-4 flex items-center gap-6 text-muted-foreground",

                // Reply/Comment
                Link {
                    to: Route::Note { note_id: event_id.clone() },
                    class: "flex items-center gap-2 hover:text-blue-500 transition",
                    MessageCircleIcon { class: "w-5 h-5" }
                    if *reply_count.read() > 0 {
                        span { class: "text-sm", "{reply_count.read()}" }
                    }
                }

                // Like
                button {
                    class: if *is_liked.read() {
                        "flex items-center gap-2 text-red-500 hover:text-red-600 transition"
                    } else {
                        "flex items-center gap-2 hover:text-red-500 transition"
                    },
                    disabled: *is_liking.read() || !has_signer,
                    onclick: handle_like,
                    HeartIcon {
                        class: "w-5 h-5",
                        filled: *is_liked.read()
                    }
                    if *like_count.read() > 0 {
                        span { class: "text-sm", "{like_count.read()}" }
                    }
                }

                // Zap
                button {
                    class: if *is_zapped.read() {
                        "flex items-center gap-2 text-yellow-500 transition"
                    } else {
                        "flex items-center gap-2 hover:text-yellow-500 transition"
                    },
                    disabled: !has_signer,
                    onclick: move |_| show_zap_modal.set(true),
                    ZapIcon {
                        class: "w-5 h-5".to_string(),
                        filled: *is_zapped.read()
                    }
                    if *zap_amount_sats.read() > 0 {
                        span { class: "text-sm", "{zap_amount_sats.read()}" }
                    }
                }

                // Bookmark
                button {
                    class: if is_bookmarked {
                        "flex items-center gap-2 text-blue-500 hover:text-blue-600 transition"
                    } else {
                        "flex items-center gap-2 hover:text-blue-500 transition"
                    },
                    disabled: *is_bookmarking.read() || !has_signer,
                    onclick: handle_bookmark,
                    BookmarkIcon {
                        class: "w-5 h-5",
                        filled: is_bookmarked
                    }
                }
            }
        }

        // Zap modal
        if *show_zap_modal.read() {
            ZapModal {
                recipient_pubkey: author_pubkey.clone(),
                recipient_name: author_name.clone(),
                lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                event_id: Some(event_id.clone()),
                on_close: move |_| {
                    show_zap_modal.set(false);
                }
            }
        }
    }
}
