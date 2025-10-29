use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, PublicKey, Filter, Kind, FromBech32, ToBech32};
use crate::routes::Route;
use crate::stores::nostr_client::{publish_reaction, publish_repost, HAS_SIGNER, get_client};
use crate::stores::bookmarks;
use crate::components::{RichContent, ReplyComposer, ZapModal};
use crate::components::icons::{HeartIcon, MessageCircleIcon, Repeat2Icon, BookmarkIcon, ZapIcon, ShareIcon};
use std::time::Duration;

#[component]
pub fn NoteCard(event: NostrEvent) -> Element {
    // Clone values that will be used in multiple closures
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_repost = author_pubkey.clone();
    let author_pubkey_like = author_pubkey.clone();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let content = event.content.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_repost = event_id.clone();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();

    // State for interactions
    let mut is_liking = use_signal(|| false);
    let mut is_liked = use_signal(|| false);
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = use_memo(move || bookmarks::is_bookmarked(&event_id_memo));
    let has_signer = *HAS_SIGNER.read();

    // State for counts
    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    // State for author profile
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch counts - consolidated into a single batched fetch
    use_effect(move || {
        let event_id_for_counts = event_id_counts.clone();
        spawn(async move {
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                Ok(id) => id,
                Err(_) => return,
            };

            // Create a combined filter for all interaction kinds (replies, likes, reposts, zaps)
            let combined_filter = Filter::new()
                .kinds(vec![
                    Kind::TextNote,      // kind 1 - replies
                    Kind::Reaction,      // kind 7 - likes
                    Kind::Repost,        // kind 6 - reposts
                    Kind::from(9735),    // kind 9735 - zaps
                ])
                .event(event_id_parsed)
                .limit(2000); // Increased limit to accommodate all event types

            // Single fetch for all interaction types
            if let Ok(events) = client.fetch_events(combined_filter, Duration::from_secs(5)).await {
                // Partition events by kind
                let mut replies = 0;
                let mut likes = 0;
                let mut reposts = 0;
                let mut total_sats = 0u64;

                for event in events {
                    match event.kind {
                        Kind::TextNote => replies += 1,
                        Kind::Reaction => likes += 1,
                        Kind::Repost => reposts += 1,
                        kind if kind == Kind::from(9735) => {
                            // Calculate zap amount
                            if let Some(amount) = event.tags.iter().find_map(|tag| {
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
                            }) {
                                total_sats += amount;
                            }
                        }
                        _ => {}
                    }
                }

                // Update all counts at once
                reply_count.set(replies.min(500));
                like_count.set(likes.min(500));
                repost_count.set(reposts.min(500));
                zap_amount_sats.set(total_sats);
            }
        });
    });

    // Fetch author's profile metadata
    use_effect(move || {
        let pubkey_str = author_pubkey_for_fetch.clone();

        spawn(async move {
            // Parse pubkey
            let pubkey = match PublicKey::from_hex(&pubkey_str)
                .or_else(|_| PublicKey::from_bech32(&pubkey_str)) {
                Ok(pk) => pk,
                Err(_) => return,
            };

            // Get client
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            // Fetch metadata event (kind 0)
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
    });

    // Format timestamp
    let timestamp = format_timestamp(created_at.as_u64());

    // Get display name and picture from metadata or fallback
    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            // Fallback to truncated pubkey
            if author_pubkey.len() > 16 {
                format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..])
            } else {
                author_pubkey.clone()
            }
        });

    let username = author_metadata.read().as_ref()
        .and_then(|m| m.name.clone())
        .unwrap_or_else(|| {
            // Truncated npub
            if let Ok(pk) = PublicKey::from_hex(&author_pubkey) {
                let npub = pk.to_bech32().expect("to_bech32 is infallible");
                if npub.len() > 18 {
                    format!("{}...{}", &npub[..12], &npub[npub.len()-6..])
                } else {
                    npub
                }
            } else {
                "unknown".to_string()
            }
        });

    let profile_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    // Compute dynamic class strings
    let like_button_class = if *is_liked.read() {
        "flex items-center text-red-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-red-500 transition"
    };

    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };

    let bookmark_button_class = if *is_bookmarked.read() {
        "flex items-center text-blue-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };

    let nav = use_navigator();
    let event_id_nav = event_id.clone();

    rsx! {
        article {
            class: "border-b border-border p-4 hover:bg-accent/50 transition-colors cursor-pointer",
            onclick: move |_| {
                nav.push(Route::Note { note_id: event_id_nav.clone() });
            },

            div {
                class: "flex gap-3",

                // Avatar
                div {
                    class: "flex-shrink-0",
                    Link {
                        to: Route::Profile { pubkey: author_pubkey.clone() },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        if let Some(picture_url) = &profile_picture {
                            img {
                                class: "w-12 h-12 rounded-full object-cover",
                                src: "{picture_url}",
                                alt: "Profile picture"
                            }
                        } else {
                            div {
                                class: "w-12 h-12 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold text-lg",
                                "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                    }
                }

                // Content
                div {
                    class: "flex-1 min-w-0",

                    // Header
                    div {
                        class: "flex items-center gap-2 mb-1 flex-wrap",
                        Link {
                            to: Route::Profile { pubkey: author_pubkey.clone() },
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            class: "font-bold hover:underline",
                            "{display_name}"
                        }
                        span {
                            class: "text-muted-foreground text-sm",
                            "@{username}"
                        }
                        span {
                            class: "text-muted-foreground text-sm",
                            "·"
                        }
                        span {
                            class: "text-muted-foreground text-sm",
                            "{timestamp}"
                        }
                    }

                    // Post content
                    div {
                        class: "mb-3",
                        RichContent {
                            content: content.clone(),
                            tags: event.tags.iter().cloned().collect()
                        }
                    }

                    // Action buttons
                    div {
                        class: "flex items-center justify-between max-w-md mt-2 -ml-2",

                        // Reply button
                        button {
                            class: "flex items-center gap-1 hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_reply_modal.set(true);
                            },
                            MessageCircleIcon {
                                class: "h-4 w-4".to_string(),
                                filled: false
                            }
                            span {
                                class: "text-xs",
                                {
                                    let count = *reply_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else if count > 0 {
                                        count.to_string()
                                    } else {
                                        "".to_string()
                                    }
                                }
                            }
                        }

                        // Repost button
                        button {
                            class: "{repost_button_class} hover:bg-green-500/10 gap-1 px-2 py-1.5 rounded",
                            disabled: !has_signer || *is_reposting.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();

                                if !has_signer || *is_reposting.read() {
                                    return;
                                }

                                let event_id_clone = event_id_repost.clone();
                                let author_pubkey_clone = author_pubkey_repost.clone();

                                is_reposting.set(true);

                                spawn(async move {
                                    match publish_repost(event_id_clone, author_pubkey_clone, None).await {
                                        Ok(repost_id) => {
                                            log::info!("Reposted event, repost ID: {}", repost_id);
                                            is_reposted.set(true);
                                            is_reposting.set(false);
                                        }
                                        Err(e) => {
                                            log::error!("Failed to repost event: {}", e);
                                            is_reposting.set(false);
                                        }
                                    }
                                });
                            },
                            Repeat2Icon {
                                class: "h-4 w-4".to_string(),
                                filled: false
                            }
                            span {
                                class: "text-xs",
                                {
                                    let count = *repost_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else if count > 0 {
                                        count.to_string()
                                    } else {
                                        "".to_string()
                                    }
                                }
                            }
                        }

                        // Like button
                        button {
                            class: "{like_button_class} hover:bg-red-500/10 gap-1 px-2 py-1.5 rounded",
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
                                    // Unlike - just toggle state locally (Nostr doesn't have unlike)
                                    is_liked.set(false);
                                    let current_count = *like_count.read();
                                    like_count.set(current_count.saturating_sub(1));
                                    is_liking.set(false);
                                } else {
                                    // Like - publish reaction
                                    spawn(async move {
                                        match publish_reaction(event_id_clone, author_pubkey_clone, "+".to_string()).await {
                                            Ok(reaction_id) => {
                                                log::info!("Liked event, reaction ID: {}", reaction_id);
                                                is_liked.set(true);
                                                // Increment like count locally
                                                let current_count = *like_count.read();
                                                like_count.set(current_count.saturating_add(1));
                                                is_liking.set(false);
                                            }
                                            Err(e) => {
                                                log::error!("Failed to like event: {}", e);
                                                is_liking.set(false);
                                            }
                                        }
                                    });
                                }
                            },
                            HeartIcon {
                                class: "h-4 w-4".to_string(),
                                filled: *is_liked.read()
                            }
                            span {
                                class: "text-xs",
                                {
                                    let count = *like_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else if count > 0 {
                                        count.to_string()
                                    } else {
                                        "".to_string()
                                    }
                                }
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
                                        class: "flex items-center gap-1 text-muted-foreground hover:text-yellow-500 hover:bg-yellow-500/10 transition px-2 py-1.5 rounded",
                                        onclick: move |e: MouseEvent| {
                                            e.stop_propagation();
                                            show_zap_modal.set(true);
                                        },
                                        ZapIcon {
                                            class: "h-4 w-4".to_string(),
                                            filled: false
                                        }
                                        span {
                                            class: "text-xs",
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
                            class: "{bookmark_button_class} hover:bg-blue-500/10 px-2 py-1.5 rounded",
                            disabled: !has_signer || *is_bookmarking.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();

                                if !has_signer || *is_bookmarking.read() {
                                    return;
                                }

                                let event_id_clone = event_id_bookmark.clone();
                                let currently_bookmarked = *is_bookmarked.read();

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
                            },
                            BookmarkIcon {
                                class: "h-4 w-4".to_string(),
                                filled: *is_bookmarked.read()
                            }
                        }

                        // Share button
                        button {
                            class: "flex items-center text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 px-2 py-1.5 rounded transition",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                // TODO: Implement share
                            },
                            ShareIcon {
                                class: "h-4 w-4".to_string(),
                                filled: false
                            }
                        }
                    }
                }
            }
        }

        // Reply modal
        if *show_reply_modal.read() {
            ReplyComposer {
                reply_to: event.clone(),
                on_close: move |_| {
                    show_reply_modal.set(false);
                },
                on_success: move |_| {
                    show_reply_modal.set(false);
                    // Optionally refresh feed here
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

// Helper function to format timestamp
fn format_timestamp(unix_timestamp: u64) -> String {
    // Use JavaScript's Date.now() for WASM compatibility
    let now = (js_sys::Date::now() / 1000.0) as u64;

    let diff = now.saturating_sub(unix_timestamp);

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m", diff / 60),
        3600..=86399 => format!("{}h", diff / 3600),
        86400..=604799 => format!("{}d", diff / 86400),
        _ => format!("{}w", diff / 604800),
    }
}

// Helper function to format sats amounts
fn format_sats(sats: u64) -> String {
    if sats >= 1_000_000 {
        format!("{}M", sats / 1_000_000)
    } else if sats >= 1_000 {
        format!("{}k", sats / 1_000)
    } else {
        sats.to_string()
    }
}

// Skeleton loader for NoteCard
#[component]
pub fn NoteCardSkeleton() -> Element {
    rsx! {
        div {
            class: "border-b border-gray-200 dark:border-gray-800 p-4 animate-pulse",
            div {
                class: "flex gap-3",

                // Avatar skeleton
                div {
                    class: "w-12 h-12 rounded-full bg-gray-300 dark:bg-gray-700"
                }

                // Content skeleton
                div {
                    class: "flex-1 space-y-2",
                    div {
                        class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/4"
                    }
                    div {
                        class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-3/4"
                    }
                    div {
                        class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/2"
                    }
                }
            }
        }
    }
}
