use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::stores::signer::SIGNER_INFO;
use crate::components::{ThreadedComment, CommentComposer, ClientInitializing, ShareModal, icons::MessageCircleIcon};
use crate::utils::build_thread_tree;
use crate::utils::format_sats_compact;
use nostr_sdk::{Event, Filter, Kind, EventId, Timestamp, PublicKey};
use std::time::Duration;
use wasm_bindgen::JsCast;
use web_sys::HtmlVideoElement;

#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,
    Global,
}

#[component]
pub fn VideoDetail(video_id: String) -> Element {
    // Parse video ID and feed type from URL
    let (clean_video_id, feed_type) = parse_video_id_and_feed(&video_id);
    let clean_video_id_for_shorts = clean_video_id.clone();

    let mut video_event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Load video on mount
    use_effect(move || {
        let id = clean_video_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

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

    rsx! {
        div {
            class: "min-h-screen bg-black",

            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && video_event.read().is_none()) {
                ClientInitializing {}
            } else if let Some(err) = error.read().as_ref() {
                // Error state
                div {
                    class: "flex items-center justify-center h-screen text-white",
                    div {
                        class: "text-center",
                        div {
                            class: "mb-4 flex justify-center",
                            crate::components::icons::AlertTriangleIcon { class: "w-24 h-24 text-red-400" }
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
            } else if let Some(event) = video_event.read().as_ref() {
                // Check video kind to determine which player to show
                if event.kind == Kind::Custom(22) {
                    // Short video - use vertical scroll player
                    ShortsPlayer {
                        initial_video_id: clean_video_id_for_shorts.clone(),
                        feed_type: feed_type,
                        initial_event: Some(event.clone())
                    }
                } else {
                    // Landscape video - use single video player
                    LandscapePlayer {
                        event: event.clone()
                    }
                }
            } else {
                // Not found state
                div {
                    class: "flex items-center justify-center h-screen text-white",
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

#[component]
fn LandscapePlayer(event: Event) -> Element {
    let mut is_muted = use_signal(|| false);
    let mut comments = use_signal(|| Vec::<Event>::new());
    let mut loading_comments = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let event_id = event.id;

    // Fetch NIP-22 comments for the video
    use_effect(move || {
        spawn(async move {
            loading_comments.set(true);

            // Fetch Kind 1111 (NIP-22 Comment) events and Kind 1 replies
            let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
            let filter_upper = Filter::new()
                .kind(Kind::Comment)
                .custom_tag(upper_e_tag, event_id.to_hex())
                .limit(500);

            let filter_lower = Filter::new()
                .kinds(vec![Kind::TextNote, Kind::Comment])
                .event(event_id)
                .limit(500);

            let mut all_comments = Vec::new();

            if let Ok(upper_comments) = nostr_client::fetch_events_aggregated(filter_upper, Duration::from_secs(10)).await {
                all_comments.extend(upper_comments.into_iter());
            }

            if let Ok(lower_comments) = nostr_client::fetch_events_aggregated(filter_lower, Duration::from_secs(10)).await {
                all_comments.extend(lower_comments.into_iter());
            }

            // Deduplicate
            let mut seen_ids = std::collections::HashSet::new();
            let unique_comments: Vec<Event> = all_comments.into_iter()
                .filter(|event| seen_ids.insert(event.id))
                .collect();

            let mut sorted_comments = unique_comments;
            sorted_comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            comments.set(sorted_comments);

            loading_comments.set(false);
        });
    });

    let video_meta = parse_video_meta(&event);

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "sticky top-0 z-20 bg-black/80 backdrop-blur-sm border-b border-gray-800",
                div {
                    class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: crate::routes::Route::Videos {},
                        class: "hover:bg-white/20 p-2 rounded-full transition text-white",
                        crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                    }
                    h2 {
                        class: "text-xl font-bold text-white",
                        "Video"
                    }
                }
            }

            // Content
            div {
                class: "max-w-[1400px] mx-auto p-4",

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
                            muted: *is_muted.read(),
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

                // Video info card
                div {
                    class: "bg-card border border-border rounded-lg p-6 mb-4",

                    // Title
                    if let Some(title) = &video_meta.title {
                        h1 {
                            class: "text-2xl font-bold mb-3",
                            "{title}"
                        }
                    }

                    // Author info
                    AuthorInfo {
                        pubkey: event.pubkey.to_string()
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
                        class: "flex flex-wrap gap-4 text-sm text-muted-foreground mb-4",

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
                                "{format_time_ago(event.created_at.as_secs())}"
                            }
                        }
                    }

                    // Interaction buttons
                    VideoInteractions {
                        event: event.clone(),
                        is_muted: *is_muted.read(),
                        on_mute_toggle: move |_| {
                            let current = *is_muted.read();
                            is_muted.set(!current);
                        }
                    }
                }

                // Comments section
                div {
                    class: "bg-card border border-border rounded-lg p-6",
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
                        let thread_tree = build_thread_tree(comment_vec, &event_id);

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
                            spawn(async move {
                                loading_comments.set(true);

                                let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
                                let filter_upper = Filter::new()
                                    .kind(Kind::Comment)
                                    .custom_tag(upper_e_tag, event_id.to_hex())
                                    .limit(500);

                                let filter_lower = Filter::new()
                                    .kinds(vec![Kind::TextNote, Kind::Comment])
                                    .event(event_id)
                                    .limit(500);

                                let mut all_comments = Vec::new();

                                if let Ok(upper_comments) = nostr_client::fetch_events_aggregated(filter_upper, Duration::from_secs(10)).await {
                                    all_comments.extend(upper_comments.into_iter());
                                }

                                if let Ok(lower_comments) = nostr_client::fetch_events_aggregated(filter_lower, Duration::from_secs(10)).await {
                                    all_comments.extend(lower_comments.into_iter());
                                }

                                let mut seen_ids = std::collections::HashSet::new();
                                let unique_comments: Vec<Event> = all_comments.into_iter()
                                    .filter(|event| seen_ids.insert(event.id))
                                    .collect();

                                let mut sorted_comments = unique_comments;
                                sorted_comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                                comments.set(sorted_comments);

                                loading_comments.set(false);
                            });
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ShortsPlayer(initial_video_id: String, feed_type: FeedType, initial_event: Option<Event>) -> Element {
    let mut events = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut current_video_index = use_signal(|| 0usize);
    let mut is_muted = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load shorts feed on mount
    use_effect(move || {
        let id = initial_video_id.clone();
        let initial_evt = initial_event.clone();
        let has_initial = initial_evt.is_some();
        loading.set(true);

        spawn(async move {
            // Load shorts from the specified feed
            let result = match feed_type {
                FeedType::Following => load_shorts_following(None).await,
                FeedType::Global => load_shorts_global(None).await,
            };

            match result {
                Ok(mut video_events) => {
                    // If we have an initial event, insert it at position 0 and deduplicate
                    if let Some(evt) = initial_evt {
                        // Remove the event from the feed if it already exists (deduplicate)
                        video_events.retain(|e| e.id != evt.id);

                        // Insert the initial event at position 0
                        video_events.insert(0, evt);

                        log::info!("Inserted initial video at position 0, total videos: {}", video_events.len());
                    }

                    if let Some(last_event) = video_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    has_more.set(video_events.len() >= 50);

                    // The initial video is now always at index 0 if provided, otherwise find it
                    let initial_index = if has_initial {
                        0
                    } else {
                        video_events.iter()
                            .position(|e| e.id.to_hex() == id)
                            .unwrap_or(0)
                    };

                    events.set(video_events);
                    current_video_index.set(initial_index);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load shorts: {}", e);
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

            loading.set(true);

            spawn(async move {
                let result = match feed_type {
                    FeedType::Following => load_shorts_following(until).await,
                    FeedType::Global => load_shorts_global(until).await,
                };

                match result {
                    Ok(new_events) => {
                        // Filter out duplicates by checking existing event IDs
                        let existing_ids: std::collections::HashSet<_> = {
                            let current = events.read();
                            current.iter().map(|e| e.id).collect()
                        };

                        let unique_events: Vec<_> = new_events.into_iter()
                            .filter(|e| !existing_ids.contains(&e.id))
                            .collect();

                        if unique_events.is_empty() {
                            // No new unique events, stop pagination
                            has_more.set(false);
                            loading.set(false);
                            log::info!("No new unique shorts found, stopping pagination");
                        } else {
                            // Update timestamp from last unique event
                            if let Some(last_event) = unique_events.last() {
                                oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                            }

                            // Set has_more based on number of unique events
                            has_more.set(unique_events.len() >= 50);

                            // Append only unique events
                            let current_idx = *current_video_index.read();
                            let mut current = events.read().clone();
                            current.extend(unique_events);
                            events.set(current);

                            // Increment index only after successful append
                            current_video_index.set(current_idx + 1);
                            loading.set(false);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to load more shorts: {}", e);
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

    rsx! {
        div {
            class: "fixed inset-0 bg-black overflow-hidden",

            // Header overlay
            div {
                class: "absolute top-0 left-0 right-0 z-50 bg-gradient-to-b from-black/60 to-transparent",
                div {
                    class: "px-4 py-3 flex items-center justify-between",

                    Link {
                        to: crate::routes::Route::Videos {},
                        class: "w-10 h-10 bg-white/20 hover:bg-white/30 rounded-full flex items-center justify-center text-white transition",
                        crate::components::icons::ArrowLeftIcon { class: "w-5 h-5" }
                    }

                    h2 {
                        class: "text-white font-bold",
                        "Shorts"
                    }

                    div {
                        class: "w-10 h-10"
                    }
                }
            }

            // Video player area
            if events.read().is_empty() && !*loading.read() {
                div {
                    class: "flex items-center justify-center h-full text-white",
                    div {
                        class: "text-center",
                        div {
                            class: "mb-4 flex justify-center",
                            crate::components::icons::VideoIcon { class: "w-24 h-24 text-gray-500" }
                        }
                        h3 {
                            class: "text-2xl font-semibold",
                            "No shorts available"
                        }
                    }
                }
            } else if let Some(event) = events.read().get(*current_video_index.read()) {
                VerticalVideoPlayer {
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
                if events.read().len() > 0 && (*current_video_index.read() < events.read().len() - 1 || *has_more.read()) {
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

#[component]
fn VerticalVideoPlayer(
    event: Event,
    is_active: bool,
    is_muted: bool,
    on_mute_toggle: EventHandler<()>,
) -> Element {
    let video_id = format!("video-{}", event.id.to_hex()[..8].to_string());
    let video_id_for_effect = video_id.clone();
    let video_meta = parse_video_meta(&event);

    // Reactively update muted state
    use_effect(use_reactive(&is_muted, move |muted| {
        let id = video_id_for_effect.clone();

        spawn(async move {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(&id) {
                        if let Ok(video) = element.dyn_into::<HtmlVideoElement>() {
                            video.set_muted(muted);
                        }
                    }
                }
            }
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
    let event_id_parsed = event.id;

    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut is_liked = use_signal(|| false);
    let mut is_liking = use_signal(|| false);
    let mut show_comments_modal = use_signal(|| false);
    let mut show_comment_composer = use_signal(|| false);
    let mut comments = use_signal(|| Vec::<Event>::new());
    let mut loading_comments = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);

    // Fetch counts
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

            let combined_filter = Filter::new()
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Comment,
                    Kind::Reaction,
                    Kind::from(9735),
                ])
                .event(event_id_parsed)
                .limit(2000);

            if let Ok(events) = client.fetch_events(combined_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());

                let mut replies = 0;
                let mut likes = 0;
                let mut total_sats = 0u64;
                let mut user_has_liked = false;

                for event in events {
                    match event.kind {
                        Kind::TextNote | Kind::Comment => replies += 1,
                        Kind::Reaction => {
                            likes += 1;
                            if let Some(ref user_pk) = current_user_pubkey {
                                if event.pubkey.to_string() == *user_pk {
                                    user_has_liked = true;
                                }
                            }
                        },
                        kind if kind == Kind::from(9735) => {
                            if let Some(amount) = event.tags.iter().find_map(|tag| {
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
                            }) {
                                total_sats += amount;
                            }
                        }
                        _ => {}
                    }
                }

                reply_count.set(replies.min(500));
                like_count.set(likes.min(500));
                zap_amount_sats.set(total_sats);
                is_liked.set(user_has_liked);
            }
        });
    }));

    // Fetch author metadata
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

    // Fetch comments when modal opens
    use_effect(use_reactive((&*show_comments_modal.read(), &event_id_for_comments), move |(modal_open, event_id_str)| {
        if !modal_open {
            return;
        }

        let event_id_clone = event_id_str.to_string();

        spawn(async move {
            loading_comments.set(true);

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_clone) {
                Ok(id) => id,
                Err(_) => {
                    loading_comments.set(false);
                    return;
                }
            };

            let event_id_hex = event_id_parsed.to_hex();
            let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
            let filter_upper = Filter::new()
                .kind(Kind::Comment)
                .custom_tag(upper_e_tag, event_id_hex.clone())
                .limit(500);

            let filter_lower = Filter::new()
                .kinds(vec![Kind::TextNote, Kind::Comment])
                .event(event_id_parsed)
                .limit(500);

            let mut all_comments = Vec::new();

            if let Ok(upper_comments) = nostr_client::fetch_events_aggregated(filter_upper, Duration::from_secs(10)).await {
                all_comments.extend(upper_comments.into_iter());
            }

            if let Ok(lower_comments) = nostr_client::fetch_events_aggregated(filter_lower, Duration::from_secs(10)).await {
                all_comments.extend(lower_comments.into_iter());
            }

            let mut seen_ids = std::collections::HashSet::new();
            let unique_comments: Vec<Event> = all_comments.into_iter()
                .filter(|event| seen_ids.insert(event.id))
                .collect();

            let mut sorted_comments = unique_comments;
            sorted_comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
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
                        "{format_time_ago(event.created_at.as_secs())}"
                    }
                }

                // Right side: Action buttons
                div {
                    class: "flex flex-col items-center gap-4",

                    // Like button
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

                    // Comment button
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

                    // Zap button
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
                                            {format_sats_compact(*zap_amount_sats.read())}
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

                    div {
                        class: "bg-card border border-border rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-y-auto",
                        onclick: move |e| e.stop_propagation(),

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

#[component]
fn VideoInteractions(event: Event, is_muted: bool, on_mute_toggle: EventHandler<()>) -> Element {
    let event_id = event.id.to_string();
    let event_id_counts = event_id.clone();

    let mut reply_count = use_signal(|| 0usize);
    let mut like_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let mut is_liked = use_signal(|| false);
    let mut is_liking = use_signal(|| false);

    // Fetch counts
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

            let combined_filter = Filter::new()
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Comment,
                    Kind::Reaction,
                    Kind::from(9735),
                ])
                .event(event_id_parsed)
                .limit(2000);

            if let Ok(events) = client.fetch_events(combined_filter, Duration::from_secs(5)).await {
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());

                let mut replies = 0;
                let mut likes = 0;
                let mut total_sats = 0u64;
                let mut user_has_liked = false;

                for event in events {
                    match event.kind {
                        Kind::TextNote | Kind::Comment => replies += 1,
                        Kind::Reaction => {
                            likes += 1;
                            if let Some(ref user_pk) = current_user_pubkey {
                                if event.pubkey.to_string() == *user_pk {
                                    user_has_liked = true;
                                }
                            }
                        },
                        kind if kind == Kind::from(9735) => {
                            if let Some(amount) = event.tags.iter().find_map(|tag| {
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
                            }) {
                                total_sats += amount;
                            }
                        }
                        _ => {}
                    }
                }

                reply_count.set(replies.min(500));
                like_count.set(likes.min(500));
                zap_amount_sats.set(total_sats);
                is_liked.set(user_has_liked);
            }
        });
    }));

    rsx! {
        div {
            class: "flex gap-3 pt-4 border-t border-border",

            // Like button
            button {
                class: if *is_liked.read() {
                    "flex items-center gap-2 px-4 py-2 bg-red-500/20 text-red-500 hover:bg-red-500/30 rounded-lg transition"
                } else {
                    "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition"
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
                    class: "w-5 h-5".to_string(),
                    filled: *is_liked.read()
                }
                span {
                    if *like_count.read() > 0 {
                        "Like ({format_count(*like_count.read())})"
                    } else {
                        "Like"
                    }
                }
            }

            // Zap button
            button {
                class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                crate::components::icons::ZapIcon {
                    class: "w-5 h-5".to_string(),
                    filled: false
                }
                span {
                    if *zap_amount_sats.read() > 0 {
                        "Zap ({format_sats_compact(*zap_amount_sats.read())})"
                    } else {
                        "Zap"
                    }
                }
            }

            // Mute toggle
            button {
                class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                onclick: move |_| on_mute_toggle.call(()),
                if is_muted {
                    crate::components::icons::VolumeXIcon { class: "w-5 h-5" }
                } else {
                    crate::components::icons::VolumeIcon { class: "w-5 h-5" }
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

#[component]
fn AuthorInfo(pubkey: String) -> Element {
    use nostr_sdk::{PublicKey, FromBech32};

    let pubkey_clone = pubkey.clone();
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

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

// Parse video ID and feed type from URL
fn parse_video_id_and_feed(video_id: &str) -> (String, FeedType) {
    if let Some((id, query)) = video_id.split_once('?') {
        let feed_type = if query.contains("feed=following") {
            FeedType::Following
        } else {
            FeedType::Global
        };
        (id.to_string(), feed_type)
    } else {
        (video_id.to_string(), FeedType::Global)
    }
}

// Helper function to load a video by ID
async fn load_video_by_id(video_id: &str) -> Result<Event, String> {
    log::info!("Loading video by ID: {}", video_id);

    let event_id = EventId::parse(video_id)
        .map_err(|e| format!("Invalid video ID: {}", e))?;

    let filter = Filter::new()
        .id(event_id)
        .kinds([Kind::Custom(21), Kind::Custom(22)]);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch video: {}", e))?;

    events
        .into_iter()
        .next()
        .ok_or_else(|| "Video not found".to_string())
}

// Load shorts from following feed
async fn load_shorts_following(until: Option<u64>) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global", e);
            return load_shorts_global(until).await;
        }
    };

    if contacts.is_empty() {
        return load_shorts_global(until).await;
    }

    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }

    if authors.is_empty() {
        return load_shorts_global(until).await;
    }

    let mut filter = Filter::new()
        .kind(Kind::Custom(22))
        .authors(authors)
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            if event_vec.is_empty() {
                return load_shorts_global(until).await;
            }

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch following shorts: {}", e);
            load_shorts_global(until).await
        }
    }
}

// Load shorts from global feed
async fn load_shorts_global(until: Option<u64>) -> Result<Vec<Event>, String> {
    let mut filter = Filter::new()
        .kind(Kind::Custom(22))
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch global shorts: {}", e);
            Err(format!("Failed to load shorts: {}", e))
        }
    }
}
