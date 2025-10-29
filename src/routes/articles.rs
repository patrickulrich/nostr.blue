use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{ArticleCard, ArticleCardSkeleton};
use crate::hooks::use_infinite_scroll;
use crate::utils::article_meta::get_identifier;
use nostr_sdk::Event;
use std::collections::HashMap;

#[component]
pub fn Articles() -> Element {
    // State for feed events
    let mut articles = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load articles on mount and when refresh is triggered
    use_effect(move || {
        let _ = refresh_trigger.read();

        loading.set(true);
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);

        spawn(async move {
            match load_articles(None).await {
                Ok(feed_events) => {
                    // Track oldest timestamp for pagination
                    if let Some(last_event) = feed_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                    }

                    // Determine if there are more events to load
                    has_more.set(feed_events.len() >= 20);

                    articles.set(feed_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Load more function for infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();

        loading.set(true);

        spawn(async move {
            match load_articles(until).await {
                Ok(mut new_articles) => {
                    // Track oldest timestamp from new events
                    if let Some(last_event) = new_articles.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                    }

                    // Determine if there are more events to load
                    has_more.set(new_articles.len() >= 20);

                    // Append new events to existing events
                    let mut current = articles.read().clone();
                    current.append(&mut new_articles);
                    articles.set(current);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more articles: {}", e);
                    loading.set(false);
                }
            }
        });
    };

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(
        load_more,
        has_more,
        loading
    );

    let article_list = articles.read();
    let is_loading = *loading.read();
    let error_msg = error.read();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    h2 {
                        class: "text-xl font-bold",
                        "📚 Articles"
                    }
                    button {
                        class: "text-sm px-3 py-1 rounded-lg hover:bg-accent transition",
                        onclick: move |_| {
                            let current = *refresh_trigger.peek();
                            refresh_trigger.set(current + 1);
                        },
                        "↻ Refresh"
                    }
                }
            }

            // Error message
            if let Some(err) = error_msg.as_ref() {
                div {
                    class: "p-4 bg-destructive/10 border border-destructive text-destructive",
                    p { "Failed to load articles: {err}" }
                    button {
                        class: "mt-2 px-3 py-1 bg-destructive text-destructive-foreground rounded-lg",
                        onclick: move |_| {
                            let current = *refresh_trigger.peek();
                            refresh_trigger.set(current + 1);
                        },
                        "Try Again"
                    }
                }
            }

            // Articles grid
            div {
                class: "p-4",

                // Initial loading state
                if is_loading && article_list.is_empty() {
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for _ in 0..6 {
                            ArticleCardSkeleton {}
                        }
                    }
                } else if article_list.is_empty() {
                    // Empty state
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "📚"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "No Articles Found"
                        }
                        p {
                            class: "text-muted-foreground text-sm mb-4",
                            "Check back later for long-form content from the Nostr network."
                        }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90",
                            onclick: move |_| {
                                let current = *refresh_trigger.peek();
                                refresh_trigger.set(current + 1);
                            },
                            "Refresh"
                        }
                    }
                } else {
                    // Article grid
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for article in article_list.iter() {
                            ArticleCard {
                                key: "{article.id}",
                                event: article.clone(),
                            }
                        }
                    }

                    // Infinite scroll sentinel
                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "h-20 flex items-center justify-center",
                            if is_loading {
                                div {
                                    class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full",
                                    for _ in 0..3 {
                                        ArticleCardSkeleton {}
                                    }
                                }
                            }
                        }
                    } else {
                        // End of feed indicator
                        div {
                            class: "text-center py-8 text-muted-foreground text-sm",
                            "You've reached the end"
                        }
                    }
                }
            }
        }
    }
}

/// Load articles with deduplication by address (kind:pubkey:identifier)
async fn load_articles(until: Option<u64>) -> Result<Vec<Event>, String> {
    // Fetch articles from the client
    let raw_articles = nostr_client::fetch_articles(20, until).await?;

    // Deduplicate by address (kind:pubkey:identifier)
    // Keep only the most recent version of each article
    let mut address_map: HashMap<String, Event> = HashMap::new();

    for article in raw_articles {
        // Only include articles with valid identifiers
        if let Some(identifier) = get_identifier(&article) {
            let address = format!("{}:{}:{}",
                article.kind.as_u16(),
                article.pubkey.to_hex(),
                identifier
            );

            // Keep the newest version (replace if newer)
            address_map.entry(address)
                .and_modify(|existing| {
                    if article.created_at > existing.created_at {
                        *existing = article.clone();
                    }
                })
                .or_insert(article);
        }
    }

    // Convert back to vec and sort by created_at descending
    let mut deduplicated: Vec<Event> = address_map.into_values().collect();
    deduplicated.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(deduplicated)
}
