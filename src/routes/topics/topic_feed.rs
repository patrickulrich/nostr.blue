//! Topic Feed Page
//! Single topic: header, composer, posts with New/Hot/Top tabs
use crate::components::{TopicPostCard, TopicPostComposer};
use crate::hooks::use_infinite_scroll;
use crate::stores::auth_store;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::profiles::prefetch_profiles;
use crate::stores::topic_store::{
    compute_hot_score, fetch_topic_posts, fetch_votes_batch, is_topic_subscribed,
    subscribe_to_topic, unsubscribe_from_topic, TopicPost, VoteCounts, LOADING_TOPIC_POSTS,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

#[component]
pub fn TopicFeed(topic: String) -> Element {
    let mut topic_sig = use_signal(|| topic.clone());
    use_effect(use_reactive!(|topic| {
        topic_sig.set(topic);
    }));

    let mut sort_mode = use_signal(|| "new".to_string());
    let mut posts = use_signal(Vec::<TopicPost>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let mut has_more = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut subscribed = use_signal(|| is_topic_subscribed(&topic));
    let mut subscribing = use_signal(|| false);
    let has_signer = *HAS_SIGNER.read();
    let loading = *LOADING_TOPIC_POSTS.read();

    // Fetch posts for this topic
    let _resource = use_resource(move || {
        let topic = topic_sig.read().clone();
        async move {
            let result = fetch_topic_posts(&topic, 30, None).await;
            if let Ok(fetched) = result {
                let pubkeys: Vec<String> = fetched.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));

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

    // Sort posts based on mode
    let sorted_posts: Vec<TopicPost> = {
        let current_posts = posts.read().clone();
        let votes = vote_counts.read().clone();
        let mode = sort_mode.read().clone();
        match mode.as_str() {
            "hot" => {
                let now = Timestamp::now().as_secs();
                let mut scored: Vec<(TopicPost, f64)> = current_posts
                    .into_iter()
                    .filter(|p| p.is_root)
                    .map(|p| {
                        let vc = votes.get(&p.id).cloned().unwrap_or_default();
                        let score = compute_hot_score(&vc, 0, 0, p.created_at, now);
                        (p, score)
                    })
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                scored.into_iter().map(|(p, _)| p).collect()
            }
            "top" => {
                let mut top: Vec<(TopicPost, i64)> = current_posts
                    .into_iter()
                    .filter(|p| p.is_root)
                    .map(|p| {
                        let vc = votes.get(&p.id).cloned().unwrap_or_default();
                        let score = vc.score();
                        (p, score)
                    })
                    .collect();
                top.sort_by_key(|b| std::cmp::Reverse(b.1));
                top.into_iter().map(|(p, _)| p).collect()
            }
            _ => {
                // "new" - chronological, root posts only
                current_posts.into_iter().filter(|p| p.is_root).collect()
            }
        }
    };

    // Infinite scroll
    let load_more = move || {
        let topic = topic_sig.read().clone();
        let current_posts = posts.read().clone();
        pagination_loading.set(true);
        spawn(async move {
            let until = current_posts.last().map(|p| p.created_at);
            if let Ok(new_posts) = fetch_topic_posts(&topic, 30, until).await {
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
    let topic_val = topic_sig.read().clone();

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            div {
                class: "flex items-center justify-between mb-4",
                h1 { class: "text-2xl font-bold text-foreground", "#{topic_val}" }
                if has_signer {
                    button {
                        class: if *subscribed.read() {
                            "px-4 py-1.5 text-sm font-medium rounded-md border border-border text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition"
                        } else {
                            "px-4 py-1.5 text-sm font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition"
                        },
                        disabled: *subscribing.read(),
                        onclick: move |_| {
                            let topic = topic_sig.read().clone();
                            let currently = *subscribed.read();
                            subscribing.set(true);
                            spawn(async move {
                                let result = if currently {
                                    unsubscribe_from_topic(&topic).await
                                } else {
                                    subscribe_to_topic(&topic).await
                                };
                                if result.is_ok() {
                                    subscribed.set(!currently);
                                }
                                subscribing.set(false);
                            });
                        },
                        if *subscribing.read() { "..." }
                        else if *subscribed.read() { "Subscribed" }
                        else { "Subscribe" }
                    }
                }
            }
            if has_signer {
                TopicPostComposer {
                    topic: Some(topic_val.clone()),
                    on_success: move |_: String| {
                        // TODO: prepend new post to list
                    },
                }
                div { class: "h-4" }
            }
            div {
                class: "flex gap-1 mb-4 border-b border-border",
                for (mode, label) in [("new", "New"), ("hot", "Hot"), ("top", "Top")] {
                    {
                        let tab_class = if *sort_mode.read() == mode {
                            "px-4 py-2 text-sm font-medium transition border-b-2 border-primary text-primary"
                        } else {
                            "px-4 py-2 text-sm font-medium transition text-muted-foreground hover:text-foreground"
                        };
                        rsx! {
                            button {
                                key: "{mode}",
                                class: "{tab_class}",
                                onclick: move |_| sort_mode.set(mode.to_string()),
                                "{label}"
                            }
                        }
                    }
                }
            }
            if loading && posts.read().is_empty() {
                div {
                    class: "flex justify-center py-12",
                    span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                }
            } else if sorted_posts.is_empty() {
                div {
                    class: "text-center py-12 text-muted-foreground",
                    "No posts in this topic yet. Be the first to post!"
                }
            } else {
                div {
                    class: "flex flex-col gap-2",
                    for post in &sorted_posts {
                        TopicPostCard {
                            key: "{post.id}",
                            post: post.clone(),
                            vote_counts: vote_counts.read().get(&post.id).cloned(),
                        }
                    }
                }
            }
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
