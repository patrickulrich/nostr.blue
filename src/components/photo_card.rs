use dioxus::prelude::*;
use nostr_sdk::{Event, PublicKey, Filter, Kind, FromBech32};
use crate::routes::Route;
use crate::stores::nostr_client::{publish_reaction, publish_note, get_client, HAS_SIGNER};
use crate::stores::bookmarks;
use crate::components::icons::{HeartIcon, MessageCircleIcon, BookmarkIcon, ZapIcon};
use crate::components::ZapModal;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ImageMeta {
    pub url: String,
    pub alt: Option<String>,
    pub blurhash: Option<String>,
    pub dim: Option<(u32, u32)>,
}

/// Parse imeta tags from NIP-68 picture events
pub fn parse_imeta_tags(event: &Event) -> Vec<ImageMeta> {
    let mut images = Vec::new();

    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            let mut image = ImageMeta {
                url: String::new(),
                alt: None,
                blurhash: None,
                dim: None,
            };

            // Parse imeta tag fields
            for field in tag_vec.iter().skip(1) {
                if let Some((key, value)) = field.split_once(' ') {
                    match key {
                        "url" => image.url = value.to_string(),
                        "alt" => image.alt = Some(value.to_string()),
                        "blurhash" => image.blurhash = Some(value.to_string()),
                        "dim" => {
                            if let Some((w, h)) = value.split_once('x') {
                                if let (Ok(width), Ok(height)) = (w.parse(), h.parse()) {
                                    image.dim = Some((width, height));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Only add if we have a URL
            if !image.url.is_empty() {
                images.push(image);
            }
        }
    }

    images
}

/// Get the title from NIP-68 picture events
pub fn get_title(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") {
            return tag_vec.get(1).map(|s| s.to_string());
        }
    }
    None
}

#[component]
pub fn PhotoCard(event: Event) -> Element {
    let images = parse_imeta_tags(&event);
    let title = get_title(&event);
    let description = &event.content;

    // Clone values for closures
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let author_pubkey_like = author_pubkey.clone();
    let author_pubkey_comment = author_pubkey.clone();
    let author_pubkey_comment_btn = author_pubkey.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();
    let event_id_comment = event_id.clone();
    let event_id_comment_btn = event_id.clone();
    let event_id_link = event_id.clone();

    // Clone images for use in closures
    let images_carousel = images.clone();

    // State for interactions
    let mut is_liking = use_signal(|| false);
    let mut is_liked = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = use_memo(move || bookmarks::is_bookmarked(&event_id_memo));
    let has_signer = *HAS_SIGNER.read();

    // State for current image (carousel)
    let mut current_image_index = use_signal(|| 0usize);

    // State for counts
    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    // State for author profile
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // State for comment composer
    let mut comment_text = use_signal(|| String::new());
    let mut is_posting_comment = use_signal(|| false);

    // State for zap modal
    let mut show_zap_modal = use_signal(|| false);

    // Get first image or return early if none
    if images.is_empty() {
        return rsx! {
            div { class: "hidden" }
        }
    }

    // Fetch counts - only run once per event_id
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

            // Fetch reply count (kind 1 events with 'e' tag referencing this event)
            let reply_filter = Filter::new()
                .kind(Kind::TextNote)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(replies) = client.fetch_events(reply_filter, Duration::from_secs(5)).await {
                reply_count.set(replies.len());
            }

            // Fetch like count (kind 7 reactions)
            let like_filter = Filter::new()
                .kind(Kind::Reaction)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(likes) = client.fetch_events(like_filter, Duration::from_secs(5)).await {
                like_count.set(likes.len());
            }

            // Fetch zap receipts (kind 9735) and calculate total
            let zap_filter = Filter::new()
                .kind(Kind::from(9735))
                .event(event_id_parsed)
                .limit(500);

            if let Ok(zaps) = client.fetch_events(zap_filter, Duration::from_secs(5)).await {
                let total_sats: u64 = zaps.iter().filter_map(|zap_event| {
                    // Look for the description tag which contains the zap request
                    zap_event.tags.iter().find_map(|tag| {
                        let tag_vec = tag.clone().to_vec();
                        if tag_vec.first()?.as_str() == "description" {
                            // Parse the JSON zap request
                            let zap_request_json = tag_vec.get(1)?.as_str();
                            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                                // Find the amount tag in the zap request
                                if let Some(tags) = zap_request.get("tags").and_then(|t| t.as_array()) {
                                    for tag_array in tags {
                                        if let Some(tag_vals) = tag_array.as_array() {
                                            if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                                                if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
                                                    // Amount is in millisats, convert to sats
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
            }
        });
    }));

    // Fetch author's profile metadata - only run once per pubkey
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
                    if let Ok(metadata) = serde_json::from_str::<nostr_sdk::Metadata>(&event.content) {
                        author_metadata.set(Some(metadata));
                    }
                }
            }
        });
    }));

    // Format timestamp
    let timestamp = format_timestamp(created_at.as_u64());

    // Get display name and picture from metadata
    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            if author_pubkey.len() > 16 {
                format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..])
            } else {
                author_pubkey.clone()
            }
        });

    let picture_url = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    // Navigation to photo detail page
    let nav = use_navigator();
    let event_id_nav = event_id.clone();

    rsx! {
        div {
            class: "border-b border-border bg-background mb-4 cursor-pointer hover:bg-accent/5 transition",
            onclick: move |_| {
                nav.push(Route::PhotoDetail { photo_id: event_id_nav.clone() });
            },

            // Author header
            div {
                class: "p-3 flex items-center gap-3",
                Link {
                    to: Route::Profile { pubkey: author_pubkey.clone() },
                    class: "flex-shrink-0",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    if let Some(pic) = picture_url {
                        img {
                            class: "w-8 h-8 rounded-full object-cover",
                            src: "{pic}",
                            alt: "Profile"
                        }
                    } else {
                        div {
                            class: "w-8 h-8 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold text-sm",
                            "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                        }
                    }
                }
                div {
                    class: "flex-1 min-w-0",
                    Link {
                        to: Route::Profile { pubkey: author_pubkey.clone() },
                        class: "font-semibold hover:underline text-sm",
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        "{display_name}"
                    }
                }
                span {
                    class: "text-xs text-muted-foreground",
                    "{timestamp}"
                }
            }

            // Image display
            div {
                class: "relative bg-black",
                img {
                    class: "w-full max-h-[600px] object-contain",
                    src: "{images[*current_image_index.read()].url}",
                    alt: "{images[*current_image_index.read()].alt.as_deref().unwrap_or(\"Photo\")}"
                }

                // Multiple images carousel indicators
                if images.len() > 1 {
                    div {
                        class: "absolute bottom-3 left-1/2 -translate-x-1/2 flex gap-1.5",
                        for (idx, _) in images.iter().enumerate() {
                            button {
                                class: if idx == *current_image_index.read() {
                                    "w-2 h-2 rounded-full bg-white"
                                } else {
                                    "w-2 h-2 rounded-full bg-white/50"
                                },
                                onclick: move |_| current_image_index.set(idx),
                            }
                        }
                    }

                    // Previous/Next buttons
                    if *current_image_index.read() > 0 {
                        button {
                            class: "absolute left-2 top-1/2 -translate-y-1/2 bg-black/50 text-white rounded-full p-2 hover:bg-black/70 transition",
                            onclick: move |_| {
                                let current = *current_image_index.read();
                                if current > 0 {
                                    current_image_index.set(current - 1);
                                }
                            },
                            "‹"
                        }
                    }
                    if *current_image_index.read() < images_carousel.len() - 1 {
                        button {
                            class: "absolute right-2 top-1/2 -translate-y-1/2 bg-black/50 text-white rounded-full p-2 hover:bg-black/70 transition",
                            onclick: move |_| {
                                let current = *current_image_index.read();
                                if current < images_carousel.len() - 1 {
                                    current_image_index.set(current + 1);
                                }
                            },
                            "›"
                        }
                    }
                }
            }

            // Action buttons
            div {
                class: "flex items-center gap-4 px-3 py-2",

                // Like button
                button {
                    class: if *is_liked.read() {
                        "flex items-center gap-1 text-red-500"
                    } else {
                        "flex items-center gap-1 hover:text-red-500 transition"
                    },
                    disabled: !has_signer || *is_liking.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        if !has_signer || *is_liking.read() {
                            return;
                        }

                        let currently_liked = *is_liked.read();
                        let event_id_clone = event_id_like.clone();
                        let author_pubkey_clone = author_pubkey_like.clone();

                        is_liking.set(true);

                        if currently_liked {
                            is_liked.set(false);
                            let current_count = *like_count.read();
                            like_count.set(current_count.saturating_sub(1));
                            is_liking.set(false);
                        } else {
                            spawn(async move {
                                match publish_reaction(event_id_clone, author_pubkey_clone, "+".to_string()).await {
                                    Ok(_) => {
                                        is_liked.set(true);
                                        let current_count = *like_count.read();
                                        like_count.set(current_count + 1);
                                        is_liking.set(false);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to like: {}", e);
                                        is_liking.set(false);
                                    }
                                }
                            });
                        }
                    },
                    HeartIcon {
                        class: "w-6 h-6".to_string(),
                        filled: *is_liked.read()
                    }
                }

                // Comment button (does nothing, just visual)
                button {
                    class: "flex items-center gap-1 hover:text-blue-500 transition",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    MessageCircleIcon {
                        class: "w-6 h-6".to_string(),
                        filled: false
                    }
                }

                // Zap button (only show if author has lightning address)
                {
                    let has_lightning = author_metadata.read().as_ref()
                        .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                        .is_some();

                    if has_lightning {
                        rsx! {
                            button {
                                class: "flex items-center gap-1 hover:text-yellow-500 transition",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_zap_modal.set(true);
                                },
                                ZapIcon {
                                    class: "w-6 h-6".to_string(),
                                    filled: false
                                }
                                span {
                                    class: "text-sm",
                                    {
                                        let amount = *zap_amount_sats.read();
                                        if amount > 0 {
                                            format_sats(amount)
                                        } else {
                                            "".to_string()
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }

                // Bookmark button
                button {
                    class: if *is_bookmarked.read() {
                        "flex items-center gap-1 text-blue-500"
                    } else {
                        "flex items-center gap-1 hover:text-blue-500 transition ml-auto"
                    },
                    disabled: !has_signer || *is_bookmarking.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        if !has_signer || *is_bookmarking.read() {
                            return;
                        }

                        let event_id_clone = event_id_bookmark.clone();
                        is_bookmarking.set(true);

                        spawn(async move {
                            let result = if bookmarks::is_bookmarked(&event_id_clone) {
                                bookmarks::unbookmark_event(event_id_clone).await
                            } else {
                                bookmarks::bookmark_event(event_id_clone).await
                            };

                            match result {
                                Ok(_) => {
                                    is_bookmarking.set(false);
                                }
                                Err(e) => {
                                    log::error!("Failed to bookmark: {}", e);
                                    is_bookmarking.set(false);
                                }
                            }
                        });
                    },
                    BookmarkIcon {
                        class: "w-6 h-6".to_string(),
                        filled: *is_bookmarked.read()
                    }
                }
            }

            // Like count
            div {
                class: "px-3 pb-2",
                if *like_count.read() > 0 {
                    span {
                        class: "font-semibold text-sm",
                        {
                            let count = *like_count.read();
                            if count == 1 {
                                format!("{} like", count)
                            } else {
                                format!("{} likes", count)
                            }
                        }
                    }
                }
            }

            // Caption
            div {
                class: "px-3 pb-2",
                if let Some(title_text) = &title {
                    div {
                        class: "mb-1",
                        span {
                            class: "font-semibold text-sm mr-2",
                            "{display_name}"
                        }
                        span {
                            class: "font-bold",
                            "{title_text}"
                        }
                    }
                }
                if !description.is_empty() {
                    div {
                        if title.is_none() {
                            span {
                                class: "font-semibold text-sm mr-2",
                                "{display_name}"
                            }
                        }
                        span {
                            class: "text-sm",
                            "{description}"
                        }
                    }
                }
            }

            // Comment count
            if *reply_count.read() > 0 {
                Link {
                    to: Route::PhotoDetail { photo_id: event_id_link.clone() },
                    class: "px-3 pb-2 block text-sm text-muted-foreground hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    {
                        let count = *reply_count.read();
                        if count == 1 {
                            format!("View 1 comment")
                        } else {
                            format!("View all {} comments", count)
                        }
                    }
                }
            }

            // Add comment
            if has_signer {
                div {
                    class: "px-3 pb-3 flex items-center gap-2",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    input {
                        class: "flex-1 bg-transparent border-none outline-none text-sm placeholder:text-muted-foreground",
                        r#type: "text",
                        placeholder: "Add a comment...",
                        value: "{comment_text}",
                        oninput: move |evt| comment_text.set(evt.value()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Enter && !comment_text.read().is_empty() && !*is_posting_comment.read() {
                                let text = comment_text.read().clone();
                                let event_id_clone = event_id_comment.clone();
                                let author_clone = author_pubkey_comment.clone();

                                is_posting_comment.set(true);
                                comment_text.set(String::new());

                                spawn(async move {
                                    // Create reply tags (e tag for event, p tag for author)
                                    let tags = vec![
                                        vec!["e".to_string(), event_id_clone],
                                        vec!["p".to_string(), author_clone],
                                    ];

                                    match publish_note(text, tags).await {
                                        Ok(_) => {
                                            let current_count = *reply_count.read();
                                            reply_count.set(current_count + 1);
                                            is_posting_comment.set(false);
                                        }
                                        Err(e) => {
                                            log::error!("Failed to post comment: {}", e);
                                            is_posting_comment.set(false);
                                        }
                                    }
                                });
                            }
                        }
                    }
                    if !comment_text.read().is_empty() {
                        button {
                            class: "text-blue-500 font-semibold text-sm hover:text-blue-600 disabled:opacity-50",
                            disabled: *is_posting_comment.read(),
                            onclick: move |_| {
                                if comment_text.read().is_empty() || *is_posting_comment.read() {
                                    return;
                                }

                                let text = comment_text.read().clone();
                                let event_id_clone = event_id_comment_btn.clone();
                                let author_clone = author_pubkey_comment_btn.clone();

                                is_posting_comment.set(true);
                                comment_text.set(String::new());

                                spawn(async move {
                                    // Create reply tags (e tag for event, p tag for author)
                                    let tags = vec![
                                        vec!["e".to_string(), event_id_clone],
                                        vec!["p".to_string(), author_clone],
                                    ];

                                    match publish_note(text, tags).await {
                                        Ok(_) => {
                                            let current_count = *reply_count.read();
                                            reply_count.set(current_count + 1);
                                            is_posting_comment.set(false);
                                        }
                                        Err(e) => {
                                            log::error!("Failed to post comment: {}", e);
                                            is_posting_comment.set(false);
                                        }
                                    }
                                });
                            },
                            "Post"
                        }
                    }
                }
            }
        }

        // Zap modal
        if *show_zap_modal.read() {
            ZapModal {
                recipient_pubkey: author_pubkey.clone(),
                recipient_name: display_name.clone(),
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

/// Format sats with k/M suffixes
fn format_sats(sats: u64) -> String {
    if sats >= 1_000_000 {
        format!("{}M", sats / 1_000_000)
    } else if sats >= 1_000 {
        format!("{}k", sats / 1_000)
    } else {
        sats.to_string()
    }
}

// Helper to format timestamp
fn format_timestamp(timestamp: u64) -> String {
    use nostr_sdk::Timestamp;

    let now = Timestamp::now().as_u64();
    let diff = now.saturating_sub(timestamp);

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m", diff / 60),
        3600..=86399 => format!("{}h", diff / 3600),
        86400..=604799 => format!("{}d", diff / 86400),
        _ => format!("{}w", diff / 604800),
    }
}
