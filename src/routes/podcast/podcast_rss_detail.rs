//! RSS Podcast Detail Route
//!
//! Shows an RSS podcast fetched via Podcast Index ID:
//! - Podcast metadata (cover, title, description)
//! - Episode list with Podcasting 2.0 features
//! - V4V payment support (if available)
//! - Chapters, transcripts, soundbites
use crate::components::{
    icons, ContentShareModal, ContentType, DisplayEpisode, PodcastEpisodeList,
};
use crate::hooks::use_infinite_scroll;
use crate::routes::podcast::podcast_shared_states::{
    PodcastApiAuthRequiredState, PodcastApiInitializingState,
};
use crate::routes::Route;
use crate::services::podcast_index::{self, PodcastFeed};
use crate::stores::{auth_store, nostr_client, podcast_subscription};
use crate::utils::markdown::sanitize_html;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

enum RssPodcastDetailState {
    Initializing,
    Loaded(Box<PodcastFeed>, u64),
    AuthRequired,
    Error(String),
}

#[derive(Props, Clone, PartialEq)]
pub struct PodcastRssFeedDetailProps {
    pub podcast_id: String,
}
/// RSS podcast detail page (loaded via Podcast Index ID)
#[component]
pub fn PodcastRssFeedDetail(props: PodcastRssFeedDetailProps) -> Element {
    let podcast_id = props.podcast_id.clone();
    let mut refresh_trigger = use_signal(|| 0u32);
    // Only fetch feed metadata, episodes are loaded incrementally
    let podcast_data = use_resource(move || {
        let id_str = podcast_id.clone();
        async move {
            let refresh = *refresh_trigger.read();
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            let has_signer = nostr_client::has_signer();
            if !client_initialized {
                return RssPodcastDetailState::Initializing;
            }
            let id: u64 = match id_str.parse() {
                Ok(id) => id,
                Err(_) => {
                    return RssPodcastDetailState::Error(format!("Invalid podcast ID: {}", id_str))
                }
            };
            if !has_signer {
                return RssPodcastDetailState::AuthRequired;
            }
            log::info!(
                "Fetching podcast metadata for ID: {} (refresh: {})",
                id,
                refresh
            );
            match podcast_index::get_podcast_by_id(id).await {
                Ok(feed) => {
                    log::info!("Successfully loaded podcast: {}", feed.title);
                    RssPodcastDetailState::Loaded(Box::new(feed), id)
                }
                Err(error) => RssPodcastDetailState::Error(error),
            }
        }
    });
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-4",
                    Link {
                        to: Route::PodcastHome {},
                        class: "p-2 hover:bg-muted rounded-full transition",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-xl font-bold", "Podcast" }
                }
            }
            match &*podcast_data.read() {
                Some(RssPodcastDetailState::Initializing) => rsx! {
                    PodcastApiInitializingState {
                        item_label: "podcast",
                    }
                },
                Some(RssPodcastDetailState::Loaded(feed, id)) => rsx! {
                    RssPodcastDetailContent { feed: (**feed).clone(), podcast_id: *id }
                },
                Some(RssPodcastDetailState::AuthRequired) => rsx! {
                    PodcastApiAuthRequiredState {
                        item_label: "podcast",
                    }
                },
                Some(RssPodcastDetailState::Error(e)) => rsx! {
                    div { class: "p-4 text-center space-y-3",
                        div { class: "text-destructive mb-2", "Failed to load podcast" }
                        div { class: "text-sm text-muted-foreground", "{e}" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v = v.wrapping_add(1));
                            },
                            "Try Again"
                        }
                    }
                },
                None => rsx! {
                    RssPodcastDetailSkeleton {}
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RssPodcastDetailContentProps {
    feed: PodcastFeed,
    podcast_id: u64,
}
#[component]
fn RssPodcastDetailContent(props: RssPodcastDetailContentProps) -> Element {
    let feed = props.feed.clone();
    let podcast_id = props.podcast_id;
    let auth = auth_store::AUTH_STATE.read();
    let toast = consume_toast();
    let mut show_share_modal = use_signal(|| false);

    // Infinite scroll state
    let mut episodes = use_signal(Vec::<DisplayEpisode>::new);
    let mut loading_more = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut initial_load_started = use_signal(|| false);
    let mut initial_load_complete = use_signal(|| false);

    // Initial episode load
    {
        let feed = feed.clone();
        use_effect(move || {
            if *initial_load_started.read() {
                return;
            }
            initial_load_started.set(true);
            let feed = feed.clone();
            spawn(async move {
                log::info!("Loading initial episodes for podcast {}", podcast_id);
                match podcast_index::get_episodes_by_feed_id(podcast_id, Some(30), None).await {
                    Ok(eps) => {
                        log::info!("Loaded {} initial episodes", eps.len());
                        has_more.set(eps.len() >= 30);
                        let display_eps: Vec<DisplayEpisode> = eps
                            .iter()
                            .map(|ep| DisplayEpisode::from_podcast_index_episode(ep, &feed))
                            .collect();
                        episodes.set(display_eps);
                    }
                    Err(e) => log::error!("Failed to load episodes: {}", e),
                }
                initial_load_complete.set(true);
            });
        });
    }

    // Load more callback - only runs after initial load is complete
    let load_more =
        {
            let feed = feed.clone();
            move || {
                // Don't load more until initial load is complete
                if !*initial_load_complete.read() || *loading_more.read() || !*has_more.read() {
                    return;
                }
                let current_count = episodes.peek().len();
                let feed = feed.clone();
                loading_more.set(true);
                spawn(async move {
                    log::info!("Loading more episodes, skip: {}", current_count);
                    match podcast_index::get_episodes_by_feed_id(
                        podcast_id,
                        Some(30),
                        Some(current_count),
                    )
                    .await
                    {
                        Ok(new_eps) => {
                            log::info!("Loaded {} more episodes", new_eps.len());
                            if new_eps.is_empty() {
                                has_more.set(false);
                            } else {
                                has_more.set(new_eps.len() >= 30);
                                episodes.write().extend(new_eps.iter().map(|ep| {
                                    DisplayEpisode::from_podcast_index_episode(ep, &feed)
                                }));
                            }
                        }
                        Err(e) => {
                            has_more.set(false);
                            log::error!("Load more failed: {}", e);
                        }
                    }
                    loading_more.set(false);
                });
            }
        };

    let sentinel_id = use_infinite_scroll(load_more, has_more, loading_more);

    let image_url = feed.get_image().map(|s| s.to_string()).unwrap_or_else(|| {
        format!(
            "https://api.dicebear.com/7.x/shapes/svg?seed={}",
            feed.title
        )
    });
    let has_v4v = feed.has_v4v();
    let podcast_guid = feed.podcast_guid.clone();
    let feed_url = feed.url.clone();
    let podcast_guid_for_memo = podcast_guid.clone();
    let is_subscribed = use_memo(move || {
        if let Some(ref guid) = podcast_guid_for_memo {
            podcast_subscription::is_subscribed(guid)
        } else {
            podcast_subscription::is_subscribed(&podcast_id.to_string())
        }
    });
    let mut subscribing = use_signal(|| false);
    let category_names: Vec<String> = feed
        .categories
        .as_ref()
        .map(|cats| cats.values().cloned().collect())
        .unwrap_or_default();
    let safe_description = feed.description.as_ref().map(|d| sanitize_html(d));

    let episode_count = episodes.read().len();

    rsx! {
        div {
            div { class: "relative",
                div { class: "absolute inset-0 h-48 bg-gradient-to-b from-green-500/20 to-background" }
                div { class: "relative p-6",
                    div { class: "flex gap-6",
                        img {
                            src: "{image_url}",
                            alt: "{feed.title}",
                            class: "w-32 h-32 md:w-40 md:h-40 rounded-lg object-cover shadow-lg",
                        }
                        div { class: "flex-1 min-w-0",
                            h1 { class: "text-2xl font-bold truncate", "{feed.title}" }
                            if let Some(ref author) = feed.author {
                                p { class: "text-muted-foreground mt-1", "{author}" }
                            }
                            div { class: "flex items-center gap-2 mt-2 flex-wrap",
                                span { class: "px-2 py-1 text-xs bg-green-500/20 text-green-400 rounded-full font-medium",
                                    "RSS"
                                }
                                if has_v4v {
                                    span {
                                        class: "px-2 py-1 text-xs bg-amber-500/20 text-amber-400 rounded-full font-medium flex items-center gap-1",
                                        dangerous_inner_html: icons::ZAP,
                                        "V4V Enabled"
                                    }
                                }
                            }
                            if !category_names.is_empty() {
                                div { class: "flex items-center gap-1 mt-2 flex-wrap",
                                    for cat in category_names.iter().take(4) {
                                        span {
                                            key: "{cat}",
                                            class: "px-2 py-0.5 text-xs bg-muted text-muted-foreground rounded",
                                            "{cat}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-3 mt-4",
                        if auth.is_authenticated {
                            button {
                                class: if *is_subscribed.read() || *subscribing.read() { "px-4 py-2 text-sm font-medium border border-border rounded-full hover:bg-muted transition" } else { "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-full hover:bg-primary/90 transition" },
                                disabled: *subscribing.read(),
                                onclick: {
                                    let url = feed_url.clone();
                                    let guid = podcast_guid.clone();
                                    let id = podcast_id;
                                    move |_| {
                                        if *subscribing.read() {
                                            return;
                                        }
                                        let url = url.clone();
                                        let guid = guid.clone();
                                        let sub_id = guid.clone().unwrap_or_else(|| id.to_string());
                                        let currently_subscribed = podcast_subscription::is_subscribed(&sub_id);
                                        subscribing.set(true);
                                        spawn(async move {
                                            if currently_subscribed {
                                                match podcast_subscription::remove_subscription(&sub_id).await {
                                                    Ok(()) => log::info!("Unsubscribed from podcast: {}", sub_id),
                                                    Err(e) => log::error!("Failed to unsubscribe: {}", e),
                                                }
                                            } else if let Some(ref guid) = guid {
                                                match podcast_subscription::add_rss_subscription(
                                                        guid,
                                                        Some(id),
                                                        Some(&url),
                                                    )
                                                    .await
                                                {
                                                    Ok(()) => {
                                                        log::info!(
                                                            "Subscribed to podcast: {} (guid: {})", url, guid
                                                        )
                                                    }
                                                    Err(e) => {
                                                        log::error!("Failed to subscribe: {}", e);
                                                        toast.error(
                                                            format!("Cannot subscribe: {}", e),
                                                            ToastOptions::new(),
                                                        );
                                                    }
                                                }
                                            } else {
                                                log::error!("Cannot subscribe: podcast does not have a GUID");
                                                toast.error(
                                                    "Cannot subscribe: podcast missing identifier"
                                                        .to_string(),
                                                    ToastOptions::new(),
                                                );
                                            }
                                            subscribing.set(false);
                                        });
                                    }
                                },
                                if *subscribing.read() {
                                    "..."
                                } else if *is_subscribed.read() {
                                    "Subscribed"
                                } else {
                                    "Subscribe"
                                }
                            }
                        }
                        if has_v4v && auth.is_authenticated {
                            button {
                                class: "px-4 py-2 text-sm font-medium bg-amber-500 text-white rounded-full hover:bg-amber-600 transition flex items-center gap-2",
                                title: "Send sats",
                                dangerous_inner_html: icons::ZAP,
                                "Boost"
                            }
                        }
                        a {
                            href: "{feed.url}",
                            target: "_blank",
                            class: "px-4 py-2 text-sm font-medium border border-border rounded-full hover:bg-muted transition",
                            "RSS Feed"
                        }
                        if let Some(ref link) = feed.link {
                            a {
                                href: "{link}",
                                target: "_blank",
                                class: "p-2 hover:bg-muted rounded-full transition",
                                title: "Visit website",
                                dangerous_inner_html: icons::EXTERNAL_LINK,
                            }
                        }
                        button {
                            class: "p-2 hover:bg-muted rounded-full transition",
                            title: "Share",
                            onclick: move |_| show_share_modal.set(true),
                            dangerous_inner_html: icons::SHARE,
                        }
                    }
                }
            }
            if *show_share_modal.read() {
                ContentShareModal {
                    title: feed.title.clone(),
                    url: format!("https://nostr.blue/podcast/rss/{}", podcast_id),
                    content_type: ContentType::Podcast,
                    image_url: feed.get_image().map(String::from),
                    on_close: move |_| show_share_modal.set(false),
                }
            }
            if let Some(ref desc) = safe_description {
                div { class: "px-6 py-4 border-b border-border",
                    div {
                        class: "text-sm text-muted-foreground prose prose-sm dark:prose-invert max-w-none",
                        dangerous_inner_html: "{desc}",
                    }
                }
            }
            div { class: "p-6",
                div { class: "flex items-center justify-between mb-4",
                    h2 { class: "font-semibold text-lg", "Episodes" }
                    span { class: "text-sm text-muted-foreground",
                        "{episode_count} episodes"
                        if *has_more.read() { "+" } else { "" }
                    }
                }
                PodcastEpisodeList {
                    episodes: episodes.read().clone(),
                    show_podcast_title: false,
                    enable_playlist: true,
                }
                // Sentinel element for infinite scroll
                if *has_more.read() {
                    div {
                        id: "{sentinel_id}",
                        class: "h-20 flex items-center justify-center",
                        if *loading_more.read() {
                            div { class: "flex items-center gap-3 text-muted-foreground",
                                span { class: "w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading more episodes..."
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Skeleton loader for RSS podcast detail
#[component]
fn RssPodcastDetailSkeleton() -> Element {
    rsx! {
        div { class: "animate-pulse",
            div { class: "p-6",
                div { class: "flex gap-6",
                    div { class: "w-32 h-32 md:w-40 md:h-40 bg-muted rounded-lg" }
                    div { class: "flex-1 space-y-3",
                        div { class: "h-8 bg-muted rounded w-3/4" }
                        div { class: "h-4 bg-muted rounded w-1/2" }
                        div { class: "flex gap-2",
                            div { class: "h-6 bg-muted rounded-full w-12" }
                            div { class: "h-6 bg-muted rounded-full w-20" }
                        }
                    }
                }
            }
            div { class: "px-6 py-4 border-b border-border space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }
            div { class: "p-6 space-y-4",
                div { class: "h-6 bg-muted rounded w-24" }
                for i in 0..5 {
                    div { key: "{i}", class: "flex gap-4 p-3",
                        div { class: "w-16 h-16 bg-muted rounded-lg" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-full" }
                            div { class: "h-3 bg-muted rounded w-1/4" }
                        }
                    }
                }
            }
        }
    }
}
