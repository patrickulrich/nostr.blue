use crate::components::TopicPostCard;
use crate::hooks::{use_infinite_scroll, use_stale_guard};
use crate::stores::auth_store;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::stores::profiles::prefetch_profiles;
use crate::stores::subscription_manager;
use crate::stores::topic_store::{
    fetch_recent_posts, fetch_subscribed_feed, fetch_subscriptions, fetch_votes_batch,
    get_subscribed_topic_names, is_topic_post, query_topic_posts_from_db, query_votes_from_db,
    recent_topic_posts_filter, topic_to_filter_urls, TopicPost, VoteCounts,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};

#[component]
pub fn TopicsHome() -> Element {
    let mut active_tab = use_signal(|| "recent".to_string());
    let mut posts = use_signal(Vec::<TopicPost>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let mut has_more = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut loading_new = use_signal(|| false);
    let mut pending_posts = use_signal(Vec::<TopicPost>::new);
    let pending_count = use_memo(move || pending_posts.read().len());
    let mut subscription_ids = use_signal(Vec::<SubscriptionId>::new);
    let mut stale = use_stale_guard();

    let _subscriptions = use_resource(move || async move {
        if let Some(pk) = auth_store::get_pubkey() {
            if let Ok(pubkey) = PublicKey::from_hex(&pk) {
                let _ = fetch_subscriptions(pubkey).await;
            }
        }
    });

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let tab = active_tab.read().clone();
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

            let filter = if tab == "feed" {
                let subscribed = get_subscribed_topic_names();
                if subscribed.is_empty() {
                    loading.set(false);
                    posts.set(Vec::new());
                    return;
                }
                let topic_urls: Vec<String> = subscribed
                    .iter()
                    .flat_map(|t| topic_to_filter_urls(t))
                    .collect();
                Filter::new()
                    .kind(Kind::Comment)
                    .custom_tags(SingleLetterTag::uppercase(Alphabet::I), topic_urls)
                    .custom_tag(SingleLetterTag::uppercase(Alphabet::K), "web".to_string())
                    .limit(30)
            } else {
                recent_topic_posts_filter(30, None)
            };

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
            let result = if tab == "feed" {
                let subscribed = get_subscribed_topic_names();
                fetch_subscribed_feed(&subscribed, 30, None).await
            } else {
                fetch_recent_posts(30, None).await
            };

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
                let sub_filter = if tab == "feed" {
                    let subscribed = get_subscribed_topic_names();
                    if subscribed.is_empty() {
                        return;
                    }
                    let topic_urls: Vec<String> = subscribed
                        .iter()
                        .flat_map(|t| topic_to_filter_urls(t))
                        .collect();
                    Filter::new()
                        .kind(Kind::Comment)
                        .custom_tags(
                            SingleLetterTag::uppercase(Alphabet::I),
                            topic_urls,
                        )
                        .custom_tag(
                            SingleLetterTag::uppercase(Alphabet::K),
                            "web".to_string(),
                        )
                        .since(Timestamp::now())
                } else {
                    Filter::new()
                        .kind(Kind::Comment)
                        .custom_tag(
                            SingleLetterTag::uppercase(Alphabet::K),
                            "web".to_string(),
                        )
                        .since(Timestamp::now())
                };

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
                                    let is_reply = event
                                        .tags
                                        .iter()
                                        .any(|t| t.is_reply() || t.is_root());
                                    if is_reply {
                                        continue;
                                    }
                                    if let Some(post) =
                                        crate::stores::topic_store::parse_topic_post(&event)
                                    {
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

    let mut accept_pending_posts = move || {
        let pending: Vec<TopicPost> = pending_posts.write().drain(..).collect();
        if pending.is_empty() {
            return;
        }
        let mut current = posts.read().clone();
        let existing: HashSet<String> = current.iter().map(|p| p.id.clone()).collect();
        for p in pending {
            if !existing.contains(&p.id) {
                current.push(p);
            }
        }
        current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        posts.set(current);
    };

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

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            h1 { class: "text-2xl font-bold text-foreground mb-4", "Topics" }
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
