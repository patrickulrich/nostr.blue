//! Topics Home Page
//! Tabs: "Your Feed" (subscribed topics) / "Recent" (all topics)
use crate::components::{TopicPostCard, TopicSidebar};
use crate::hooks::use_infinite_scroll;
use crate::stores::auth_store;
use crate::stores::profiles::prefetch_profiles;
use crate::stores::topic_store::{
    fetch_recent_posts, fetch_subscribed_feed, fetch_subscriptions, fetch_votes_batch,
    get_subscribed_topic_names, TopicPost, VoteCounts, LOADING_TOPIC_POSTS,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

#[component]
pub fn TopicsHome() -> Element {
    let mut active_tab = use_signal(|| "recent".to_string());
    let mut posts = use_signal(Vec::<TopicPost>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let mut has_more = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let loading = *LOADING_TOPIC_POSTS.read();

    // Fetch subscriptions on mount
    let _subscriptions = use_resource(move || async move {
        if let Some(pk) = auth_store::get_pubkey() {
            if let Ok(pubkey) = PublicKey::from_hex(&pk) {
                let _ = fetch_subscriptions(pubkey).await;
            }
        }
    });

    // Fetch posts when tab changes
    let tab = active_tab.read().clone();
    let _posts_resource = use_resource(move || {
        let tab = tab.clone();
        async move {
            let result = if tab == "feed" {
                let subscribed = get_subscribed_topic_names();
                if subscribed.is_empty() {
                    Ok(Vec::new())
                } else {
                    fetch_subscribed_feed(&subscribed, 30, None).await
                }
            } else {
                fetch_recent_posts(30, None).await
            };

            if let Ok(fetched) = result {
                // Prefetch author profiles
                let pubkeys: Vec<String> = fetched.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));

                // Fetch votes
                let event_ids: Vec<EventId> = fetched
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                let user_pk = auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                if let Ok(votes) = fetch_votes_batch(event_ids, user_pk).await {
                    vote_counts.write().extend(votes);
                }

                has_more.set(fetched.len() >= 30);
                posts.set(fetched);
            }
        }
    });

    // Infinite scroll
    let load_more = move || {
        let tab = active_tab.read().clone();
        let current_posts = posts.read().clone();
        pagination_loading.set(true);
        spawn(async move {
            let until = current_posts.last().map(|p| p.created_at);
            let result = if tab == "feed" {
                let subscribed = get_subscribed_topic_names();
                fetch_subscribed_feed(&subscribed, 30, until).await
            } else {
                fetch_recent_posts(30, until).await
            };

            if let Ok(new_posts) = result {
                has_more.set(new_posts.len() >= 30);
                let pubkeys: Vec<String> = new_posts.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));

                let event_ids: Vec<EventId> = new_posts
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                let user_pk = auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                if let Ok(votes) = fetch_votes_batch(event_ids, user_pk).await {
                    vote_counts.write().extend(votes);
                }

                posts.write().extend(new_posts);
            }
            pagination_loading.set(false);
        });
    };

    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    rsx! {
        div {
            class: "flex gap-6 max-w-6xl mx-auto px-4 py-4",
            // Sidebar (desktop)
            div {
                class: "hidden lg:block w-64 shrink-0",
                TopicSidebar {}
            }
            // Main content
            div {
                class: "flex-1 min-w-0",
                // Header
                h1 { class: "text-2xl font-bold text-foreground mb-4", "Topics" }
                // Tabs
                {
                    let tab_val = active_tab.read().clone();
                    let feed_class = if tab_val == "feed" {
                        "px-4 py-2 text-sm font-medium transition border-b-2 border-primary text-primary"
                    } else {
                        "px-4 py-2 text-sm font-medium transition text-muted-foreground hover:text-foreground"
                    };
                    let recent_class = if tab_val == "recent" {
                        "px-4 py-2 text-sm font-medium transition border-b-2 border-primary text-primary"
                    } else {
                        "px-4 py-2 text-sm font-medium transition text-muted-foreground hover:text-foreground"
                    };
                    rsx! {
                        div {
                            class: "flex gap-1 mb-4 border-b border-border",
                            button {
                                class: "{feed_class}",
                                onclick: move |_| active_tab.set("feed".to_string()),
                                "Your Feed"
                            }
                            button {
                                class: "{recent_class}",
                                onclick: move |_| active_tab.set("recent".to_string()),
                                "Recent"
                            }
                        }
                    }
                }
                // Posts
                if loading && posts.read().is_empty() {
                    div {
                        class: "flex justify-center py-12",
                        span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                    }
                } else if posts.read().is_empty() {
                    div {
                        class: "text-center py-12 text-muted-foreground",
                        if *active_tab.read() == "feed" {
                            p { "No posts in your subscribed topics yet." }
                            p { class: "text-sm mt-1", "Browse topics to subscribe and build your feed." }
                        } else {
                            p { "No topic posts found." }
                        }
                    }
                } else {
                    div {
                        class: "flex flex-col gap-2",
                        for post in posts.read().iter() {
                            TopicPostCard {
                                key: "{post.id}",
                                post: post.clone(),
                                vote_counts: vote_counts.read().get(&post.id).cloned(),
                                show_topic_badge: true,
                            }
                        }
                    }
                }
                // Infinite scroll sentinel
                if *has_more.read() {
                    div {
                        id: "{sentinel_id}",
                        class: "p-8 flex justify-center",
                        if *pagination_loading.read() {
                            span { class: "flex items-center gap-2 text-muted-foreground",
                                span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading more..."
                            }
                        }
                    }
                }
            }
        }
    }
}
