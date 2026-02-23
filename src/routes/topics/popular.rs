//! Topics Popular Page
//! Hot-ranked posts and trending topics
use crate::components::{TopicPostCard, TopicSidebar};
use crate::stores::auth_store;
use crate::stores::profiles::prefetch_profiles;
use crate::stores::topic_store::{
    compute_hot_score, fetch_recent_posts, fetch_votes_batch,
    VoteCounts, ScoredPost, LOADING_TOPIC_POSTS,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

#[component]
pub fn TopicsPopular() -> Element {
    let mut scored_posts = use_signal(Vec::<ScoredPost>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let loading = *LOADING_TOPIC_POSTS.read();

    // Fetch and rank posts
    let _resource = use_resource(move || async move {
        let result = fetch_recent_posts(100, None).await;
        if let Ok(posts) = result {
            let pubkeys: Vec<String> = posts.iter().map(|p| p.pubkey.clone()).collect();
            spawn(prefetch_profiles(pubkeys));

            // Fetch votes for scoring
            let event_ids: Vec<EventId> = posts
                .iter()
                .filter_map(|p| EventId::from_hex(&p.id).ok())
                .collect();
            let user_pk = auth_store::get_pubkey()
                .and_then(|pk| PublicKey::from_hex(&pk).ok());
            let votes = fetch_votes_batch(event_ids, user_pk).await.unwrap_or_default();
            vote_counts.write().extend(votes.clone());

            // Compute hot scores
            let now = Timestamp::now().as_secs();
            let mut scored: Vec<ScoredPost> = posts
                .into_iter()
                .filter(|p| p.is_root) // Only top-level posts
                .map(|post| {
                    let vc = votes.get(&post.id).cloned().unwrap_or_default();
                    let score = compute_hot_score(&vc, 0, 0, post.created_at, now);
                    ScoredPost { post, score }
                })
                .collect();
            scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            scored_posts.set(scored);
        }
    });

    rsx! {
        div {
            class: "flex gap-6 max-w-6xl mx-auto px-4 py-4",
            // Sidebar
            div {
                class: "hidden lg:block w-64 shrink-0",
                TopicSidebar {}
            }
            // Main
            div {
                class: "flex-1 min-w-0",
                h1 { class: "text-2xl font-bold text-foreground mb-4", "Popular" }
                if loading && scored_posts.read().is_empty() {
                    div {
                        class: "flex justify-center py-12",
                        span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                    }
                } else if scored_posts.read().is_empty() {
                    div {
                        class: "text-center py-12 text-muted-foreground",
                        "No popular posts yet."
                    }
                } else {
                    div {
                        class: "flex flex-col gap-2",
                        for scored in scored_posts.read().iter() {
                            TopicPostCard {
                                key: "{scored.post.id}",
                                post: scored.post.clone(),
                                vote_counts: vote_counts.read().get(&scored.post.id).cloned(),
                                show_topic_badge: true,
                            }
                        }
                    }
                }
            }
        }
    }
}
