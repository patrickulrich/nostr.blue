use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::stores::signer::SIGNER_INFO;
use crate::components::{ThreadedComment, CommentComposer, ClientInitializing, ShareModal, icons::MessageCircleIcon};
use crate::utils::build_thread_tree;
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;
use js_sys::eval;

#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,
    Global,
}

impl FeedType {
    fn label(&self) -> &'static str {
        match self {
            FeedType::Following => "Following",
            FeedType::Global => "Global",
        }
    }
}

#[component]
pub fn Videos() -> Element {
    // State for feed events
    let mut events = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut sidebar_open = use_signal(|| false);

    // Vertical scroll state
    let mut current_video_index = use_signal(|| 0usize);
    let mut is_muted = use_signal(|| false);

    // Pagination state
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load feed on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_videos(None).await,
                FeedType::Global => load_global_videos(None).await,
            };

            match result {
                Ok(video_events) => {
                    if let Some(last_event) = video_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                    }

                    has_more.set(video_events.len() >= 50);
                    events.set(video_events);
                    current_video_index.set(0);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Navigation functions
    let mut next_video = move || {
        let current = *current_video_index.read();
        let total = events.read().len();

        if current + 1 < total {
            current_video_index.set(current + 1);
        } else if *has_more.read() && !*loading.read() {
            // Load more videos
            let until = *oldest_timestamp.read();
            let current_feed_type = *feed_type.read();

            loading.set(true);

            spawn(async move {
                let result = match current_feed_type {
                    FeedType::Following => load_following_videos(until).await,
                    FeedType::Global => load_global_videos(until).await,
                };

                match result {
                    Ok(mut new_events) => {
                        if let Some(last_event) = new_events.last() {
                            oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                        }

                        has_more.set(new_events.len() >= 50);

                        let current_idx = *current_video_index.read();
                        let mut current = events.read().clone();
                        current.append(&mut new_events);
                        events.set(current);
                        current_video_index.set(current_idx + 1);
                        loading.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to load more videos: {}", e);
                        loading.set(false);
                    }
                }
            });
        }
    };

    let mut prev_video = move || {
        let current = *current_video_index.read();
        if current > 0 {
            current_video_index.set(current - 1);
        }
    };

    // Note: Keyboard navigation would be added via onkeydown event on the div

    rsx! {
        div {
            class: "fixed inset-0 bg-black overflow-hidden",

            // Slide-out Sidebar Overlay
            if *sidebar_open.read() {
                div {
                    class: "fixed inset-0 bg-black/50 z-50",
                    onclick: move |_| sidebar_open.set(false),

                    aside {
                        class: "w-[275px] bg-gray-900 h-full border-r border-gray-700",
                        style: "margin-top: 64px; height: calc(100vh - 64px);",
                        onclick: move |e| e.stop_propagation(),
                        div {
                            class: "h-full flex flex-col p-4 overflow-y-auto",

                            // Navigation Menu (matches main sidebar)
                            nav {
                                class: "flex flex-col gap-1",

                                Link {
                                    to: crate::routes::Route::Home {},
                                    onclick: move |_| sidebar_open.set(false),
                                    class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                    crate::components::icons::HomeIcon { class: "w-7 h-7" }
                                    span { "Home" }
                                }

                                Link {
                                    to: crate::routes::Route::Explore {},
                                    onclick: move |_| sidebar_open.set(false),
                                    class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                    crate::components::icons::CompassIcon { class: "w-7 h-7" }
                                    span { "Explore" }
                                }

                                Link {
                                    to: crate::routes::Route::Articles {},
                                    onclick: move |_| sidebar_open.set(false),
                                    class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                    crate::components::icons::BookOpenIcon { class: "w-7 h-7" }
                                    span { "Articles" }
                                }

                                // Check if user is authenticated
                                if crate::stores::auth_store::AUTH_STATE.read().is_authenticated {
                                    Link {
                                        to: crate::routes::Route::Photos {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                        crate::components::icons::CameraIcon { class: "w-7 h-7" }
                                        span { "Photos" }
                                    }

                                    Link {
                                        to: crate::routes::Route::Videos {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white font-bold",
                                        crate::components::icons::VideoIcon { class: "w-7 h-7" }
                                        span { "Videos" }
                                    }

                                    Link {
                                        to: crate::routes::Route::Notifications {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                        crate::components::icons::BellIcon { class: "w-7 h-7" }
                                        span { "Notifications" }
                                    }

                                    Link {
                                        to: crate::routes::Route::DMs {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                        crate::components::icons::MailIcon { class: "w-7 h-7" }
                                        span { "Messages" }
                                    }

                                    Link {
                                        to: crate::routes::Route::Lists {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                        crate::components::icons::ListIcon { class: "w-7 h-7" }
                                        span { "Lists" }
                                    }

                                    Link {
                                        to: crate::routes::Route::Bookmarks {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                        crate::components::icons::BookmarkIcon { class: "w-7 h-7" }
                                        span { "Bookmarks" }
                                    }

                                    // Profile link with pubkey
                                    if let Some(pubkey) = &crate::stores::auth_store::AUTH_STATE.read().pubkey {
                                        Link {
                                            to: crate::routes::Route::Profile { pubkey: pubkey.clone() },
                                            onclick: move |_| sidebar_open.set(false),
                                            class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                            crate::components::icons::UserIcon { class: "w-7 h-7" }
                                            span { "Profile" }
                                        }
                                    }

                                    Link {
                                        to: crate::routes::Route::Settings {},
                                        onclick: move |_| sidebar_open.set(false),
                                        class: "flex items-center justify-start gap-4 px-4 py-2 rounded-full hover:bg-gray-800 transition text-xl w-full text-white",
                                        crate::components::icons::SettingsIcon { class: "w-7 h-7" }
                                        span { "Settings" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Header overlay
            div {
                class: "absolute top-0 left-0 right-0 z-50 bg-gradient-to-b from-black/60 to-transparent",
                div {
                    class: "px-4 py-3 flex items-center justify-between",

                    // Left side: N logo button + Feed type selector
                    div {
                        class: "flex items-center gap-3",

                        // N Logo button to open sidebar
                        button {
                            class: "w-10 h-10 bg-blue-500 hover:bg-blue-600 rounded-full flex items-center justify-center text-white font-bold text-xl transition",
                            onclick: move |_| sidebar_open.set(true),
                            "N"
                        }

                        // Feed type selector
                        div {
                            class: "relative",
                            button {
                                class: "text-white font-bold flex items-center gap-2 hover:bg-white/20 px-3 py-2 rounded-lg transition",
                                onclick: move |_| {
                                    let current = *show_dropdown.read();
                                    show_dropdown.set(!current);
                                },
                                crate::components::icons::VideoIcon { class: "w-5 h-5" }
                                span { "{feed_type.read().label()}" }
                                if *show_dropdown.read() {
                                    crate::components::icons::ChevronUpIcon { class: "w-4 h-4" }
                                } else {
                                    crate::components::icons::ChevronDownIcon { class: "w-4 h-4" }
                                }
                            }

                            if *show_dropdown.read() {
                                div {
                                    class: "absolute top-full left-0 mt-2 bg-gray-900 border border-gray-700 rounded-lg shadow-lg min-w-[200px] overflow-hidden z-50",

                                    button {
                                        class: "w-full px-4 py-3 text-left text-white hover:bg-gray-800 transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Following);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Following"
                                            }
                                            div {
                                                class: "text-xs text-gray-400",
                                                "Videos from people you follow"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Following {
                                            crate::components::icons::CheckIcon { class: "w-5 h-5" }
                                        }
                                    }

                                    div {
                                        class: "border-t border-gray-700"
                                    }

                                    button {
                                        class: "w-full px-4 py-3 text-left text-white hover:bg-gray-800 transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Global);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Global"
                                            }
                                            div {
                                                class: "text-xs text-gray-400",
                                                "Videos from everyone"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Global {
                                            crate::components::icons::CheckIcon { class: "w-5 h-5" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Right side: Refresh button
                    button {
                        class: "p-2 hover:bg-white/20 rounded-full transition disabled:opacity-50 text-white",
                        disabled: *loading.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh feed",
                        if *loading.read() {
                            span {
                                class: "inline-block w-5 h-5 border-2 border-white border-t-transparent rounded-full animate-spin"
                            }
                        } else {
                            crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                        }
                    }
                }
            }

            // Video player area
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "flex items-center justify-center h-full text-white",
                    div {
                        class: "text-center",
                        div {
                            class: "mb-4 flex justify-center",
                            crate::components::icons::AlertTriangleIcon { class: "w-24 h-24 text-red-400" }
                        }
                        p {
                            class: "text-red-400",
                            "Error loading videos: {err}"
                        }
                    }
                }
            } else if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && events.read().is_empty()) {
                // Show client initializing animation during:
                // 1. Client initialization
                // 2. Initial video load (loading + no videos, regardless of error state)
                ClientInitializing {}
            } else if events.read().is_empty() {
                div {
                    class: "flex items-center justify-center h-full text-white",
                    div {
                        class: "text-center space-y-4",
                        div {
                            class: "mb-4 flex justify-center",
                            crate::components::icons::VideoIcon { class: "w-24 h-24 text-gray-500" }
                        }
                        h3 {
                            class: "text-2xl font-semibold",
                            "No videos yet"
                        }
                        p {
                            class: "text-gray-400",
                            "Video posts from the network will appear here."
                        }
                        p {
                            class: "text-sm text-gray-500",
                            "NIP-71 video events (Kind 21 & 22)"
                        }
                    }
                }
            } else {
                // Display current video
                if let Some(event) = events.read().get(*current_video_index.read()) {
                    VideoPlayer {
                        key: "{event.id}",
                        event: event.clone(),
                        is_active: true,
                        is_muted: *is_muted.read(),
                        on_mute_toggle: move |_| {
                            let current = *is_muted.read();
                            is_muted.set(!current);
                        }
                    }
                }

                // Navigation buttons
                div {
                    class: "absolute right-4 top-1/2 -translate-y-1/2 flex flex-col gap-4 z-40",

                    // Up button
                    if *current_video_index.read() > 0 {
                        button {
                            class: "w-12 h-12 rounded-full bg-white/20 hover:bg-white/30 backdrop-blur-sm flex items-center justify-center text-white transition",
                            onclick: move |_| prev_video(),
                            crate::components::icons::ChevronUpIcon { class: "w-6 h-6" }
                        }
                    }

                    // Down button
                    if *current_video_index.read() < events.read().len() - 1 || *has_more.read() {
                        button {
                            class: "w-12 h-12 rounded-full bg-white/20 hover:bg-white/30 backdrop-blur-sm flex items-center justify-center text-white transition",
                            onclick: move |_| next_video(),
                            if *loading.read() {
                                span {
                                    class: "inline-block w-5 h-5 border-2 border-white border-t-transparent rounded-full animate-spin"
                                }
                            } else {
                                crate::components::icons::ChevronDownIcon { class: "w-6 h-6" }
                            }
                        }
                    }
                }

                // Video counter
                div {
                    class: "absolute bottom-4 left-4 text-white text-sm font-medium bg-black/50 px-3 py-2 rounded-full backdrop-blur-sm",
                    "{*current_video_index.read() + 1} / {events.read().len()}"
                }
            }
        }
    }
}

#[component]
fn VideoPlayer(
    event: Event,
    is_active: bool,
    is_muted: bool,
    on_mute_toggle: EventHandler<()>,
) -> Element {
    // Generate unique ID for this video element
    let video_id = format!("video-{}", event.id.to_hex()[..8].to_string());
    let video_id_for_effect = video_id.clone();

    // Parse video metadata from NIP-71 imeta tags
    let video_meta = parse_video_meta(&event);

    // Reactively update muted state
    use_effect(use_reactive(&is_muted, move |muted| {
        let id = video_id_for_effect.clone();

        spawn(async move {
            let video_id_json = serde_json::to_string(&id)
                .unwrap_or_else(|_| format!("\"{}\"", id));

            let script = format!(
                r#"
                (function() {{
                    let video = document.getElementById({video_id});
                    if (video) video.muted = {muted};
                }})();
                "#,
                video_id = video_id_json,
                muted = if muted { "true" } else { "false" }
            );
            let _ = eval(&script);
        });
    }));

    rsx! {
        div {
            class: "relative w-full h-full flex items-center justify-center bg-black",

            if let Some(url) = video_meta.url.clone() {
                video {
                    id: "{video_id}",
                    class: "max-w-full max-h-full object-contain",
                    src: "{url}",
                    poster: "{video_meta.thumbnail.clone().unwrap_or_default()}",
                    loop: true,
                    muted: is_muted,
                    autoplay: is_active,
                    playsinline: true,
                    controls: true,
                }
            } else {
                div {
                    class: "flex flex-col items-center justify-center text-white",
                    div {
                        class: "text-6xl mb-4",
                        "▶"
                    }
                    p {
                        "Video unavailable"
                    }
                }
            }

            // Video info overlay
            VideoInfo {
                event: event.clone(),
                video_meta: video_meta,
                is_muted: is_muted,
                on_mute_toggle: on_mute_toggle
            }
        }
    }
}

#[component]
fn VideoInfo(
    event: Event,
    video_meta: VideoMeta,
    is_muted: bool,
    on_mute_toggle: EventHandler<()>,
) -> Element {
    use nostr_sdk::{PublicKey, FromBech32};

    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let event_id = event.id.to_string();
    let event_id_counts = event_id.clone();
    let event_id_for_comments = event_id.clone();
    let event_id_parsed = event.id; // Store the already-parsed EventId

    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // State for counts
    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    // State for interactions
    let mut is_liked = use_signal(|| false);
    let mut is_liking = use_signal(|| false);

    // State for comments modal
    let mut show_comments_modal = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut comments = use_signal(|| Vec::<Event>::new());
    let mut loading_comments = use_signal(|| false);

    // State for share modal
    let mut show_share_modal = use_signal(|| false);

    // Fetch counts - consolidated into a single batched fetch
    use_effect(use_reactive(&event_id_counts, move |event_id_for_counts| {
        spawn(async move {
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                Ok(id) => id,
                Err(_) => return,
            };

            // Create a combined filter for all interaction kinds (replies, likes, zaps)
            let combined_filter = Filter::new()
                .kinds(vec![
                    Kind::TextNote,      // kind 1 - replies
                    Kind::Comment,       // kind 1111 - NIP-22 comments
                    Kind::Reaction,      // kind 7 - likes
                    Kind::from(9735),    // kind 9735 - zaps
                ])
                .event(event_id_parsed)
                .limit(2000);

            // Single fetch for all interaction types
            if let Ok(events) = client.fetch_events(combined_filter, Duration::from_secs(5)).await {
                // Get current user's pubkey to check if they've already reacted
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());

                // Partition events by kind
                let mut replies = 0;
                let mut likes = 0;
                let mut total_sats = 0u64;
                let mut user_has_liked = false;

                for event in events {
                    match event.kind {
                        Kind::TextNote | Kind::Comment => replies += 1,
                        Kind::Reaction => {
                            likes += 1;
                            // Check if this reaction is from the current user
                            if let Some(ref user_pk) = current_user_pubkey {
                                if event.pubkey.to_string() == *user_pk {
                                    user_has_liked = true;
                                }
                            }
                        },
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

                // Update all counts and states at once
                reply_count.set(replies.min(500));
                like_count.set(likes.min(500));
                zap_amount_sats.set(total_sats);
                is_liked.set(user_has_liked);
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

            let filter = Filter::new()
                .author(pubkey)
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
    }));

    // Fetch comments when modal opens - reactive to modal state
    use_effect(use_reactive((&*show_comments_modal.read(), &event_id_for_comments), move |(modal_open, event_id_str)| {
        if !modal_open {
            return;
        }

        let event_id_clone = event_id_str.to_string();

        spawn(async move {
            loading_comments.set(true);
            log::info!("Fetching comments for video: {}", event_id_clone);

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_clone) {
                Ok(id) => {
                    log::info!("Successfully parsed event ID");
                    id
                }
                Err(e) => {
                    log::error!("Failed to parse event ID {}: {}", event_id_clone, e);
                    loading_comments.set(false);
                    return;
                }
            };

            // Fetch comments for this video
            // NIP-22 comments use uppercase E tags for root reference
            // We need to use custom_tag() to filter for uppercase 'E' tags
            // Also include lowercase 'e' tags for standard kind 1 replies
            let event_id_hex = event_id_parsed.to_hex();

            // Create filter for uppercase E tags (NIP-22 comments)
            let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
            let filter_upper = Filter::new()
                .kind(Kind::Comment)
                .custom_tag(upper_e_tag, event_id_hex.clone())
                .limit(500);

            // Create filter for lowercase e tags (standard replies)
            let filter_lower = Filter::new()
                .kinds(vec![Kind::TextNote, Kind::Comment])
                .event(event_id_parsed)
                .limit(500);

            log::info!("Fetching comments with uppercase E and lowercase e tag filters");

            // Fetch both filters and combine results
            let mut all_comments = Vec::new();

            if let Ok(upper_comments) = nostr_client::fetch_events_aggregated(filter_upper, Duration::from_secs(10)).await {
                log::info!("Loaded {} comments with uppercase E tags", upper_comments.len());
                all_comments.extend(upper_comments.into_iter());
            } else {
                log::warn!("Failed to fetch comments with uppercase E tags");
            }

            if let Ok(lower_comments) = nostr_client::fetch_events_aggregated(filter_lower, Duration::from_secs(10)).await {
                log::info!("Loaded {} comments with lowercase e tags", lower_comments.len());
                all_comments.extend(lower_comments.into_iter());
            } else {
                log::warn!("Failed to fetch comments with lowercase e tags");
            }

            // Deduplicate by event ID
            let mut seen_ids = std::collections::HashSet::new();
            let unique_comments: Vec<Event> = all_comments.into_iter()
                .filter(|event| seen_ids.insert(event.id))
                .collect();

            let mut sorted_comments = unique_comments;
            sorted_comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            log::info!("Total unique comments: {}", sorted_comments.len());
            comments.set(sorted_comments);

            loading_comments.set(false);
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

    let profile_image = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    rsx! {
        div {
            class: "absolute bottom-0 left-0 right-0 p-6 pb-20 bg-gradient-to-t from-black/80 to-transparent",

            div {
                class: "flex items-end justify-between",

                // Left side: Video info
                div {
                    class: "flex-1 mr-4",

                    // Author info
                    Link {
                        to: crate::routes::Route::Profile { pubkey: event.pubkey.to_string() },
                        class: "flex items-center mb-3",
                        div {
                            class: "w-10 h-10 rounded-full bg-gray-700 flex items-center justify-center text-white font-bold mr-3 ring-2 ring-white overflow-hidden",
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
                        span {
                            class: "text-white font-semibold",
                            "{display_name}"
                        }
                    }

                    // Video title
                    if let Some(title) = video_meta.title {
                        h3 {
                            class: "text-white font-semibold text-lg mb-2",
                            "{title}"
                        }
                    }

                    // Video description
                    if !event.content.is_empty() {
                        p {
                            class: "text-white/90 text-sm line-clamp-2",
                            "{event.content}"
                        }
                    }

                    // Timestamp
                    p {
                        class: "text-white/60 text-xs mt-2",
                        "{format_time_ago(event.created_at.as_u64())}"
                    }
                }

                // Right side: Action buttons
                div {
                    class: "flex flex-col items-center gap-4",

                    // Like button with count
                    button {
                        class: if *is_liked.read() {
                            "text-red-500 hover:bg-white/20 p-3 rounded-full transition flex flex-col items-center"
                        } else {
                            "text-white hover:bg-white/20 p-3 rounded-full transition flex flex-col items-center"
                        },
                        disabled: *is_liking.read(),
                        onclick: move |_| {
                            if *is_liking.read() {
                                return;
                            }

                            let event_id_clone = event_id.clone();
                            let author_pk_clone = event.pubkey.to_string();

                            is_liking.set(true);

                            spawn(async move {
                                match nostr_client::publish_reaction(event_id_clone, author_pk_clone, "+".to_string()).await {
                                    Ok(_) => {
                                        is_liked.set(true);
                                        let current_count = *like_count.read();
                                        like_count.set(current_count.saturating_add(1));
                                        is_liking.set(false);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to like video: {}", e);
                                        is_liking.set(false);
                                    }
                                }
                            });
                        },
                        crate::components::icons::HeartIcon {
                            class: "w-6 h-6".to_string(),
                            filled: *is_liked.read()
                        }
                        if *like_count.read() > 0 {
                            span {
                                class: if *is_liked.read() {
                                    "text-xs mt-1 font-semibold text-red-500"
                                } else {
                                    "text-xs mt-1 font-semibold"
                                },
                                {
                                    let count = *like_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else {
                                        format_count(count)
                                    }
                                }
                            }
                        }
                    }

                    // Comment button with count - opens modal
                    button {
                        class: "text-white hover:bg-white/20 p-3 rounded-full transition flex flex-col items-center",
                        onclick: move |_| show_comments_modal.set(true),
                        crate::components::icons::MessageCircleIcon {
                            class: "w-6 h-6".to_string(),
                            filled: false
                        }
                        if *reply_count.read() > 0 {
                            span {
                                class: "text-xs mt-1 font-semibold",
                                {
                                    let count = *reply_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else {
                                        format_count(count)
                                    }
                                }
                            }
                        }
                    }

                    // Zap button with amount (if author has lightning)
                    {
                        let has_lightning = author_metadata.read().as_ref()
                            .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                            .is_some();

                        if has_lightning {
                            rsx! {
                                button {
                                    class: "text-white hover:bg-white/20 p-3 rounded-full transition flex flex-col items-center",
                                    crate::components::icons::ZapIcon {
                                        class: "w-6 h-6".to_string(),
                                        filled: false
                                    }
                                    if *zap_amount_sats.read() > 0 {
                                        span {
                                            class: "text-xs mt-1 font-semibold text-yellow-400",
                                            {format_sats(*zap_amount_sats.read())}
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }

                    // Share button
                    button {
                        class: "text-white hover:bg-white/20 p-3 rounded-full transition",
                        onclick: move |_| show_share_modal.set(true),
                        crate::components::icons::ShareIcon { class: "w-6 h-6" }
                    }

                    // Mute button
                    button {
                        class: "text-white hover:bg-white/20 p-3 rounded-full transition",
                        onclick: move |_| on_mute_toggle.call(()),
                        if is_muted {
                            crate::components::icons::VolumeXIcon { class: "w-6 h-6" }
                        } else {
                            crate::components::icons::VolumeIcon { class: "w-6 h-6" }
                        }
                    }
                }
            }

            // Comments Modal
            if *show_comments_modal.read() {
                div {
                    class: "fixed inset-0 z-[100] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4",
                    onclick: move |_| show_comments_modal.set(false),

                    // Modal content
                    div {
                        class: "bg-card border border-border rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-y-auto",
                        onclick: move |e| e.stop_propagation(),

                        // Header
                        div {
                            class: "sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between z-10",
                            h3 {
                                class: "text-lg font-semibold text-white",
                                "Comments"
                            }
                            button {
                                class: "text-muted-foreground hover:text-foreground transition text-white",
                                onclick: move |_| show_comments_modal.set(false),
                                "✕"
                            }
                        }

                        // Comments body
                        div {
                            class: "p-6",
                            {
                                let comment_vec = comments.read().clone();
                                let thread_tree = build_thread_tree(comment_vec, &event_id_parsed);

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

                        // Footer with Add Comment button
                        div {
                            class: "sticky bottom-0 bg-card border-t border-border px-6 py-4 flex items-center justify-end gap-3 z-10",
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition flex items-center gap-2",
                                onclick: move |_| {
                                    show_comments_modal.set(false);
                                    show_comment_composer.set(true);
                                },
                                MessageCircleIcon { class: "w-4 h-4".to_string(), filled: false }
                                span { "Add Comment" }
                            }
                        }
                    }
                }
            }

            // Comment Composer Modal
            if *show_comment_composer.read() {
                CommentComposer {
                    comment_on: event.clone(),
                    parent_comment: None,
                    on_close: move |_| show_comment_composer.set(false),
                    on_success: move |_| {
                        show_comment_composer.set(false);
                        // Refresh comments
                        comments.set(Vec::new());
                        show_comments_modal.set(true);
                    }
                }
            }

            // Share Modal
            if *show_share_modal.read() {
                ShareModal {
                    event: event.clone(),
                    on_close: move |_| show_share_modal.set(false)
                }
            }
        }
    }
}

// Video metadata structure
#[derive(Clone, Debug, PartialEq)]
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

// Format count with k/M suffixes
fn format_count(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{}M", count / 1_000_000)
    } else if count >= 1_000 {
        format!("{}k", count / 1_000)
    } else {
        count.to_string()
    }
}

// Format sats with k/M suffixes
fn format_sats(sats: u64) -> String {
    if sats >= 1_000_000 {
        format!("{}M", sats / 1_000_000)
    } else if sats >= 1_000 {
        format!("{}k", sats / 1_000)
    } else {
        sats.to_string()
    }
}

// Helper function to load following videos (Kind 21 & 22 from followed users)
async fn load_following_videos(until: Option<u64>) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following videos feed for {} (until: {:?})", pubkey_str, until);

    // Fetch contacts
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            return load_global_videos(until).await;
        }
    };

    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global videos");
        return load_global_videos(until).await;
    }

    log::info!("User follows {} accounts", contacts.len());

    // Parse contact pubkeys
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }

    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        return load_global_videos(until).await;
    }

    // Create filter for NIP-71 video events (Kind 21 and 22)
    let mut filter = Filter::new()
        .kinds([Kind::Custom(21), Kind::Custom(22)])
        .authors(authors)
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    log::info!("Fetching video events from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} video events from following", events.len());

            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            if event_vec.is_empty() {
                log::info!("No videos from followed users, showing global feed");
                return load_global_videos(until).await;
            }

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch following videos: {}, falling back to global", e);
            load_global_videos(until).await
        }
    }
}

// Helper function to load global videos (Kind 21 & 22 from everyone)
async fn load_global_videos(until: Option<u64>) -> Result<Vec<Event>, String> {
    log::info!("Loading global videos feed (until: {:?})...", until);

    let mut filter = Filter::new()
        .kinds([Kind::Custom(21), Kind::Custom(22)])
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400 * 7); // 7 days ago
        filter = filter.since(since);
    }

    log::info!("Fetching global video events with filter: {:?}", filter);

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} global video events", events.len());

            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch global video events: {}", e);
            Err(format!("Failed to load videos: {}", e))
        }
    }
}
