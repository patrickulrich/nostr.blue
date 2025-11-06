use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::components::ClientInitializing;
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;
use wasm_bindgen::JsCast;

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
    // State for featured landscape videos
    let mut featured_landscape = use_signal(|| Vec::<Event>::new());
    let mut loading_featured = use_signal(|| false);

    // State for recent shorts section
    let mut recent_shorts = use_signal(|| Vec::<Event>::new());
    let mut loading_recent_shorts = use_signal(|| false);

    // State for combined feed
    let mut feed_events = use_signal(|| Vec::<Event>::new());
    let mut loading_feed = use_signal(|| false);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut refresh_trigger = use_signal(|| 0);

    // Pagination state
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    let mut error = use_signal(|| None::<String>);

    // Load featured landscape videos on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading_featured.set(true);

        spawn(async move {
            match load_featured_content().await {
                Ok(landscape) => {
                    featured_landscape.set(landscape);
                    loading_featured.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load featured landscape videos: {}", e);
                    loading_featured.set(false);
                }
            }
        });
    });

    // Load recent shorts on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading_recent_shorts.set(true);

        spawn(async move {
            match load_recent_shorts().await {
                Ok(shorts) => {
                    recent_shorts.set(shorts);
                    loading_recent_shorts.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load recent shorts: {}", e);
                    loading_recent_shorts.set(false);
                }
            }
        });
    });

    // Load combined feed when feed type changes or refresh is triggered
    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        loading_feed.set(true);
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
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    has_more.set(video_events.len() >= 50);
                    feed_events.set(video_events);
                    loading_feed.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading_feed.set(false);
                }
            }
        });
    });

    // Load more function for infinite scroll
    let mut load_more = move || {
        if *loading_feed.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        let current_feed_type = *feed_type.read();

        loading_feed.set(true);

        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_videos(until).await,
                FeedType::Global => load_global_videos(until).await,
            };

            match result {
                Ok(mut new_events) => {
                    if let Some(last_event) = new_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }

                    has_more.set(new_events.len() >= 50);

                    let mut current = feed_events.read().clone();
                    current.append(&mut new_events);
                    feed_events.set(current);
                    loading_feed.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more videos: {}", e);
                    loading_feed.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "min-h-screen bg-background",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-6 py-4 flex items-center justify-between max-w-[1600px] mx-auto",

                    h1 {
                        class: "text-2xl font-bold flex items-center gap-3",
                        crate::components::icons::VideoIcon { class: "w-7 h-7" }
                        "Videos"
                    }

                    button {
                        class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                        disabled: *loading_featured.read() || *loading_recent_shorts.read() || *loading_feed.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh",
                        if *loading_featured.read() || *loading_recent_shorts.read() || *loading_feed.read() {
                            span {
                                class: "inline-block w-5 h-5 border-2 border-foreground border-t-transparent rounded-full animate-spin"
                            }
                        } else {
                            crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                        }
                    }
                }
            }

            // Content
            div {
                class: "max-w-[1600px] mx-auto px-6 py-6",

                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else {
                    // Featured Landscape Videos Section
                    if !featured_landscape.read().is_empty() {
                        div {
                            class: "mb-8",
                            h2 {
                                class: "text-xl font-semibold mb-4",
                                "Recommended Videos"
                            }
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for event in featured_landscape.read().iter().take(3) {
                                    LandscapeVideoCard {
                                        key: "{event.id}",
                                        event: event.clone(),
                                        feed_type: *feed_type.read()
                                    }
                                }
                            }
                        }
                    }

                    // Recent Shorts Section
                    if !recent_shorts.read().is_empty() {
                        div {
                            class: "mb-8",
                            h2 {
                                class: "text-xl font-semibold mb-4",
                                "Recent Shorts"
                            }
                            div {
                                class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                                for event in recent_shorts.read().iter().take(5) {
                                    ShortsVideoCard {
                                        key: "{event.id}",
                                        event: event.clone(),
                                        feed_type: FeedType::Following
                                    }
                                }
                            }
                        }
                    }

                    // Combined Feed Section
                    div {
                        class: "mt-8",

                        // Feed selector
                        div {
                            class: "flex items-center justify-between mb-4",
                            h2 {
                                class: "text-xl font-semibold",
                                "All Videos"
                            }

                            div {
                                class: "relative",
                                button {
                                    class: "flex items-center gap-2 px-4 py-2 bg-accent hover:bg-accent/80 rounded-lg transition",
                                    onclick: move |_| {
                                        let current = *show_dropdown.read();
                                        show_dropdown.set(!current);
                                    },
                                    span { "{feed_type.read().label()}" }
                                    if *show_dropdown.read() {
                                        crate::components::icons::ChevronUpIcon { class: "w-4 h-4" }
                                    } else {
                                        crate::components::icons::ChevronDownIcon { class: "w-4 h-4" }
                                    }
                                }

                                if *show_dropdown.read() {
                                    div {
                                        class: "absolute top-full right-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-50",

                                        button {
                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
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
                                                    class: "text-xs text-muted-foreground",
                                                    "Videos from people you follow"
                                                }
                                            }
                                            if *feed_type.read() == FeedType::Following {
                                                crate::components::icons::CheckIcon { class: "w-5 h-5" }
                                            }
                                        }

                                        div {
                                            class: "border-t border-border"
                                        }

                                        button {
                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
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
                                                    class: "text-xs text-muted-foreground",
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

                        // Combined feed grid
                        if let Some(err) = error.read().as_ref() {
                            div {
                                class: "text-center py-12",
                                div {
                                    class: "text-destructive mb-2",
                                    "Error: {err}"
                                }
                            }
                        } else if feed_events.read().is_empty() && !*loading_feed.read() {
                            div {
                                class: "text-center py-12",
                                div {
                                    class: "mb-4 flex justify-center",
                                    crate::components::icons::VideoIcon { class: "w-24 h-24 text-muted-foreground" }
                                }
                                h3 {
                                    class: "text-xl font-semibold mb-2",
                                    "No videos yet"
                                }
                                p {
                                    class: "text-muted-foreground",
                                    "Videos will appear here"
                                }
                            }
                        } else {
                            // Unified feed with both landscape and shorts mixed together
                            div {
                                class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                for event in feed_events.read().iter() {
                                    // Each video type uses its own card component with appropriate styling
                                    if event.kind == Kind::Custom(22) {
                                        // Shorts use vertical card
                                        ShortsVideoCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                            feed_type: *feed_type.read()
                                        }
                                    } else {
                                        // Landscape videos use horizontal card
                                        LandscapeVideoCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                            feed_type: *feed_type.read()
                                        }
                                    }
                                }
                            }

                            // Load more button
                            if *has_more.read() {
                                div {
                                    class: "flex justify-center mt-8",
                                    button {
                                        class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                                        disabled: *loading_feed.read(),
                                        onclick: move |_| load_more(),
                                        if *loading_feed.read() {
                                            span {
                                                class: "inline-block w-5 h-5 border-2 border-primary-foreground border-t-transparent rounded-full animate-spin mr-2"
                                            }
                                            "Loading..."
                                        } else {
                                            "Load More"
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

#[component]
fn LandscapeVideoCard(event: Event, feed_type: FeedType) -> Element {
    let video_meta = parse_video_meta(&event);
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let author_pubkey = event.pubkey.to_string();
    let mut is_hovering = use_signal(|| false);
    let video_element_id = format!("preview-{}", event.id.to_hex()[..12].to_string());
    let video_element_id_for_effect = video_element_id.clone();

    // Fetch author metadata
    use_effect(use_reactive(&author_pubkey, move |pubkey_str| {
        spawn(async move {
            if let Ok(pubkey) = PublicKey::parse(&pubkey_str) {
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
            }
        });
    }));

    // Play/pause video on hover (only if no thumbnail)
    use_effect(use_reactive(&*is_hovering.read(), move |hovering| {
        let id = video_element_id_for_effect.clone();
        spawn(async move {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(&id) {
                        if let Ok(video) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                            if hovering {
                                let _ = video.play();
                            } else {
                                let _ = video.pause();
                                video.set_current_time(0.0);
                            }
                        }
                    }
                }
            }
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

    let video_id = event.id.to_hex();
    let feed_param = match feed_type {
        FeedType::Following => "following",
        FeedType::Global => "global",
    };

    rsx! {
        div {
            class: "group cursor-pointer",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),

            Link {
                to: crate::routes::Route::VideoDetail { video_id: format!("{}?feed={}", video_id, feed_param) },

                div {
                    class: "relative aspect-video bg-muted rounded-lg overflow-hidden mb-3",

                    // Show thumbnail if available, otherwise show video (first frame until hover)
                    if let Some(thumbnail) = &video_meta.thumbnail {
                        img {
                            src: "{thumbnail}",
                            alt: "{video_meta.title.as_deref().unwrap_or(\"Video\")}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                        }
                    } else if let Some(url) = &video_meta.url {
                        video {
                            id: "{video_element_id}",
                            class: "w-full h-full object-cover",
                            src: "{url}",
                            muted: true,
                            loop: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                    } else {
                        div {
                            class: "w-full h-full flex items-center justify-center bg-muted",
                            crate::components::icons::VideoIcon { class: "w-12 h-12 text-muted-foreground" }
                        }
                    }

                    // Duration badge
                    if let Some(duration) = &video_meta.duration {
                        div {
                            class: "absolute bottom-2 right-2 bg-black/80 text-white text-xs px-2 py-1 rounded",
                            "{duration}"
                        }
                    }
                }

                // Video info
                div {
                    if let Some(title) = &video_meta.title {
                        h3 {
                            class: "font-semibold line-clamp-2 mb-1 group-hover:text-primary transition",
                            "{title}"
                        }
                    }

                    p {
                        class: "text-sm text-muted-foreground mb-1",
                        "{display_name}"
                    }

                    p {
                        class: "text-xs text-muted-foreground",
                        "{format_time_ago(event.created_at.as_secs())}"
                    }
                }
            }
        }
    }
}

#[component]
fn ShortsVideoCard(event: Event, feed_type: FeedType) -> Element {
    let video_meta = parse_video_meta(&event);
    let mut is_hovering = use_signal(|| false);
    let video_element_id = format!("preview-short-{}", event.id.to_hex()[..12].to_string());
    let video_element_id_for_effect = video_element_id.clone();

    // Play/pause video on hover (only if no thumbnail)
    use_effect(use_reactive(&*is_hovering.read(), move |hovering| {
        let id = video_element_id_for_effect.clone();
        spawn(async move {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(&id) {
                        if let Ok(video) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                            if hovering {
                                let _ = video.play();
                            } else {
                                let _ = video.pause();
                                video.set_current_time(0.0);
                            }
                        }
                    }
                }
            }
        });
    }));

    let video_id = event.id.to_hex();
    let feed_param = match feed_type {
        FeedType::Following => "following",
        FeedType::Global => "global",
    };

    rsx! {
        div {
            class: "group cursor-pointer",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),

            Link {
                to: crate::routes::Route::VideoDetail { video_id: format!("{}?feed={}", video_id, feed_param) },

                div {
                    class: "relative aspect-[9/16] bg-muted rounded-lg overflow-hidden mb-2",

                    // Show thumbnail if available, otherwise show video (first frame until hover)
                    if let Some(thumbnail) = &video_meta.thumbnail {
                        img {
                            src: "{thumbnail}",
                            alt: "{video_meta.title.as_deref().unwrap_or(\"Short\")}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                        }
                    } else if let Some(url) = &video_meta.url {
                        video {
                            id: "{video_element_id}",
                            class: "w-full h-full object-cover",
                            src: "{url}",
                            muted: true,
                            loop: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                    } else {
                        div {
                            class: "w-full h-full flex items-center justify-center bg-muted",
                            crate::components::icons::VideoIcon { class: "w-8 h-8 text-muted-foreground" }
                        }
                    }

                    // Shorts indicator
                    div {
                        class: "absolute bottom-2 left-2 bg-black/80 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
                        crate::components::icons::VideoIcon { class: "w-3 h-3" }
                        "Short"
                    }
                }

                // Title only for shorts
                if let Some(title) = &video_meta.title {
                    p {
                        class: "text-sm font-medium line-clamp-2 group-hover:text-primary transition",
                        "{title}"
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

// Load featured landscape videos (3 landscape videos from Following, fallback to Global)
async fn load_featured_content() -> Result<Vec<Event>, String> {
    log::info!("Loading featured landscape videos...");

    // Try Following feed first
    let _result = if let Some(pubkey_str) = auth_store::get_pubkey() {
        match nostr_client::fetch_contacts(pubkey_str).await {
            Ok(contacts) if !contacts.is_empty() => {
                let mut authors = Vec::new();
                for contact in contacts.iter() {
                    if let Ok(pk) = PublicKey::parse(contact) {
                        authors.push(pk);
                    }
                }

                if !authors.is_empty() {
                    // Fetch only landscape videos (Kind 21)
                    let filter = Filter::new()
                        .kinds([Kind::Custom(21)])
                        .authors(authors)
                        .limit(20);

                    let all_events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
                        .await
                        .unwrap_or_default();

                    let mut all_events_vec: Vec<Event> = all_events.into_iter().collect();
                    all_events_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                    // Take first 3 landscape videos
                    let landscape_vec: Vec<Event> = all_events_vec.into_iter().take(3).collect();

                    if !landscape_vec.is_empty() {
                        return Ok(landscape_vec);
                    }
                }
            }
            _ => {}
        }
    };

    // Fallback to Global if Following is empty or not authenticated
    log::info!("Falling back to global feed for featured landscape videos");

    // Fetch only landscape videos (Kind 21)
    let filter = Filter::new()
        .kinds([Kind::Custom(21)])
        .limit(20);

    let all_events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .unwrap_or_default();

    let mut all_events_vec: Vec<Event> = all_events.into_iter().collect();
    all_events_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Take first 3 landscape videos
    let landscape_vec: Vec<Event> = all_events_vec.into_iter().take(3).collect();

    Ok(landscape_vec)
}

// Load recent shorts videos (5 shorts from Following feed only)
async fn load_recent_shorts() -> Result<Vec<Event>, String> {
    log::info!("Loading recent shorts videos from Following feed...");

    // Only fetch from Following feed
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    match nostr_client::fetch_contacts(pubkey_str).await {
        Ok(contacts) if !contacts.is_empty() => {
            let mut authors = Vec::new();
            for contact in contacts.iter() {
                if let Ok(pk) = PublicKey::parse(contact) {
                    authors.push(pk);
                }
            }

            if !authors.is_empty() {
                // Fetch only shorts videos (Kind 22)
                let filter = Filter::new()
                    .kinds([Kind::Custom(22)])
                    .authors(authors)
                    .limit(20);

                let all_events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
                    .await
                    .unwrap_or_default();

                let mut all_events_vec: Vec<Event> = all_events.into_iter().collect();
                all_events_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                // Take first 5 shorts videos
                let shorts_vec: Vec<Event> = all_events_vec.into_iter().take(5).collect();

                return Ok(shorts_vec);
            }
        }
        Ok(_) => {
            log::info!("User doesn't follow anyone, returning empty shorts");
            return Ok(Vec::new());
        }
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}", e);
            return Ok(Vec::new());
        }
    }

    Ok(Vec::new())
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
