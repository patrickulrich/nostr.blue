use crate::components::icons;
use crate::hooks::{use_nostr_resource, NostrResourceState};
use crate::routes::podcast::podcast_shared_states::{
    PodcastApiAuthRequiredState, PodcastApiInitializingState,
};
use crate::routes::Route;
use crate::services::podcast_index;
use crate::stores::podcast_subscription;
use dioxus::prelude::*;
#[component]
pub fn PodcastTrending() -> Element {
    let mut selected_category = use_signal(|| None::<String>);
    let podcasts = use_nostr_resource(move || {
        let cat = selected_category.read().clone();
        async move {
            let feeds = podcast_index::get_trending(Some(50), cat.as_deref()).await?;
            log::info!("Fetched {} trending podcasts", feeds.len());
            #[cfg(feature = "mobile_platform")]
            {
                if let Ok(json) = serde_json::to_string(&feeds) {
                    let _ = crate::platform::android_media::save_browse_cache("trending_podcasts", &json);
                }
            }
            Ok(feeds)
        }
    });
    let podcasts_state = podcasts.state();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center gap-4",
                    Link {
                        to: Route::PodcastHome {},
                        class: "p-2 hover:bg-muted rounded-full transition",
                        dangerous_inner_html: icons::ARROW_LEFT,
                    }
                    h1 { class: "text-xl font-bold", "Trending Podcasts" }
                }
            }
            match &*podcasts_state.read() {
                NostrResourceState::Initializing => rsx! {
                    PodcastApiInitializingState { item_label: "trending podcasts" }
                },
                NostrResourceState::AuthRequired => rsx! {
                    PodcastApiAuthRequiredState { item_label: "trending podcasts" }
                },
                NostrResourceState::Loading => rsx! {
                    div { class: "p-4 space-y-4",
                        div { class: "flex flex-wrap gap-2",
                            for cat in podcast_subscription::get_categories() {
                                div { class: "px-3 py-1.5 text-sm rounded-full bg-muted animate-pulse", "{cat.name}" }
                            }
                        }
                        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                            for i in 0..20 {
                                TrendingCardSkeleton { key: "{i}" }
                            }
                        }
                    }
                },
                NostrResourceState::Error(err) => rsx! {
                    div { class: "text-center py-16 text-destructive",
                        "Failed to load trending podcasts: {err}"
                    }
                },
                NostrResourceState::Loaded(feeds) => rsx! {
                    div { class: "p-4 space-y-4",
                        div { class: "flex flex-wrap gap-2",
                            CategoryChip {
                                label: "All",
                                selected: selected_category.read().is_none(),
                                onclick: move |_| selected_category.set(None),
                            }
                            for cat in podcast_subscription::get_categories() {
                                CategoryChip {
                                    label: cat.name,
                                    selected: selected_category.read().as_ref().is_some_and(|c| c == cat.name),
                                    onclick: move |_| selected_category.set(Some(cat.name.to_string())),
                                }
                            }
                        }
                        if feeds.is_empty() {
                            div { class: "text-center py-16 text-muted-foreground",
                                "No trending podcasts found"
                            }
                        } else {
                            div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                                for feed in feeds.iter() {
                                    TrendingCard { key: "{feed.id}", feed: feed.clone() }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct CategoryChipProps {
    label: &'static str,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
}
#[component]
fn CategoryChip(props: CategoryChipProps) -> Element {
    rsx! {
        button {
            class: if props.selected { "px-3 py-1.5 text-sm font-medium rounded-full bg-primary text-primary-foreground" } else { "px-3 py-1.5 text-sm font-medium rounded-full bg-muted hover:bg-muted/80 transition" },
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
        }
    }
}
#[derive(Props, Clone, PartialEq)]
struct TrendingCardProps {
    feed: podcast_index::PodcastFeed,
}
#[component]
fn TrendingCard(props: TrendingCardProps) -> Element {
    let feed = &props.feed;
    let image = feed.get_image();
    let podcast_id = feed.id.to_string();
    rsx! {
        Link {
            to: Route::PodcastRssFeedDetail {
                podcast_id,
            },
            class: "group block bg-card border border-border rounded-lg overflow-hidden hover:border-primary/50 transition",
            div { class: "relative aspect-square bg-muted",
                if let Some(img) = image {
                    img {
                        src: "{img}",
                        alt: "{feed.title}",
                        class: "w-full h-full object-cover",
                        loading: "lazy",
                        referrerpolicy: "no-referrer",
                    }
                } else {
                    div {
                        class: "w-full h-full flex items-center justify-center text-muted-foreground",
                        dangerous_inner_html: icons::PODCAST,
                    }
                }
                if feed.has_v4v() {
                    div { class: "absolute top-2 left-2 px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/90 text-white rounded",
                        "V4V"
                    }
                }
            }
            div { class: "p-2",
                p { class: "text-sm font-medium line-clamp-2", "{feed.title}" }
                if let Some(ref author) = feed.author {
                    p { class: "text-xs text-muted-foreground truncate mt-0.5", "{author}" }
                }
            }
        }
    }
}
#[component]
fn TrendingCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            div { class: "aspect-square bg-muted" }
            div { class: "p-2 space-y-1",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
        }
    }
}
