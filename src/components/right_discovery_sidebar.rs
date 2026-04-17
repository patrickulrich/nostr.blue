use crate::components::icons::{ArrowLeftIcon, TrendingUpIcon};
use crate::routes::Route;
use crate::services::search::sidebar_discovery::{
    self, load_hot_post_source, load_trend_source, save_hot_post_source, save_trend_source,
    HotPostItem, HotPostSource, NostrarchivesNote, TrendItem, TrendSource,
};
use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr_sdk::{Event, EventId, ToBech32};

const MAX_TAGS: usize = 5;
const MAX_HOT_POSTS: usize = 5;

#[component]
pub fn RightDiscoverySidebar() -> Element {
    let mut hot_post_source = use_signal(load_hot_post_source);
    let mut hot_posts = use_signal(Vec::<HotPostItem>::new);
    let mut hot_posts_loading = use_signal(|| true);
    let mut hot_posts_error = use_signal(|| None::<String>);

    let mut trend_source = use_signal(load_trend_source);
    let mut trend_items = use_signal(Vec::<TrendItem>::new);
    let mut tag_sparklines = use_signal(std::collections::HashMap::<String, Vec<u32>>::new);
    let mut tags_loading = use_signal(|| true);
    let mut tags_error = use_signal(|| None::<String>);

    use_effect(move || {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            return;
        }
        let source = *hot_post_source.read();
        spawn(async move {
            hot_posts_loading.set(true);
            hot_posts_error.set(None);
            match sidebar_discovery::get_hot_posts(source, MAX_HOT_POSTS).await {
                Ok(items) => {
                    sidebar_discovery::prefetch_author_profiles(&items).await;
                    hot_posts.set(filter_hot_posts(items).await);
                }
                Err(err) => hot_posts_error.set(Some(err)),
            }
            hot_posts_loading.set(false);
        });
    });

    use_effect(move || {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            return;
        }
        let source = *trend_source.read();
        spawn(async move {
            tags_loading.set(true);
            tags_error.set(None);
            match source {
                TrendSource::Ditto => {
                    match sidebar_discovery::get_ditto_trending_tags(MAX_TAGS).await {
                        Ok(result) => {
                            let tag_names: Vec<String> =
                                result.tags.iter().map(|tag| tag.tag.clone()).collect();
                            let sparklines = sidebar_discovery::get_ditto_tag_sparklines(
                                &tag_names,
                                result.label_created_at,
                            )
                            .await
                            .unwrap_or_default();
                            tag_sparklines.set(sparklines);
                            trend_items.set(
                                result
                                    .tags
                                    .into_iter()
                                    .map(TrendItem::Ditto)
                                    .collect(),
                            );
                        }
                        Err(err) => tags_error.set(Some(err)),
                    }
                }
                TrendSource::Nostrarchives => {
                    tag_sparklines.set(std::collections::HashMap::new());
                    match sidebar_discovery::get_nostrarchives_trending_tags(MAX_TAGS).await {
                        Ok(tags) => {
                            trend_items.set(
                                tags.into_iter()
                                    .map(TrendItem::Nostrarchives)
                                    .collect(),
                            );
                        }
                        Err(err) => tags_error.set(Some(err)),
                    }
                }
            }
            tags_loading.set(false);
        });
    });

    rsx! {
        div { class: "h-full min-h-0 overflow-y-auto scrollbar-hide pr-1 space-y-3",
            HotPostsPanel {
                source: *hot_post_source.read(),
                items: hot_posts.read().clone(),
                loading: *hot_posts_loading.read(),
                error: hot_posts_error.read().clone(),
                on_toggle: move |_| {
                    let next = hot_post_source.read().cycle(true);
                    if let Err(err) = save_hot_post_source(next) {
                        log::warn!("Failed to persist hot post source: {err}");
                    }
                    hot_post_source.set(next);
                },
            }
            TrendsPanel {
                source: *trend_source.read(),
                items: trend_items.read().clone(),
                sparklines: tag_sparklines.read().clone(),
                loading: *tags_loading.read(),
                error: tags_error.read().clone(),
                on_toggle: move |_| {
                    let next = trend_source.read().cycle(true);
                    if let Err(err) = save_trend_source(next) {
                        log::warn!("Failed to persist trend source: {err}");
                    }
                    trend_source.set(next);
                },
            }
        }
    }
}

