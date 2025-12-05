use dioxus::prelude::*;
use dioxus::events::MediaData;
use dioxus::web::WebEventExt;
use wasm_bindgen::JsCast;
use crate::utils::ThreadNode;
use crate::components::{RichContent, ReplyComposer, ZapModal, ReactionButton};
use crate::routes::Route;
use crate::stores::nostr_client::{self, publish_repost, HAS_SIGNER, get_client};
use crate::stores::voice_messages_store;
use crate::hooks::use_reaction;
use crate::stores::bookmarks;
use crate::stores::signer::SIGNER_INFO;
use crate::components::icons::{MessageCircleIcon, Repeat2Icon, BookmarkIcon, ZapIcon, ShareIcon};
use crate::utils::time::format_relative_time_ex;
use crate::utils::format_sats_compact;
use nostr_sdk::{Metadata, Filter, Kind};
use nostr_sdk::prelude::NostrDatabaseExt;
use std::time::Duration;

/// Check if an event is a voice message
fn is_voice_message(event: &nostr_sdk::Event) -> bool {
    event.kind == Kind::VoiceMessage || event.kind == Kind::VoiceMessageReply
}

const MAX_DEPTH: usize = 8; // Limit nesting to prevent excessive indentation

#[component]
pub fn ThreadedComment(node: ThreadNode, depth: usize) -> Element {
    let event = &node.event;
    let children = &node.children;

    // Clone values needed for closures
    let event_id = event.id.to_string();
    let event_id_like = event_id.clone();
    let event_id_repost = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();
    let author_pubkey = event.pubkey;
    let author_pubkey_str = author_pubkey.to_string();
    let author_pubkey_like = author_pubkey_str.clone();
    let author_pubkey_repost = author_pubkey_str.clone();

    let mut author_metadata = use_signal(|| None::<Metadata>);

    // State for interactions
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = use_memo(move || bookmarks::is_bookmarked(&event_id_memo));
    let has_signer = *HAS_SIGNER.read();
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);

    // Reaction hook - handles like state with optimistic updates and toggle support
    let reaction = use_reaction(
        event_id_like.clone(),
        author_pubkey_like.clone(),
        None, // No precomputed counts for comments
    );

    // State for counts (likes handled by use_reaction hook)
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    // Voice message state
    let is_voice = is_voice_message(event);
    let audio_url = if is_voice { event.content.clone() } else { String::new() };
    let audio_id = format!("voice-comment-{}", event_id);
    let mut duration = use_signal(|| 0.0f64);
    let mut current_time = use_signal(|| 0.0f64);
    let event_id_parsed = event.id;

    // Parse imeta tags for duration per NIP-92/NIP-94
    let imeta_duration: Option<f64> = if is_voice {
        event.tags.iter()
            .find(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("imeta"))
            .and_then(|tag| {
                for field in tag.as_slice().iter().skip(1) {
                    let field_str = field.as_str();
                    if field_str.starts_with("duration ") {
                        if let Ok(d) = field_str[9..].parse::<f64>() {
                            return Some(d);
                        }
                    }
                }
                None
            })
    } else {
        None
    };

    // Fetch author metadata
    use_effect(move || {
        spawn(async move {
            if let Some(client) = nostr_client::NOSTR_CLIENT.read().as_ref() {
                // Check database first (instant, no network)
                if let Ok(Some(metadata)) = client.database().metadata(author_pubkey).await {
                    author_metadata.set(Some(metadata));
                    return;
                }

                // If not in database, fetch from relays (auto-caches to database)
                if let Ok(Some(metadata)) = client.fetch_metadata(author_pubkey, std::time::Duration::from_secs(5)).await {
                    author_metadata.set(Some(metadata));
                }
            }
        });
    });

    // Fetch counts
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

            // Fetch reply count
            let reply_filter = Filter::new()
                .kind(Kind::TextNote)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(replies) = client.fetch_events(reply_filter, Duration::from_secs(5)).await {
                reply_count.set(replies.len());
            }

            // Note: Reactions/likes are handled by use_reaction hook

            // Fetch repost count
            let repost_filter = Filter::new()
                .kind(Kind::Repost)
                .event(event_id_parsed)
                .limit(500);

            if let Ok(reposts) = client.fetch_events(repost_filter, Duration::from_secs(5)).await {
                // Get current user's pubkey to check if they've already reposted
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());
                let mut user_has_reposted = false;

                // Check if current user has reposted
                if let Some(ref user_pk) = current_user_pubkey {
                    for repost in reposts.iter() {
                        if repost.pubkey.to_string() == *user_pk {
                            user_has_reposted = true;
                            break;
                        }
                    }
                }

                repost_count.set(reposts.len());
                is_reposted.set(user_has_reposted);
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
    });

    // Button class helpers
    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 hover:text-green-600 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };

    let bookmark_button_class = if *is_bookmarked.read() {
        "flex items-center text-blue-500 hover:text-blue-600 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };

    // Voice message handlers
    let audio_id_for_effect = audio_id.clone();
    let audio_id_for_handlers = audio_id.clone();

    // Control audio element based on playback state (for voice messages)
    use_effect(move || {
        if !is_voice {
            return;
        }
        let global_state = voice_messages_store::VOICE_PLAYBACK.read();
        let is_playing = global_state.currently_playing == Some(event_id_parsed);
        let audio_id_clone = audio_id_for_effect.clone();

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };
        let audio_element = match document.get_element_by_id(&audio_id_clone) {
            Some(el) => el,
            None => return,
        };
        let audio = match audio_element.dyn_ref::<web_sys::HtmlAudioElement>() {
            Some(a) => a,
            None => return,
        };

        if is_playing {
            let _ = audio.play();
        } else {
            let _ = audio.pause();
        }
    });

    // Voice message event handlers
    let handle_timeupdate = move |evt: Event<MediaData>| {
        if let Some(target) = evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let time = audio.current_time();
                current_time.set(time);
                if voice_messages_store::is_playing(&event_id_parsed) {
                    voice_messages_store::set_current_time(time);
                }
            }
        }
    };

    let handle_loadedmetadata = move |evt: Event<MediaData>| {
        if let Some(target) = evt.data.as_web_event().target() {
            if let Some(audio) = target.dyn_ref::<web_sys::HtmlAudioElement>() {
                let dur = audio.duration();
                if !dur.is_nan() {
                    duration.set(dur);
                    if voice_messages_store::is_playing(&event_id_parsed) {
                        voice_messages_store::set_duration(dur);
                    }
                }
            }
        }
    };

    let handle_ended = move |_| {
        voice_messages_store::pause_voice_message();
        current_time.set(0.0);
    };

    let toggle_play = move |_| {
        voice_messages_store::toggle_voice_message(event_id_parsed);
    };

    // Voice player display values
    let duration_val = imeta_duration.unwrap_or(*duration.read());
    let current_time_str = voice_messages_store::format_time(*current_time.read());
    let duration_str = voice_messages_store::format_time(duration_val);
    let progress_percent = if duration_val > 0.0 {
        *current_time.read() / duration_val * 100.0
    } else {
        0.0
    };

    // Calculate indentation (left margin)
    let indent_level = depth.min(MAX_DEPTH);
    let margin_left = indent_level * 4; // 4px per level

    // Clone event_id for navigation
    let event_id_nav = event.id.to_hex();
    let nav = use_navigator();

    rsx! {
        div {
            class: "comment-thread",
            style: "margin-left: {margin_left}px;",

            // Comment card - clickable to navigate to thread
            div {
                class: "border-l-2 border-border pl-3 py-2 hover:bg-accent/20 transition cursor-pointer",
                onclick: {
                    let event_id_click = event_id_nav.clone();
                    let navigator = nav.clone();
                    move |_| {
                        // Don't navigate if clicking on interactive elements
                        // The event will be stopped by buttons/links
                        navigator.push(Route::Note { note_id: event_id_click.clone(), from_voice: None });
                    }
                },

                // Author info
                div {
                    class: "flex items-start gap-2 mb-2",

                    // Avatar
                    Link {
                        to: Route::Profile { pubkey: author_pubkey.to_string() },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        if let Some(metadata) = author_metadata.read().as_ref() {
                            if let Some(picture) = &metadata.picture {
                                img {
                                    class: "w-8 h-8 rounded-full flex-shrink-0",
                                    src: "{picture}",
                                    alt: "Avatar",
                                    loading: "lazy"
                                }
                            } else {
                                div {
                                    class: "w-8 h-8 rounded-full bg-blue-500 flex items-center justify-center text-white text-xs font-bold flex-shrink-0",
                                    if let Some(name) = &metadata.name {
                                        "{name.chars().next().unwrap_or('?').to_uppercase()}"
                                    } else {
                                        "?"
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "w-8 h-8 rounded-full bg-gray-400 flex items-center justify-center text-white text-xs flex-shrink-0",
                                "?"
                            }
                        }
                    }

                    // Author name and timestamp
                    div {
                        class: "flex-1 min-w-0",

                        div {
                            class: "flex items-baseline gap-2 flex-wrap",

                            Link {
                                to: Route::Profile { pubkey: author_pubkey.to_string() },
                                class: "font-semibold text-sm hover:underline truncate",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                if let Some(metadata) = author_metadata.read().as_ref() {
                                    if let Some(display_name) = &metadata.display_name {
                                        "{display_name}"
                                    } else if let Some(name) = &metadata.name {
                                        "{name}"
                                    } else {
                                        span {
                                            class: "font-mono text-xs",
                                            "{author_pubkey.to_string().chars().take(16).collect::<String>()}..."
                                        }
                                    }
                                } else {
                                    span {
                                        class: "font-mono text-xs",
                                        "{author_pubkey.to_string().chars().take(16).collect::<String>()}..."
                                    }
                                }
                            }

                            span {
                                class: "text-xs text-muted-foreground",
                                "{format_relative_time_ex(event.created_at, true, true)}"
                            }
                        }

                        // Comment content - voice player or text
                        div {
                            class: "text-sm mt-1",
                            if is_voice {
                                // Inline voice player for voice message replies
                                div {
                                    class: "my-2",
                                    // Hidden audio element
                                    audio {
                                        id: "{audio_id_for_handlers}",
                                        src: "{audio_url}",
                                        preload: "metadata",
                                        style: "display: none;",
                                        ontimeupdate: handle_timeupdate,
                                        onloadedmetadata: handle_loadedmetadata,
                                        onended: handle_ended,
                                    }
                                    // Compact player controls
                                    div {
                                        class: "flex items-center gap-3 bg-muted/30 rounded-lg p-2",
                                        onclick: move |e: MouseEvent| e.stop_propagation(),
                                        // Play/Pause button
                                        button {
                                            class: "flex-shrink-0 w-8 h-8 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition flex items-center justify-center",
                                            onclick: toggle_play,
                                            if voice_messages_store::VOICE_PLAYBACK.read().currently_playing == Some(event_id_parsed) {
                                                // Pause icon
                                                svg {
                                                    class: "w-4 h-4",
                                                    view_box: "0 0 24 24",
                                                    fill: "currentColor",
                                                    rect { x: "6", y: "4", width: "4", height: "16" }
                                                    rect { x: "14", y: "4", width: "4", height: "16" }
                                                }
                                            } else {
                                                // Play icon
                                                svg {
                                                    class: "w-4 h-4 ml-0.5",
                                                    view_box: "0 0 24 24",
                                                    fill: "currentColor",
                                                    polygon { points: "8,5 19,12 8,19" }
                                                }
                                            }
                                        }
                                        // Progress bar and time
                                        div {
                                            class: "flex-1",
                                            div {
                                                class: "w-full h-1 bg-muted rounded-full overflow-hidden mb-1",
                                                div {
                                                    class: "h-full bg-primary transition-all",
                                                    style: "width: {progress_percent}%"
                                                }
                                            }
                                            div {
                                                class: "flex justify-between text-xs text-muted-foreground",
                                                span { "{current_time_str}" }
                                                span { "{duration_str}" }
                                            }
                                        }
                                    }
                                }
                            } else {
                                RichContent {
                                    content: event.content.clone(),
                                    tags: event.tags.clone().to_vec()
                                }
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
                                                let current_count = *repost_count.read();
                                                repost_count.set(current_count.saturating_add(1));
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

                            // Like button with reaction picker
                            ReactionButton {
                                reaction: reaction.clone(),
                                has_signer: has_signer,
                                icon_class: "h-4 w-4".to_string(),
                                count_class: "text-xs".to_string(),
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
                                                        format_sats_compact(amount)
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
                                class: "{bookmark_button_class} hover:bg-blue-500/10 gap-1 px-2 py-1.5 rounded",
                                disabled: *is_bookmarking.read(),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    if *is_bookmarking.read() {
                                        return;
                                    }

                                    let event_id_clone = event_id_bookmark.clone();
                                    is_bookmarking.set(true);

                                    spawn(async move {
                                        let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);
                                        if currently_bookmarked {
                                            if let Err(e) = bookmarks::unbookmark_event(event_id_clone).await {
                                                log::error!("Failed to unbookmark: {}", e);
                                            }
                                        } else {
                                            if let Err(e) = bookmarks::bookmark_event(event_id_clone).await {
                                                log::error!("Failed to bookmark: {}", e);
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
                                class: "flex items-center gap-1 text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    log::info!("Share button clicked for event");
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

            // Recursively render children
            if !children.is_empty() && depth < MAX_DEPTH {
                div {
                    class: "space-y-1 mt-1",
                    for child in children {
                        ThreadedComment {
                            node: child.clone(),
                            depth: depth + 1
                        }
                    }
                }
            } else if !children.is_empty() && depth >= MAX_DEPTH {
                // Max depth reached, show "Continue thread" link
                div {
                    class: "ml-4 mt-2",
                    Link {
                        to: Route::Note { note_id: event.id.to_hex(), from_voice: None },
                        class: "text-xs text-blue-500 hover:underline",
                        "→ Continue thread ({children.len()} more replies)"
                    }
                }
            }
        }

        // Reply composer modal
        if *show_reply_modal.read() {
            ReplyComposer {
                reply_to: event.clone(),
                on_close: move |_| {
                    show_reply_modal.set(false);
                },
                on_success: move |_| {
                    show_reply_modal.set(false);
                    // Update reply count
                    let current = *reply_count.read();
                    reply_count.set(current + 1);
                }
            }
        }

        // Zap modal
        if *show_zap_modal.read() {
            {
                let zap_display_name = author_metadata.read().as_ref()
                    .and_then(|m| m.display_name.clone().or(m.name.clone()))
                    .unwrap_or_else(|| {
                        author_pubkey.to_string().chars().take(16).collect::<String>() + "..."
                    });

                rsx! {
                    ZapModal {
                        recipient_pubkey: author_pubkey_str.clone(),
                        recipient_name: zap_display_name,
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
    }
}

