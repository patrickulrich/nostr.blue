use crate::components::{TopicPostCard, TopicPostComposer};
use crate::hooks::{use_infinite_scroll, use_stale_guard};
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED, HAS_SIGNER};
use crate::stores::profiles::prefetch_profiles;
use crate::stores::subscription_manager;
use crate::stores::topic_store::{
    compute_hot_score, fetch_topic_posts, fetch_votes_batch, is_topic_post,
    is_topic_subscribed, parse_topic_post, query_topic_posts_from_db, query_votes_from_db,
    subscribe_to_topic, topic_posts_filter, unsubscribe_from_topic,
    TopicPost, VoteCounts,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};

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
    let mut loading = use_signal(|| true);
    let mut loading_new = use_signal(|| false);
    let mut pending_posts = use_signal(Vec::<TopicPost>::new);
    let pending_count = use_memo(move || pending_posts.read().len());
    let mut subscription_ids = use_signal(Vec::<SubscriptionId>::new);
    let mut stale = use_stale_guard();

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let current_topic = topic_sig.read().clone();
        let token = stale.bump();

        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        subscription_ids.write().clear();
        pending_posts.set(Vec::new());
        loading.set(true);
        loading_new.set(false);

        spawn(async move {
            if stale.is_stale(token) {
                return;
            }
            let is_stale = || stale.is_stale(token);

            let filter = topic_posts_filter(&current_topic, 30, None);

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
                vote_counts.write().extend(db_votes);
                has_more.set(db_posts.len() >= 30);
                posts.set(db_posts);
                loading.set(false);
                loading_new.set(true);
            }

            if is_stale() {
                return;
            }
            let result = fetch_topic_posts(&current_topic, 30, None).await;
            if is_stale() {
                return;
            }
            if let Ok(fetched) = result {
                let pubkeys: Vec<String> = fetched.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));
                let event_ids: Vec<EventId> = fetched
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                let user_pk =
                    auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                if let Ok(votes) = fetch_votes_batch(event_ids, user_pk).await {
                    vote_counts.write().extend(votes);
                }
                has_more.set(fetched.len() >= 30);
                posts.set(fetched);
            }
            loading.set(false);
            loading_new.set(false);

            if is_stale() {
                return;
            }
            if let Some(client) = nostr_client::get_client() {
                let topic_hashtag = format!("#{}", current_topic);
                let sub_filter = Filter::new()
                    .kind(Kind::Comment)
                    .custom_tags(SingleLetterTag::uppercase(Alphabet::I), [topic_hashtag])
                    .custom_tag(SingleLetterTag::uppercase(Alphabet::K), "#".to_string())
                    .since(Timestamp::now());

                match subscription_manager::subscribe_realtime(&client, sub_filter, Some(300))
                    .await
                {
                    Ok(sub_id) => {
                        subscription_ids.write().push(sub_id.clone());
                        let active_ids = subscription_ids;
                        let mut pending = pending_posts;
                        let current_posts = posts;
                        let _current_votes = vote_counts;
                        let stale_check = stale;
                        let stale_token = token;
                        spawn(async move {
                            let mut notifications = client.notifications();
                            loop {
                                if stale_check.is_stale(stale_token) {
                                    break;
                                }
                                let Ok(notification) = notifications.recv().await else {
                                    break;
                                };
                                if let RelayPoolNotification::Event {
                                    subscription_id: event_sub_id,
                                    event,
                                    ..
                                } = notification
                                {
                                    let active = active_ids.read();
                                    if !active.contains(&event_sub_id) {
                                        continue;
                                    }
                                    drop(active);

                                    if event.kind != Kind::Comment || !is_topic_post(&event) {
                                        continue;
                                    }
                                    if let Some(post) = parse_topic_post(&event) {
                                        if post.topic != current_topic {
                                            continue;
                                        }
                                        let already_buffered = pending
                                            .read()
                                            .iter()
                                            .any(|p| p.id == post.id);
                                        let already_in_feed = current_posts
                                            .read()
                                            .iter()
                                            .any(|p| p.id == post.id);
                                        if !already_buffered && !already_in_feed {
                                            let author_pk = post.pubkey.clone();
                                            spawn(async move {
                                                let _ =
                                                    crate::stores::profiles::fetch_profile(
                                                        author_pk,
                                                    )
                                                    .await;
                                            });
                                            pending.write().push(post);
                                        }
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::warn!("Failed to subscribe to topic real-time: {}", e);
                    }
                }
            }
        });
    });

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
                current_posts.into_iter().filter(|p| p.is_root).collect()
            }
        }
    };

    let mut accept_pending_posts = move || {
        let pending: Vec<TopicPost> = pending_posts.write().drain(..).collect();
        crate::stores::social::topic_store::merge_pending_posts(posts, pending);
    };

    let load_more = move || {
        let topic = topic_sig.read().clone();
        let current_posts = posts.read().clone();
        pagination_loading.set(true);
        spawn(async move {
            let until = current_posts.last().map(|p| p.created_at);
            if let Ok(new_posts) = fetch_topic_posts(&topic, 30, until).await {
                has_more.set(new_posts.len() >= 30);

                let new_unique: Vec<TopicPost> = {
                    let mut guard = posts.write();
                    let existing: HashSet<String> =
                        guard.iter().map(|p| p.id.clone()).collect();
                    let unique: Vec<TopicPost> = new_posts
                        .into_iter()
                        .filter(|p| !existing.contains(&p.id))
                        .collect();
                    guard.extend(unique.iter().cloned());
                    unique
                };

                let pubkeys: Vec<String> =
                    new_unique.iter().map(|p| p.pubkey.clone()).collect();
                spawn(prefetch_profiles(pubkeys));

                let event_ids: Vec<EventId> = new_unique
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                let user_pk =
                    auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                if let Ok(votes) = fetch_votes_batch(event_ids, user_pk).await {
                    vote_counts.write().extend(votes);
                }
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
            if *pending_count.read() > 0 {
                {
                    let count = *pending_count.read();
                    let post_text = if count == 1 { "post" } else { "posts" };
                    rsx! {
                        div {
                            class: "sticky top-[57px] z-20 border-b border-border bg-blue-500 hover:bg-blue-600 transition-colors cursor-pointer",
                            onclick: move |_| accept_pending_posts(),
                            div { class: "px-4 py-3 text-center",
                                span { class: "text-white font-medium", "Show {count} new {post_text}" }
                            }
                        }
                    }
                }
            }
            if *loading_new.read() && !posts.read().is_empty() {
                div {
                    class: "sticky top-[57px] z-20 border-b border-border bg-muted/80 backdrop-blur-sm",
                    div { class: "px-4 py-2 text-center",
                        span { class: "inline-flex items-center gap-2 text-sm text-muted-foreground",
                            span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                            "Loading new posts..."
                        }
                    }
                }
            }
            if *loading.read() && posts.read().is_empty() {
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