#[component]
fn HotPostsPanel(
    source: HotPostSource,
    items: Vec<HotPostItem>,
    loading: bool,
    error: Option<String>,
    on_toggle: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        section { class: "rounded-xl border border-border bg-card p-3",
            div { class: "mb-2 flex items-start justify-between gap-3",
                div {
                    h3 { class: "text-lg font-bold text-foreground", "Hot Posts" }
                    Link {
                        to: Route::Trending {
                            source: Some(source.query_value().to_string()),
                        },
                        class: "text-xs text-muted-foreground hover:text-primary hover:underline",
                        "Source: {source.label()}"
                    }
                }
                button {
                    class: "rounded-lg p-1.5 text-muted-foreground transition hover:bg-accent hover:text-foreground",
                    onclick: move |evt| on_toggle.call(evt),
                    title: "Next source",
                    ArrowLeftIcon { class: "h-4 w-4 rotate-180".to_string() }
                }
            }
            if loading {
                SidebarLoading {}
            } else if let Some(err) = error {
                SidebarError { message: err }
            } else if items.is_empty() {
                SidebarEmpty { message: "No hot posts right now".to_string() }
            } else {
                div { class: "space-y-1.5",
                    for item in items {
                        match item {
                            HotPostItem::NostrWine(note) => rsx! {
                                NostrWineHotPostCard { key: "{note.event.id}", note }
                            },
                            HotPostItem::Ditto(event) => rsx! {
                                DittoHotPostCard { key: "{event.id}", event }
                            },
                            HotPostItem::Nostrarchives(note) => rsx! {
                                NostrarchivesHotPostCard { key: "{note.id}", note }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TrendsPanel(
    source: TrendSource,
    items: Vec<TrendItem>,
    sparklines: std::collections::HashMap<String, Vec<u32>>,
    loading: bool,
    error: Option<String>,
    on_toggle: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        section { class: "rounded-xl border border-border bg-card p-3",
            div { class: "mb-2 flex items-center justify-between gap-3",
                h3 { class: "text-lg font-bold text-foreground", "Trends" }
                div { class: "flex items-center gap-1",
                    TrendingUpIcon { class: "h-4 w-4 text-muted-foreground".to_string() }
                    span { class: "text-xs text-muted-foreground", "{source.label()}" }
                    button {
                        class: "rounded-lg p-1 text-muted-foreground transition hover:bg-accent hover:text-foreground",
                        onclick: move |evt| on_toggle.call(evt),
                        title: "Next trend source",
                        ArrowLeftIcon { class: "h-3.5 w-3.5 rotate-180".to_string() }
                    }
                }
            }
            if loading {
                SidebarLoading {}
            } else if let Some(err) = error {
                SidebarError { message: err }
            } else if items.is_empty() {
                SidebarEmpty { message: "No trends available".to_string() }
            } else {
                div { class: "space-y-1.5",
                    for item in items {
                        match &item {
                            TrendItem::Ditto(data) => rsx! {
                                DittoTrendRow {
                                    key: "{data.tag}",
                                    tag: data.tag.clone(),
                                    accounts: data.accounts,
                                    uses: data.uses,
                                    sparkline: sparklines.get(&data.tag).cloned().unwrap_or_default(),
                                }
                            },
                            TrendItem::Nostrarchives(data) => rsx! {
                                NostrarchivesTrendRow {
                                    key: "{data.hashtag}",
                                    hashtag: data.hashtag.clone(),
                                    count: data.count,
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DittoTrendRow(
    tag: String,
    accounts: u32,
    uses: u32,
    sparkline: Vec<u32>,
) -> Element {
    rsx! {
        Link {
            to: Route::Hashtag { tag: tag.clone() },
            class: "flex items-center justify-between rounded-lg px-2 py-1.5 transition hover:bg-accent/50",
            div {
                p { class: "font-semibold text-sm", "#{tag}" }
                p { class: "text-xs text-muted-foreground",
                    "{accounts} people • {uses} uses"
                }
            }
            TrendSparkline { data: sparkline }
        }
    }
}

#[component]
fn NostrarchivesTrendRow(hashtag: String, count: u64) -> Element {
    rsx! {
        Link {
            to: Route::Hashtag { tag: hashtag.clone() },
            class: "flex items-center justify-between rounded-lg px-2 py-1.5 transition hover:bg-accent/50",
            div {
                p { class: "font-semibold text-sm", "#{hashtag}" }
                p { class: "text-xs text-muted-foreground",
                    "{count} posts"
                }
            }
        }
    }
}

#[component]
fn NostrWineHotPostCard(note: crate::services::trending::TrendingNote) -> Element {
    let author_name = sidebar_discovery::profile_display_name(&note.event.pubkey);
    let picture = sidebar_discovery::profile_avatar(&note.event.pubkey);
    let content = crate::services::trending::truncate_content(&note.event.content, 100);
    let note_bech32 = EventId::from_hex(&note.event.id)
        .ok()
        .and_then(|id| id.to_bech32().ok())
        .unwrap_or_else(|| note.event.id.clone());

    rsx! {
        Link {
            to: Route::Note { note_id: note_bech32, from_voice: None },
            class: "block rounded-lg border border-border/60 p-2.5 transition hover:bg-accent/40",
            div { class: "flex gap-3",
                img {
                    src: "{picture}",
                    alt: "{author_name}",
                    class: "h-10 w-10 rounded-full object-cover shrink-0",
                    loading: "lazy",
                }
                div { class: "min-w-0 flex-1",
                    p { class: "truncate text-sm font-semibold", "{author_name}" }
                    p { class: "mt-1 line-clamp-2 text-sm text-muted-foreground", "{content}" }
                }
            }
        }
    }
}

#[component]
fn DittoHotPostCard(event: Event) -> Element {
    let pubkey_hex = event.pubkey.to_hex();
    let author_name = sidebar_discovery::profile_display_name(&pubkey_hex);
    let picture = sidebar_discovery::profile_avatar(&pubkey_hex);
    let content = truncate_event_content(&event);
    let note_bech32 = event.id.to_bech32().unwrap_or_else(|_| event.id.to_hex());

    rsx! {
        Link {
            to: Route::Note { note_id: note_bech32, from_voice: None },
            class: "block rounded-lg border border-border/60 p-2.5 transition hover:bg-accent/40",
            div { class: "flex gap-3",
                img {
                    src: "{picture}",
                    alt: "{author_name}",
                    class: "h-10 w-10 rounded-full object-cover shrink-0",
                    loading: "lazy",
                }
                div { class: "min-w-0 flex-1",
                    p { class: "truncate text-sm font-semibold", "{author_name}" }
                    p { class: "mt-1 line-clamp-2 text-sm text-muted-foreground", "{content}" }
                }
            }
        }
    }
}

#[component]
fn NostrarchivesHotPostCard(note: NostrarchivesNote) -> Element {
    let author_name = sidebar_discovery::profile_display_name(&note.pubkey);
    let picture = sidebar_discovery::profile_avatar(&note.pubkey);
    let content = truncate_content_str(&note.content, 100);
    let note_bech32 = EventId::from_hex(&note.id)
        .ok()
        .and_then(|id| id.to_bech32().ok())
        .unwrap_or_else(|| note.id.clone());

    rsx! {
        Link {
            to: Route::Note { note_id: note_bech32, from_voice: None },
            class: "block rounded-lg border border-border/60 p-2.5 transition hover:bg-accent/40",
            div { class: "flex gap-3",
                img {
                    src: "{picture}",
                    alt: "{author_name}",
                    class: "h-10 w-10 rounded-full object-cover shrink-0",
                    loading: "lazy",
                }
                div { class: "min-w-0 flex-1",
                    p { class: "truncate text-sm font-semibold", "{author_name}" }
                    p { class: "mt-1 line-clamp-2 text-sm text-muted-foreground", "{content}" }
                }
            }
        }
    }
}

#[component]
fn SidebarLoading() -> Element {
    rsx! {
        div { class: "flex items-center justify-center py-6",
            span { class: "inline-block h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" }
        }
    }
}

#[component]
fn SidebarError(message: String) -> Element {
    rsx! { p { class: "py-3 text-sm text-muted-foreground", "{message}" } }
}

#[component]
fn SidebarEmpty(message: String) -> Element {
    rsx! { p { class: "py-3 text-sm text-muted-foreground", "{message}" } }
}

#[component]
fn TrendSparkline(data: Vec<u32>) -> Element {
    let path = sparkline_path(&data);
    if path.is_empty() {
        return rsx! { div { class: "h-7 w-12 rounded bg-muted/70" } };
    }

    rsx! {
        svg {
            class: "h-7 w-12 text-primary/70",
            width: "50",
            height: "28",
            view_box: "0 0 50 28",
            fill: "none",
            path {
                d: "{path}",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}

fn sparkline_path(data: &[u32]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let width = 50.0_f64;
    let height = 28.0_f64;
    let margin = 2.0_f64;
    let min = *data.iter().min().unwrap_or(&0) as f64;
    let max = *data.iter().max().unwrap_or(&0) as f64;
    let hfactor = if data.len() > 1 {
        (width - margin * 2.0) / (data.len() as f64 - 1.0)
    } else {
        width - margin * 2.0
    };
    let vfactor = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        (height - margin * 2.0) / (max - min)
    };

    let mut path = String::new();
    for (index, value) in data.iter().enumerate() {
        let x = margin + index as f64 * hfactor;
        let y = if (max - min).abs() < f64::EPSILON {
            height / 2.0
        } else {
            margin + (max - *value as f64) * vfactor
        };
        if index == 0 {
            path.push_str(&format!("M{:.2} {:.2}", x, y));
        } else {
            path.push_str(&format!(" L{:.2} {:.2}", x, y));
        }
    }
    path
}

fn truncate_event_content(event: &Event) -> String {
    let clean = event
        .content
        .split_whitespace()
        .filter(|part| !part.starts_with("http://") && !part.starts_with("https://"))
        .collect::<Vec<_>>()
        .join(" ");
    if clean.is_empty() {
        "(media)".to_string()
    } else if clean.len() > 100 {
        format!("{}...", &clean[..100])
    } else {
        clean
    }
}

fn truncate_content_str(content: &str, max_length: usize) -> String {
    if content.len() <= max_length {
        content.to_string()
    } else {
        let truncate_at = content
            .char_indices()
            .take_while(|(idx, _)| *idx < max_length)
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0);
        format!("{}...", content[..truncate_at].trim())
    }
}

async fn filter_hot_posts(items: Vec<HotPostItem>) -> Vec<HotPostItem> {
    let mute_data = nostr_client::get_mute_list_data().await.unwrap_or_default();
    items
        .into_iter()
        .filter(|item| match item {
            HotPostItem::NostrWine(note) => {
                !mute_data.blocked_users.contains(&note.event.pubkey)
                    && !mute_data.muted_posts.contains(&note.event.id)
            }
            HotPostItem::Ditto(event) => {
                let event_id = event.id.to_hex();
                let pubkey = event.pubkey.to_hex();
                !mute_data.blocked_users.contains(&pubkey)
                    && !mute_data.muted_posts.contains(&event_id)
            }
            HotPostItem::Nostrarchives(note) => {
                !mute_data.blocked_users.contains(&note.pubkey)
                    && !mute_data.muted_posts.contains(&note.id)
            }
        })
        .collect()
}
