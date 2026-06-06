use crate::components::topic::PopularSidebar;
use crate::components::TopicPostCard;
use crate::hooks::use_stale_guard;
use crate::stores::auth_store;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::profiles::prefetch_profiles;
use crate::stores::topic_store::{
    compute_hot_score, fetch_recent_posts, fetch_votes_batch, query_topic_posts_from_db,
    query_votes_from_db, recent_topic_posts_filter, ScoredPost, TimeRange, VoteCounts,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

#[component]
pub fn TopicsPopular() -> Element {
    let mut scored_posts = use_signal(Vec::<ScoredPost>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let mut loading = use_signal(|| true);
    let mut loading_new = use_signal(|| false);
    let mut time_range = use_signal(TimeRange::default);
    let mut stale = use_stale_guard();

    use_effect(use_reactive!(|(time_range,)| {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let token = stale.bump();
        loading.set(true);
        loading_new.set(false);

        let since = time_range().since_secs();

        spawn(async move {
            if stale.is_stale(token) {
                return;
            }
            let is_stale = || stale.is_stale(token);

            let filter = recent_topic_posts_filter(100, None, since);

            let db_posts = query_topic_posts_from_db(filter.clone()).await;
            if is_stale() {
                return;
            }
            if !db_posts.is_empty() {
                let pubkeys: Vec<String> = db_posts.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));
                let event_ids: Vec<EventId> = db_posts
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                let user_pk =
                    auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                let db_votes = query_votes_from_db(event_ids, user_pk).await;
                vote_counts.write().extend(db_votes.clone());

                let now = Timestamp::now().as_secs();
                let mut scored: Vec<ScoredPost> = db_posts
                    .into_iter()
                    .filter(|p| p.is_root)
                    .map(|post| {
                        let vc = db_votes.get(&post.id).cloned().unwrap_or_default();
                        let score = compute_hot_score(&vc, 0, 0, post.created_at, now);
                        ScoredPost { post, score }
                    })
                    .collect();
                scored.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                scored_posts.set(scored);
                loading.set(false);
                loading_new.set(true);
            }

            if is_stale() {
                return;
            }
            let result = fetch_recent_posts(100, None).await;
            if is_stale() {
                return;
            }
            if let Ok(posts) = result {
                let pubkeys: Vec<String> = posts.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));

                let event_ids: Vec<EventId> = posts
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                let user_pk =
                    auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                let votes = fetch_votes_batch(event_ids, user_pk)
                    .await
                    .unwrap_or_default();
                vote_counts.write().extend(votes.clone());

                let now = Timestamp::now().as_secs();
                let mut scored: Vec<ScoredPost> = posts
                    .into_iter()
                    .filter(|p| p.is_root)
                    .map(|post| {
                        let vc = votes.get(&post.id).cloned().unwrap_or_default();
                        let score = compute_hot_score(&vc, 0, 0, post.created_at, now);
                        ScoredPost { post, score }
                    })
                    .collect();
                scored.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                if let Some(min_ts) = since {
                    scored.retain(|s| s.post.created_at >= min_ts);
                }

                scored_posts.set(scored);
            }
            loading.set(false);
            loading_new.set(false);
        });
    }));

    let ranges = [TimeRange::Day, TimeRange::Week, TimeRange::All];

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            h1 { class: "text-2xl font-bold text-foreground mb-4", "Popular" }
            div {
                class: "inline-flex items-center rounded-lg bg-muted p-1 mb-4",
                for range in ranges {
                    {
                        let current = *time_range.read();
                        let class = if current == range {
                            "px-3 py-1.5 text-sm font-medium rounded-md bg-background text-foreground shadow-sm transition".to_string()
                        } else {
                            "px-3 py-1.5 text-sm font-medium rounded-md text-muted-foreground hover:text-foreground transition".to_string()
                        };
                        let r = range;
                        rsx! {
                            button {
                                key: "{r.label()}",
                                class: "{class}",
                                onclick: move |_| time_range.set(r),
                                "{r.label()}"
                            }
                        }
                    }
                }
            }
            div {
                class: "flex gap-6",
                div {
                    class: "flex-1 min-w-0",
                    if *loading.read() && scored_posts.read().is_empty() {
                        div {
                            class: "flex justify-center py-12",
                            span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        }
                    } else if *loading_new.read() && !scored_posts.read().is_empty() {
                        div {
                            class: "flex flex-col gap-2",
                            div {
                                class: "sticky top-[57px] z-20 border-b border-border bg-muted/80 backdrop-blur-sm",
                                div { class: "px-4 py-2 text-center",
                                    span { class: "inline-flex items-center gap-2 text-sm text-muted-foreground",
                                        span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                        "Loading new posts..."
                                    }
                                }
                            }
                            for scored in scored_posts.read().iter() {
                                TopicPostCard {
                                    key: "{scored.post.id}",
                                    post: scored.post.clone(),
                                    vote_counts: vote_counts.read().get(&scored.post.id).cloned(),
                                    show_topic_badge: true,
                                }
                            }
                        }
                    } else if scored_posts.read().is_empty() {
                        div {
                            class: "text-center py-12 text-muted-foreground",
                            "No popular posts found for this time range."
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
                div {
                    class: "hidden lg:block w-72 shrink-0",
                    PopularSidebar { posts: scored_posts.read().clone() }
                }
            }
        }
    }
}
