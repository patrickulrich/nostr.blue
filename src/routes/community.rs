use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::{NoteCard, NoteCardSkeleton};
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;

use super::communities::Community;

#[component]
pub fn CommunityPage(a_tag: String) -> Element {
    let mut community = use_signal(|| None::<Community>);
    let mut posts = use_signal(|| Vec::<Event>::new());
    let mut loading_community = use_signal(|| true);
    let mut loading_posts = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    let a_tag_for_community = a_tag.clone();
    let a_tag_for_posts = a_tag.clone();
    let a_tag_for_load_more = a_tag.clone();

    // Fetch community metadata
    use_effect(move || {
        let a_tag_str = a_tag_for_community.clone();

        loading_community.set(true);
        error.set(None);

        spawn(async move {
            match fetch_community(&a_tag_str).await {
                Ok(Some(comm)) => {
                    community.set(Some(comm));
                    loading_community.set(false);
                }
                Ok(None) => {
                    error.set(Some("Community not found".to_string()));
                    loading_community.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading_community.set(false);
                }
            }
        });
    });

    // Fetch community posts
    use_effect(move || {
        let a_tag_str = a_tag_for_posts.clone();

        loading_posts.set(true);

        spawn(async move {
            match fetch_community_posts(&a_tag_str, None).await {
                Ok(community_posts) => {
                    if let Some(last) = community_posts.last() {
                        oldest_timestamp.set(Some(last.created_at.as_u64()));
                    }
                    has_more.set(community_posts.len() >= 50);
                    posts.set(community_posts);
                    loading_posts.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load posts: {}", e);
                    loading_posts.set(false);
                }
            }
        });
    });

    // Load more function
    let mut load_more = move || {
        if *loading_posts.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        let a_tag_str = a_tag_for_load_more.clone();
        loading_posts.set(true);

        spawn(async move {
            match fetch_community_posts(&a_tag_str, until).await {
                Ok(mut new_posts) => {
                    if let Some(last) = new_posts.last() {
                        oldest_timestamp.set(Some(last.created_at.as_u64()));
                    }
                    has_more.set(new_posts.len() >= 50);

                    let mut current = posts.read().clone();
                    current.append(&mut new_posts);
                    posts.set(current);
                    loading_posts.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more posts: {}", e);
                    loading_posts.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
                        "←"
                    }
                    if let Some(comm) = community.read().as_ref() {
                        div {
                            h2 {
                                class: "text-xl font-bold",
                                "{comm.name.as_ref().unwrap_or(&comm.d_tag)}"
                            }
                            if !posts.read().is_empty() {
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "{posts.read().len()} posts"
                                }
                            }
                        }
                    }
                }
            }

            // Community Info
            if *loading_community.read() {
                div {
                    class: "p-4 animate-pulse",
                    div {
                        class: "h-24 bg-muted rounded-lg"
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            } else if let Some(comm) = community.read().as_ref() {
                div {
                    class: "border-b border-border p-4",

                    div {
                        class: "flex items-start gap-3 mb-3",

                        // Community avatar
                        if let Some(image_url) = &comm.image {
                            img {
                                class: "w-16 h-16 rounded-full object-cover",
                                src: "{image_url}",
                                alt: "Community image"
                            }
                        } else {
                            div {
                                class: "w-16 h-16 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white text-2xl",
                                "👥"
                            }
                        }

                        // Community info
                        div {
                            class: "flex-1",
                            h1 {
                                class: "text-2xl font-bold mb-1",
                                "{comm.name.as_ref().unwrap_or(&comm.d_tag)}"
                            }
                            p {
                                class: "text-muted-foreground text-sm",
                                "{comm.d_tag}"
                            }
                        }
                    }

                    // Description
                    if let Some(desc) = &comm.description {
                        p {
                            class: "text-sm mb-3 whitespace-pre-wrap",
                            "{desc}"
                        }
                    }

                    // Stats
                    div {
                        class: "flex gap-4 text-sm text-muted-foreground",
                        if !comm.moderators.is_empty() {
                            span {
                                "👤 {comm.moderators.len()} moderator"
                                if comm.moderators.len() != 1 { "s" }
                            }
                        }
                    }
                }
            }

            // Posts
            div {
                if *loading_posts.read() && posts.read().is_empty() {
                    div {
                        class: "divide-y divide-border",
                        for _ in 0..5 {
                            NoteCardSkeleton {}
                        }
                    }
                } else if !posts.read().is_empty() {
                    div {
                        class: "divide-y divide-border",
                        for post in posts.read().iter() {
                            NoteCard {
                                event: post.clone()
                            }
                        }
                    }

                    // Load More button
                    if *has_more.read() {
                        div {
                            class: "p-4 flex justify-center",
                            button {
                                class: "px-6 py-3 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50",
                                disabled: *loading_posts.read(),
                                onclick: move |_| load_more(),
                                if *loading_posts.read() {
                                    span {
                                        class: "flex items-center gap-2",
                                        span {
                                            class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                        }
                                        "Loading..."
                                    }
                                } else {
                                    "Load More"
                                }
                            }
                        }
                    } else if !posts.read().is_empty() {
                        div {
                            class: "p-8 text-center text-muted-foreground",
                            "You've reached the end"
                        }
                    }
                } else if !*loading_posts.read() {
                    // Empty state
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "💬"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "No posts yet"
                        }
                        p {
                            class: "text-muted-foreground",
                            "Be the first to post in this community"
                        }
                    }
                }
            }
        }
    }
}

// Fetch a specific community by a_tag
async fn fetch_community(a_tag: &str) -> Result<Option<Community>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    log::info!("Fetching community: {}", a_tag);

    // Parse a_tag: "34550:pubkey:d_tag"
    let parts: Vec<&str> = a_tag.split(':').collect();
    if parts.len() != 3 {
        return Err("Invalid a_tag format".to_string());
    }

    let pubkey_hex = parts[1];
    let d_tag = parts[2];

    // Parse pubkey
    let pubkey = PublicKey::from_hex(pubkey_hex)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch community event
    let filter = Filter::new()
        .kind(Kind::Custom(34550))
        .author(pubkey)
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::D),
            d_tag.to_string()
        )
        .limit(1);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                Ok(super::communities::parse_community_event(event))
            } else {
                Ok(None)
            }
        }
        Err(e) => {
            log::error!("Failed to fetch community: {}", e);
            Err(format!("Failed to fetch community: {}", e))
        }
    }
}

// Fetch posts for a community
async fn fetch_community_posts(a_tag: &str, until: Option<u64>) -> Result<Vec<Event>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    log::info!("Fetching posts for community: {}", a_tag);

    // Create filter for both kind 1111 (NIP-72) and kind 1 (backwards compatibility)
    let mut filter = Filter::new()
        .kinds(vec![Kind::Custom(1111), Kind::TextNote])
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
            a_tag.to_string()
        )
        .limit(50);

    // Add until timestamp for pagination
    if let Some(until_ts) = until {
        let timestamp = Timestamp::from(until_ts);
        filter = filter.until(timestamp);
    }

    // Fetch events
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("Fetched {} posts", event_vec.len());
            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch community posts: {}", e);
            Err(format!("Failed to fetch posts: {}", e))
        }
    }
}

