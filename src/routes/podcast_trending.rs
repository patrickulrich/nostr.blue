//! Podcast Trending Page
//!
//! Full page view of trending podcasts from Podcast Index API

use dioxus::prelude::*;
use crate::components::icons;
use crate::routes::Route;
use crate::services::podcast_index;
use crate::stores::{nostr_client, podcast_subscription};

/// Full trending podcasts page
#[component]
pub fn PodcastTrending() -> Element {
    let mut podcasts = use_signal(|| None::<Vec<podcast_index::PodcastFeed>>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut selected_category = use_signal(|| None::<String>);

    // Fetch trending podcasts (waits for client - NIP-98 auth required)
    {
        let category = selected_category.read().clone();
        use_effect(move || {
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
            let has_signer = nostr_client::has_signer();

            // Wait for nostr client AND signer - NIP-98 auth requires both
            if !client_initialized || !has_signer {
                return;
            }

            let cat = category.clone();
            loading.set(true);
            error.set(None);

            spawn(async move {
                match podcast_index::get_trending(Some(50), cat.as_deref()).await {
                    Ok(feeds) => {
                        log::info!("Fetched {} trending podcasts", feeds.len());
                        podcasts.set(Some(feeds));
                        loading.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to fetch trending podcasts: {}", e);
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            });
        });
    }

    rsx! {
        div {
            class: "min-h-screen",

            // Header with back button
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center gap-4",
                    Link {
                        to: Route::PodcastHome {},
                        class: "p-2 hover:bg-muted rounded-full transition",
                        dangerous_inner_html: icons::ARROW_LEFT
                    }
                    h1 {
                        class: "text-xl font-bold",
                        "Trending Podcasts"
                    }
                }
            }

            // Content
            div {
                class: "p-4 space-y-4",

                // Category filter chips
                div {
                    class: "flex flex-wrap gap-2",
                    CategoryChip {
                        label: "All",
                        selected: selected_category.read().is_none(),
                        onclick: move |_| selected_category.set(None)
                    }
                    for cat in podcast_subscription::get_categories() {
                        CategoryChip {
                            label: cat.name,
                            selected: selected_category.read().as_ref().is_some_and(|c| c == cat.name),
                            onclick: move |_| selected_category.set(Some(cat.name.to_string()))
                        }
                    }
                }

                // Results
                if *loading.read() {
                    div {
                        class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                        for i in 0..20 {
                            TrendingCardSkeleton { key: "{i}" }
                        }
                    }
                } else if let Some(ref err) = *error.read() {
                    div {
                        class: "text-center py-16 text-destructive",
                        "Failed to load trending podcasts: {err}"
                    }
                } else if let Some(ref feeds) = *podcasts.read() {
                    if feeds.is_empty() {
                        div {
                            class: "text-center py-16 text-muted-foreground",
                            "No trending podcasts found"
                        }
                    } else {
                        div {
                            class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3",
                            for feed in feeds.iter() {
                                TrendingCard {
                                    key: "{feed.id}",
                                    feed: feed.clone()
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Category filter chip
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
            class: if props.selected {
                "px-3 py-1.5 text-sm font-medium rounded-full bg-primary text-primary-foreground"
            } else {
                "px-3 py-1.5 text-sm font-medium rounded-full bg-muted hover:bg-muted/80 transition"
            },
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
        }
    }
}

/// Trending podcast card
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
            to: Route::PodcastRssFeedDetail { podcast_id },
            class: "group block bg-card border border-border rounded-lg overflow-hidden hover:border-primary/50 transition",

            // Image
            div {
                class: "relative aspect-square bg-muted",
                if let Some(img) = image {
                    img {
                        src: "{img}",
                        alt: "{feed.title}",
                        class: "w-full h-full object-cover",
                        loading: "lazy"
                    }
                } else {
                    div {
                        class: "w-full h-full flex items-center justify-center text-muted-foreground",
                        dangerous_inner_html: icons::PODCAST
                    }
                }

                // V4V badge
                if feed.has_v4v() {
                    div {
                        class: "absolute top-2 left-2 px-1.5 py-0.5 text-[10px] font-semibold bg-amber-500/90 text-white rounded",
                        "V4V"
                    }
                }
            }

            // Title and author
            div {
                class: "p-2",
                p {
                    class: "text-sm font-medium line-clamp-2",
                    "{feed.title}"
                }
                if let Some(ref author) = feed.author {
                    p {
                        class: "text-xs text-muted-foreground truncate mt-0.5",
                        "{author}"
                    }
                }
            }
        }
    }
}

#[component]
fn TrendingCardSkeleton() -> Element {
    rsx! {
        div {
            class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            div {
                class: "aspect-square bg-muted"
            }
            div {
                class: "p-2 space-y-1",
                div {
                    class: "h-4 bg-muted rounded w-3/4"
                }
                div {
                    class: "h-3 bg-muted rounded w-1/2"
                }
            }
        }
    }
}
