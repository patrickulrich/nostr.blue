use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;

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
            } else if *loading.read() && events.read().is_empty() {
                div {
                    class: "flex items-center justify-center h-full text-white",
                    span {
                        class: "flex items-center gap-3",
                        span {
                            class: "inline-block w-8 h-8 border-4 border-white border-t-transparent rounded-full animate-spin"
                        }
                        "Loading videos..."
                    }
                }
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

    // Parse video metadata from NIP-71 imeta tags
    let video_meta = parse_video_meta(&event);

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
    use crate::stores::nostr_client::get_client;
    use nostr_sdk::{PublicKey, FromBech32};

    let author_pubkey = event.pubkey.to_string();
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch author's profile metadata
    use_effect(move || {
        let pubkey_str = author_pubkey.clone();

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
    });

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

                    // Like button (placeholder - would integrate with reactions system)
                    button {
                        class: "text-white hover:bg-white/20 p-3 rounded-full transition",
                        crate::components::icons::HeartIcon {
                            class: "w-6 h-6".to_string(),
                            filled: false
                        }
                    }

                    // Comment button
                    button {
                        class: "text-white hover:bg-white/20 p-3 rounded-full transition",
                        crate::components::icons::MessageCircleIcon {
                            class: "w-6 h-6".to_string(),
                            filled: false
                        }
                    }

                    // Share button
                    button {
                        class: "text-white hover:bg-white/20 p-3 rounded-full transition",
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

// Helper function to load following videos (Kind 21 & 22 from followed users)
async fn load_following_videos(until: Option<u64>) -> Result<Vec<Event>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

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

    match client.fetch_events(filter, Duration::from_secs(10)).await {
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
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

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

    match client.fetch_events(filter, Duration::from_secs(10)).await {
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
