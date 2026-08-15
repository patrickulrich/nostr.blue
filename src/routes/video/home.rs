use crate::components::{ClientInitializing, MiniLiveStreamCard};
use crate::routes::video::live_discovery::{
    load_following_live_streams, load_global_live_streams, LiveStreamStatusFilter,
};
use crate::stores::feed_cache::FeedCacheKey;
use crate::stores::{auth_store, feed_cache, nostr_client};
use crate::utils::format::{format_relative_time_or, truncate_pubkey};
use crate::utils::video_kinds::{
    dedupe_videos_by_url, horizontal_kinds, is_horizontal_video, vertical_kinds,
};
use crate::utils::FeedItem;
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp};
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
    let mut featured_landscape = use_signal(Vec::<Event>::new);
    let mut loading_featured = use_signal(|| false);
    let mut recent_verts = use_signal(Vec::<Event>::new);
    let mut loading_recent_verts = use_signal(|| false);
    let mut recent_live_streams = use_signal(Vec::<Event>::new);
    let mut loading_recent_live_streams = use_signal(|| false);
    let mut feed_events = use_signal(Vec::<Event>::new);
    let mut loading_feed = use_signal(|| false);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut refresh_trigger = use_signal(|| 0);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);
    let mut request_id = use_signal(|| 0u32);
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following));
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
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        loading_recent_live_streams.set(true);
        spawn(async move {
            match load_recent_live_streams().await {
                Ok(streams) => {
                    recent_live_streams.set(streams);
                    loading_recent_live_streams.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load recent live streams: {}", e);
                    loading_recent_live_streams.set(false);
                }
            }
        });
    });
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        loading_recent_verts.set(true);
        spawn(async move {
            match load_recent_verts().await {
                Ok(verts) => {
                    recent_verts.set(verts);
                    loading_recent_verts.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load recent verts: {}", e);
                    loading_recent_verts.set(false);
                }
            }
        });
    });
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();
        let auth_state = auth_store::AUTH_STATE.read();
        let is_authenticated = auth_state.is_authenticated;
        let login_method = auth_state.login_method.clone();
        drop(auth_state); // Release lock before async operations

        // Wait for client initialization
        if !client_initialized {
            return;
        }
        // For authenticated users with signing capability, wait for signer restoration
        // This prevents race condition where CLIENT_INITIALIZED is true but
        // restore_session_async() hasn't attached the signer yet
        // ReadOnly (npub) users don't need a signer and should bypass this guard
        let requires_signer = matches!(
            login_method,
            Some(auth_store::LoginMethod::BrowserExtension)
                | Some(auth_store::LoginMethod::PrivateKey)
                | Some(auth_store::LoginMethod::RemoteSigner)
        ) || {
            #[cfg(feature = "mobile_platform")]
            {
                matches!(login_method, Some(auth_store::LoginMethod::AndroidSigner))
            }
            #[cfg(not(feature = "mobile_platform"))]
            {
                false
            }
        };
        if is_authenticated && requires_signer && !has_signer {
            log::debug!("Waiting for signer restoration before loading video feed...");
            return;
        }
        let (last_refresh, last_feed) = *last_loaded_trigger.peek();
        let has_data = !feed_events.peek().is_empty();
        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;
        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!(
                "Skipping videos feed re-load: data already present, no intentional change"
            );
            return;
        }
        last_loaded_trigger.set((refresh, current_feed_type));
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        loading_feed.set(true);
        if feed_type_changed {
            feed_events.set(Vec::new());
        }
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);
        spawn(async move {
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale videos feed request {}", current_id);
                return;
            }
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = match current_feed_type {
                FeedType::Following => FeedCacheKey::Videos { pubkey: pubkey_str },
                FeedType::Global => FeedCacheKey::VideosGlobal,
            };
            let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                .await
                .unwrap_or_default();
            if *request_id.peek() != current_id {
                log::debug!(
                    "Discarding stale videos feed request {} after cache load",
                    current_id
                );
                return;
            }
            if !cached_items.is_empty() {
                log::info!("Loaded {} videos from cache", cached_items.len());
                let cached_events: Vec<Event> = cached_items
                    .iter()
                    .map(|i| i.event().clone())
                    .filter(|event| is_horizontal_video(event.kind.as_u16()))
                    .collect();
                if let Some(oldest) = cached_events.iter().map(|e| e.created_at).min() {
                    oldest_timestamp.set(Some(oldest.as_secs().saturating_sub(1)));
                }
                feed_events.set(cached_events);
            }
            let result = match current_feed_type {
                FeedType::Following => {
                    load_following_horizontal_videos(None, |batch| {
                        if *request_id.peek() != current_id {
                            return;
                        }
                        let mut current = feed_events.cloned();
                        current.extend(batch);
                        current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                        let deduped = dedupe_videos_by_url(current);
                        feed_events.set(deduped);
                        loading_feed.set(false);
                    })
                    .await
                }
                FeedType::Global => load_global_horizontal_videos(None, |batch| {
                    if *request_id.peek() != current_id {
                        return;
                    }
                    let mut current = feed_events.cloned();
                    current.extend(batch);
                    current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                    let deduped = dedupe_videos_by_url(current);
                    feed_events.set(deduped);
                    loading_feed.set(false);
                })
                .await
                .map(|(e, h)| (e, h, false)),
            };
            if *request_id.peek() != current_id {
                log::debug!(
                    "Discarding stale videos feed request {} after network load",
                    current_id
                );
                return;
            }
            match result {
                Ok((video_events, page_has_more, did_fallback)) => {
                    let effective_cache_key = if did_fallback {
                        log::info!("No contacts, switched to Global videos feed");
                        feed_type.set(FeedType::Global);
                        FeedCacheKey::VideosGlobal
                    } else {
                        cache_key.clone()
                    };
                    if let Some(last_event) = video_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }
                    let feed_items: Vec<FeedItem> = video_events
                        .iter()
                        .map(|e| FeedItem::OriginalPost(e.clone()))
                        .collect();
                    let cache_key_for_store = effective_cache_key;
                    spawn(async move {
                        let _ =
                            feed_cache::store_feed_items(&cache_key_for_store, &feed_items).await;
                        let _ = feed_cache::run_eviction_if_needed().await;
                    });
                    has_more.set(page_has_more);
                    feed_events.set(video_events);
                    loading_feed.set(false);
                }
                Err(e) => {
                    if cached_items.is_empty() {
                        error.set(Some(e));
                    } else {
                        log::warn!("Network error but showing cached videos: {}", e);
                    }
                    loading_feed.set(false);
                }
            }
        });
    });
    let mut load_more = move || {
        if *loading_feed.read() || !*has_more.read() {
            return;
        }
        let until = match *oldest_timestamp.read() {
            Some(ts) => Some(ts),
            None => return,
        };
        let current_feed_type = *feed_type.read();
        loading_feed.set(true);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_horizontal_videos(until, |_| {})
                    .await
                    .map(|(e, h, _)| (e, h)),
                FeedType::Global => load_global_horizontal_videos(until, |_| {}).await,
            };
            match result {
                Ok((new_events, page_has_more)) => {
                    // Capture oldest timestamp from new page BEFORE merge (for cursor advancement)
                    // This ensures the cursor advances past duplicates even if they get deduped
                    let oldest_new_ts = new_events
                        .iter()
                        .min_by_key(|e| e.created_at)
                        .map(|e| e.created_at.as_secs());

                    // Merge existing feed with new events, then dedupe
                    // This allows addressable kinds to replace non-addressable across pages
                    let current = feed_events.cloned();
                    let merged: Vec<_> = current.clone().into_iter().chain(new_events).collect();
                    let deduped = dedupe_videos_by_url(merged);

                    // Always advance cursor based on fetched page (not deduped list)
                    // This prevents getting stuck when a page contains only duplicates
                    if let Some(ts) = oldest_new_ts {
                        oldest_timestamp.set(Some(ts));
                    }

                    // Always trust API for pagination state
                    has_more.set(page_has_more);

                    // Only update UI when content actually changed
                    if deduped != current {
                        feed_events.set(deduped);
                    } else {
                        log::info!(
                            "Page contained only duplicates, cursor advanced to continue discovery"
                        );
                    }
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
        div { class: "min-h-screen bg-background",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div { class: "px-6 py-4 flex items-center justify-between max-w-[1600px] mx-auto",
                    h1 { class: "text-2xl font-bold flex items-center gap-3",
                        crate::components::icons::VideoIcon { class: "w-7 h-7" }
                        "Videos"
                    }
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                        disabled: *loading_featured.read()
                            || *loading_recent_verts.read()
                            || *loading_recent_live_streams.read()
                            || *loading_feed.read(),
                        onclick: move |_| {
                            let current = *refresh_trigger.read();
                            refresh_trigger.set(current + 1);
                        },
                        title: "Refresh",
                        if *loading_featured.read()
                            || *loading_recent_verts.read()
                            || *loading_recent_live_streams.read()
                            || *loading_feed.read()
                        {
                            span { class: "inline-block w-5 h-5 border-2 border-foreground border-t-transparent rounded-full animate-spin" }
                        } else {
                            crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                        }
                    }
                }
            }
            div { class: "max-w-[1600px] mx-auto px-6 py-6",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else {
                    if !featured_landscape.read().is_empty() {
                        div { class: "mb-8",
                            h2 { class: "text-xl font-semibold mb-4", "Featured Videos" }
                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for event in featured_landscape.read().iter().take(3) {
                                    LandscapeVideoCard {
                                        key: "{event.id}",
                                        event: event.clone(),
                                        feed_type: *feed_type.read(),
                                    }
                                }
                            }
                        }
                    }
                    if !recent_verts.read().is_empty() {
                        div { class: "mb-8",
                            div { class: "mb-4 flex items-center justify-between gap-4",
                                h2 { class: "text-xl font-semibold", "Recent Verts" }
                                Link {
                                    to: crate::routes::Route::VideosVerts {},
                                    class: "text-sm font-medium text-primary hover:text-primary/80 transition",
                                    "More ->"
                                }
                            }
                            div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                                for event in recent_verts.read().iter().take(5) {
                                    VertsVideoCard {
                                        key: "{event.id}",
                                        event: event.clone(),
                                        feed_type: FeedType::Following,
                                    }
                                }
                            }
                        }
                    }
                    if !recent_live_streams.read().is_empty() {
                        div { class: "mb-8",
                            div { class: "mb-4 flex items-center justify-between gap-4",
                                h2 { class: "text-xl font-semibold", "Live Now" }
                                Link {
                                    to: crate::routes::Route::VideosLive {},
                                    class: "text-sm font-medium text-primary hover:text-primary/80 transition",
                                    "More ->"
                                }
                            }
                            div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4",
                                for event in recent_live_streams.read().iter().take(4) {
                                    MiniLiveStreamCard { key: "{event.id}", event: event.clone() }
                                }
                            }
                        }
                    }
                    div { class: "mt-8",
                        div { class: "flex items-center justify-between mb-4",
                            h2 { class: "text-xl font-semibold", "More Videos" }
                            div { class: "relative",
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
                                    div { class: "absolute top-full right-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-50",
                                        button {
                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                            onclick: move |_| {
                                                feed_type.set(FeedType::Following);
                                                show_dropdown.set(false);
                                            },
                                            div {
                                                div { class: "font-medium", "Following" }
                                                div { class: "text-xs text-muted-foreground",
                                                    "Videos from people you follow"
                                                }
                                            }
                                            if *feed_type.read() == FeedType::Following {
                                                crate::components::icons::CheckIcon { class: "w-5 h-5" }
                                            }
                                        }
                                        div { class: "border-t border-border" }
                                        button {
                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                            onclick: move |_| {
                                                feed_type.set(FeedType::Global);
                                                show_dropdown.set(false);
                                            },
                                            div {
                                                div { class: "font-medium", "Global" }
                                                div { class: "text-xs text-muted-foreground",
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
                        if let Some(err) = error.read().as_ref() {
                            div { class: "text-center py-12",
                                div { class: "text-destructive mb-2", "Error: {err}" }
                            }
                        } else if feed_events.read().is_empty() && !*loading_feed.read() {
                            div { class: "text-center py-12",
                                div { class: "mb-4 flex justify-center",
                                    crate::components::icons::VideoIcon { class: "w-24 h-24 text-muted-foreground" }
                                }
                                h3 { class: "text-xl font-semibold mb-2", "No horizontal videos yet" }
                                p { class: "text-muted-foreground", "Landscape videos will appear here" }
                            }
                        } else {
                            div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                for event in feed_events.read().iter() {
                                    if is_horizontal_video(event.kind.as_u16()) {
                                        LandscapeVideoCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                            feed_type: *feed_type.read(),
                                        }
                                    }
                                }
                            }
                            if *has_more.read() {
                                div { class: "flex justify-center mt-8",
                                    button {
                                        class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                                        disabled: *loading_feed.read(),
                                        onclick: move |_| load_more(),
                                        if *loading_feed.read() {
                                            span { class: "inline-block w-5 h-5 border-2 border-primary-foreground border-t-transparent rounded-full animate-spin mr-2" }
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
    let video_element_id = format!("preview-{}", &event.id.to_hex()[..12]);
    let video_element_id_for_effect = video_element_id.clone();
    use_effect(use_reactive(&author_pubkey, move |pubkey_str| {
        spawn(async move {
            if let Ok(pubkey) = PublicKey::parse(&pubkey_str) {
                let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
                if let Ok(events) =
                    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await
                {
                    if let Some(event) = events.into_iter().next() {
                        if let Ok(metadata) =
                            serde_json::from_str::<nostr_sdk::Metadata>(&event.content)
                        {
                            author_metadata.set(Some(metadata));
                        }
                    }
                }
            }
        });
    }));
    use_effect(move || {
        let hovering = *is_hovering.read();
        let id = video_element_id_for_effect.clone();
        spawn(async move {
            let action = if hovering { "play" } else { "pause" };
            let js = format!(
                r#"(function() {{ var v = document.getElementById("{id}"); if (v) {{ if ("{action}" === "play") {{ v.play().catch(function(){{}}); }} else {{ v.pause(); v.currentTime = 0; }} }} }})()"#
            );
            let _ = document::eval(&js).await;
        });
    });
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(crate::stores::profiles::display_name_or_name)
        .unwrap_or_else(|| {
            let pk = event.pubkey.to_string();
            truncate_pubkey(&pk)
        });
    let video_id = event.id.to_hex();
    let video_kind = event.kind;
    let video_src = if video_meta.thumbnail.is_none() {
        video_meta.url.as_ref().map(|u| format!("{}#t=0.1", u))
    } else {
        video_meta.url.clone()
    };

    rsx! {
        div {
            class: "group cursor-pointer",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),
            Link {
                to: crate::routes::Route::AddressViewer {
                    address: crate::utils::nip19_urls::note_route_id_with_kind(&video_id, None, Some(video_kind)),
                },
                div { class: "relative aspect-video bg-muted rounded-lg overflow-hidden mb-3",
                    if let Some(thumbnail) = &video_meta.thumbnail {
                        img {
                            src: "{thumbnail}",
                            alt: "{video_meta.title.as_deref().unwrap_or(\"Video\")}",
                            class: if *is_hovering.read() && video_meta.url.is_some() {
                                "w-full h-full object-cover absolute inset-0 opacity-0"
                            } else {
                                "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                            },
                        }
                    }
                    if let Some(url) = &video_src {
                        video {
                            id: "{video_element_id}",
                            class: if video_meta.thumbnail.is_some() && !*is_hovering.read() {
                                "w-full h-full object-cover absolute inset-0 opacity-0"
                            } else {
                                "w-full h-full object-cover"
                            },
                            src: "{url}",
                            muted: true,
                            r#loop: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                    }
                    if video_meta.thumbnail.is_none() && video_meta.url.is_none() {
                        div { class: "w-full h-full flex items-center justify-center bg-muted",
                            crate::components::icons::VideoIcon { class: "w-12 h-12 text-muted-foreground" }
                        }
                    }
                    if let Some(duration) = &video_meta.duration {
                        div { class: "absolute bottom-2 right-2 bg-black/80 text-white text-xs px-2 py-1 rounded",
                            "{duration}"
                        }
                    }
                }
                div {
                    if let Some(title) = &video_meta.title {
                        h3 { class: "font-semibold line-clamp-2 mb-1 group-hover:text-primary transition",
                            "{title}"
                        }
                    }
                    p { class: "text-sm text-muted-foreground mb-1", "{display_name}" }
                    p { class: "text-xs text-muted-foreground",
                        {format_relative_time_or(event.created_at.as_secs(), "just now")}
                    }
                }
            }
        }
    }
}
#[component]
fn VertsVideoCard(event: Event, feed_type: FeedType) -> Element {
    let video_meta = parse_video_meta(&event);
    let mut is_hovering = use_signal(|| false);
    let video_element_id = format!("preview-vert-{}", &event.id.to_hex()[..12]);
    let video_element_id_for_effect = video_element_id.clone();
    use_effect(move || {
        let hovering = *is_hovering.read();
        let id = video_element_id_for_effect.clone();
        spawn(async move {
            let action = if hovering { "play" } else { "pause" };
            let js = format!(
                r#"(function() {{ var v = document.getElementById("{id}"); if (v) {{ if ("{action}" === "play") {{ v.play().catch(function(){{}}); }} else {{ v.pause(); v.currentTime = 0; }} }} }})()"#
            );
            let _ = document::eval(&js).await;
        });
    });
    let video_id = event.id.to_hex();
    let video_kind = event.kind;
    let video_src = if video_meta.thumbnail.is_none() {
        video_meta.url.as_ref().map(|u| format!("{}#t=0.1", u))
    } else {
        video_meta.url.clone()
    };

    rsx! {
        div {
            class: "group cursor-pointer",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),
            Link {
                to: crate::routes::Route::AddressViewer {
                    address: crate::utils::nip19_urls::note_route_id_with_kind(&video_id, None, Some(video_kind)),
                },
                div { class: "relative aspect-[9/16] bg-muted rounded-lg overflow-hidden mb-2",
                    if let Some(thumbnail) = &video_meta.thumbnail {
                        img {
                            src: "{thumbnail}",
                            alt: "{video_meta.title.as_deref().unwrap_or(\"Vert\")}",
                            class: if *is_hovering.read() && video_meta.url.is_some() {
                                "w-full h-full object-cover absolute inset-0 opacity-0"
                            } else {
                                "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                            },
                        }
                    }
                    if let Some(url) = &video_src {
                        video {
                            id: "{video_element_id}",
                            class: if video_meta.thumbnail.is_some() && !*is_hovering.read() {
                                "w-full h-full object-cover absolute inset-0 opacity-0"
                            } else {
                                "w-full h-full object-cover"
                            },
                            src: "{url}",
                            muted: true,
                            r#loop: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                    }
                    if video_meta.thumbnail.is_none() && video_meta.url.is_none() {
                        div { class: "w-full h-full flex items-center justify-center bg-muted",
                            crate::components::icons::VideoIcon { class: "w-8 h-8 text-muted-foreground" }
                        }
                    }
                    div { class: "absolute bottom-2 left-2 bg-black/80 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
                        crate::components::icons::VideoIcon { class: "w-3 h-3" }
                        "Vert"
                    }
                }
                if let Some(title) = &video_meta.title {
                    p { class: "text-sm font-medium line-clamp-2 group-hover:text-primary transition",
                        "{title}"
                    }
                }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
struct VideoMeta {
    url: Option<String>,
    thumbnail: Option<String>,
    title: Option<String>,
    duration: Option<String>,
    dimensions: Option<String>,
}
fn parse_video_meta(event: &Event) -> VideoMeta {
    let mut meta = VideoMeta {
        url: None,
        thumbnail: None,
        title: None,
        duration: None,
        dimensions: None,
    };
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("title") && slice.len() > 1 {
            meta.title = Some(slice[1].clone());
            break;
        }
    }
    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("imeta") {
            for field in slice.iter().skip(1) {
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
async fn load_featured_content() -> Result<Vec<Event>, String> {
    log::info!("Loading featured landscape videos...");
    if let Some(pubkey_str) = auth_store::get_pubkey() {
        match nostr_client::fetch_contacts(pubkey_str).await {
            Ok(contacts) if !contacts.is_empty() => {
                let mut authors = Vec::new();
                for contact in contacts.iter() {
                    if let Ok(pk) = PublicKey::parse(contact) {
                        authors.push(pk);
                    }
                }
                if !authors.is_empty() {
                    let filter = Filter::new()
                        .kinds(horizontal_kinds())
                        .authors(authors)
                        .limit(20);
                    let mut all_events = Vec::new();
                    nostr_client::stream_video_events_from_connected_relays_batched(
                        filter,
                        Duration::from_secs(10),
                        20,
                        |batch| {
                            all_events.extend(batch);
                        },
                    )
                    .await
                    .unwrap_or(0);
                    all_events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                    let deduped = dedupe_videos_by_url(all_events);
                    let landscape_vec: Vec<Event> = deduped.into_iter().take(3).collect();
                    if !landscape_vec.is_empty() {
                        return Ok(landscape_vec);
                    }
                }
            }
            _ => {}
        }
    }
    log::info!("Falling back to global feed for featured landscape videos");
    let filter = Filter::new().kinds(horizontal_kinds()).limit(20);
    let mut all_events = Vec::new();
    nostr_client::stream_video_events_from_connected_relays_batched(
        filter,
        Duration::from_secs(10),
        20,
        |batch| {
            all_events.extend(batch);
        },
    )
    .await
    .unwrap_or(0);
    all_events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    let deduped = dedupe_videos_by_url(all_events);
    let landscape_vec: Vec<Event> = deduped.into_iter().take(3).collect();
    Ok(landscape_vec)
}
async fn load_recent_verts() -> Result<Vec<Event>, String> {
    log::info!("Loading recent verts videos from Following feed...");
    let Some(pubkey_str) = auth_store::get_pubkey() else {
        return Ok(Vec::new());
    };
    match nostr_client::fetch_contacts(pubkey_str).await {
        Ok(contacts) if !contacts.is_empty() => {
            let mut authors = Vec::new();
            for contact in contacts.iter() {
                if let Ok(pk) = PublicKey::parse(contact) {
                    authors.push(pk);
                }
            }
            if !authors.is_empty() {
                let filter = Filter::new()
                    .kinds(vertical_kinds())
                    .authors(authors)
                    .limit(20);
                let mut all_events = Vec::new();
                nostr_client::stream_video_events_from_connected_relays_batched(
                    filter,
                    Duration::from_secs(10),
                    20,
                    |batch| {
                        all_events.extend(batch);
                    },
                )
                .await
                .unwrap_or(0);
                all_events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                let deduped = dedupe_videos_by_url(all_events);
                let verts_vec: Vec<Event> = deduped.into_iter().take(5).collect();
                return Ok(verts_vec);
            }
        }
        Ok(_) => {
            log::info!("User doesn't follow anyone, returning empty verts");
            return Ok(Vec::new());
        }
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}", e);
            return Ok(Vec::new());
        }
    }
    Ok(Vec::new())
}
async fn load_recent_live_streams() -> Result<Vec<Event>, String> {
    let following_live = load_following_live_streams(None, LiveStreamStatusFilter::Live)
        .await
        .map(|(events, _, _, _, _)| events)
        .unwrap_or_default();
    if !following_live.is_empty() {
        return Ok(following_live.into_iter().take(4).collect());
    }

    let (global_live, _, _) = load_global_live_streams(None, LiveStreamStatusFilter::Live).await?;
    Ok(global_live.into_iter().take(4).collect())
}

/// Load following horizontal videos from followed users.
/// Returns (events, has_more, did_fallback).
async fn load_following_horizontal_videos<F>(
    until: Option<u64>,
    mut on_batch: F,
) -> Result<(Vec<Event>, bool, bool), String>
where
    F: FnMut(Vec<Event>),
{
    let Some(pubkey_str) = auth_store::get_pubkey() else {
        let (events, has_more) = load_global_horizontal_videos(until, |_| {}).await?;
        return Ok((events, has_more, true));
    };
    log::info!(
        "Loading following horizontal videos feed for {} (until: {:?})",
        pubkey_str,
        until
    );
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!(
                "Failed to fetch contacts: {}, falling back to global feed",
                e
            );
            let (events, has_more) = load_global_horizontal_videos(until, |_| {}).await?;
            return Ok((events, has_more, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global horizontal videos");
        let (events, has_more) = load_global_horizontal_videos(until, |_| {}).await?;
        return Ok((events, has_more, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let (events, has_more) = load_global_horizontal_videos(until, |_| {}).await?;
        return Ok((events, has_more, true));
    }
    let mut filter = Filter::new()
        .kinds(horizontal_kinds())
        .authors(authors)
        .limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    }
    let mut events = Vec::new();
    let count = nostr_client::stream_video_events_from_connected_relays_batched(
        filter,
        Duration::from_secs(10),
        10,
        |batch| {
            events.extend(batch.clone());
            on_batch(batch);
        },
    )
    .await
    .map_err(|e| format!("Failed to stream following horizontal videos: {}", e))?;
    if events.is_empty() {
        log::info!("No horizontal videos from followed users");
        return Ok((Vec::new(), false, false));
    }
    events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    let deduped = dedupe_videos_by_url(events);
    log::info!(
        "Loaded {} horizontal video events from following (after dedup)",
        deduped.len()
    );
    let has_more = count >= 50;
    Ok((deduped, has_more, false))
}

async fn load_global_horizontal_videos<F>(
    until: Option<u64>,
    mut on_batch: F,
) -> Result<(Vec<Event>, bool), String>
where
    F: FnMut(Vec<Event>),
{
    log::info!(
        "Loading global horizontal videos feed (until: {:?})...",
        until
    );
    let mut filter = Filter::new().kinds(horizontal_kinds()).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts.saturating_sub(1)));
    }
    let mut events = Vec::new();
    let count = nostr_client::stream_video_events_from_connected_relays_batched(
        filter,
        Duration::from_secs(10),
        10,
        |batch| {
            events.extend(batch.clone());
            on_batch(batch);
        },
    )
    .await
    .map_err(|e| format!("Failed to stream global horizontal videos: {}", e))?;
    if events.is_empty() {
        return Err("Failed to load any content".to_string());
    }
    events.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    let deduped = dedupe_videos_by_url(events);
    log::info!(
        "Loaded {} global horizontal video events (after dedup)",
        deduped.len()
    );
    let has_more = count >= 50;
    Ok((deduped, has_more))
}
