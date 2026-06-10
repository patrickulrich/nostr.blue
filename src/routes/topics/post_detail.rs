//! Topic Post Detail Page
//! Full post + threaded replies
use crate::components::{ThreadView, TopicPostCard, TopicPostComposer};
use crate::hooks::use_mute_block_cache;
use crate::stores::auth_store;
use crate::components::ClientInitializing;
use crate::stores::nostr_client::{CLIENT_INITIALIZED, HAS_SIGNER};
use crate::stores::profiles::prefetch_profiles;
use crate::stores::topic_store::{
    build_topic_thread_tree, fetch_post_by_id, fetch_post_replies, fetch_topic_metadata,
    fetch_topic_pins, fetch_votes_batch, TopicPost, TopicThread, VoteCounts,
};
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;

fn decode_post_id(post_id: &str) -> String {
    if post_id.starts_with("nevent1") || post_id.starts_with("note1") {
        if let Ok(nip19) = Nip19::from_bech32(post_id) {
            return match nip19 {
                Nip19::Event(nevent) => nevent.event_id.to_hex(),
                Nip19::EventId(event_id) => event_id.to_hex(),
                _ => post_id.to_string(),
            };
        }
    }
    post_id.to_string()
}

#[component]
pub fn TopicPostDetail(topic: String, post_id: String) -> Element {
    let mut post = use_signal(|| None::<TopicPost>);
    let mut replies = use_signal(Vec::<Rc<TopicThread>>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let mut loading = use_signal(|| true);
    let has_signer = *HAS_SIGNER.read();
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();
    let mut creator_pubkey = use_signal(|| None::<String>);
    let mut pinned_ids = use_signal(Vec::<String>::new);

    use_effect(use_reactive!(|(topic, post_id)| {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            loading.set(false);
            return;
        }

        let hex_id = decode_post_id(&post_id);
        loading.set(true);
        post.set(None);
        replies.set(Vec::new());

        spawn(async move {
            if let Ok(Some(fetched_post)) = fetch_post_by_id(&hex_id).await {
                let reply_posts = fetch_post_replies(&hex_id, &topic, 200)
                    .await
                    .unwrap_or_default();

                let mut pubkeys: Vec<String> =
                    reply_posts.iter().map(|p| p.pubkey.clone()).collect();
                pubkeys.push(fetched_post.pubkey.clone());
                spawn(prefetch_profiles(pubkeys));

                let mut all_event_ids: Vec<EventId> = reply_posts
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                if let Ok(id) = EventId::from_hex(&fetched_post.id) {
                    all_event_ids.push(id);
                }
                let user_pk =
                    auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                if let Ok(votes) = fetch_votes_batch(all_event_ids, user_pk).await {
                    vote_counts.write().extend(votes);
                }

                let tree = build_topic_thread_tree(reply_posts);
                replies.set(tree);
                post.set(Some(fetched_post));
            }

            // Fetch topic metadata + pins for the overflow menu
            if let Some(meta) = fetch_topic_metadata(&topic).await {
                creator_pubkey.set(Some(meta.creator_pubkey.clone()));
                let pins = fetch_topic_pins(&topic, &meta.creator_pubkey).await;
                pinned_ids.set(pins);
            }

            loading.set(false);
        });
    }));

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            if !*CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if *loading.read() {
                div {
                    class: "flex justify-center py-12",
                    span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                }
            } else if let Some(main_post) = &*post.read() {
                TopicPostCard {
                    post: main_post.clone(),
                    vote_counts: vote_counts.read().get(&main_post.id).cloned(),
                    show_topic_badge: true,
                    cached_muted_posts: cached_muted_posts.read().clone(),
                    cached_blocked_users: cached_blocked_users.read().clone(),
                    cached_muted_words: cached_muted_words.read().clone(),
                    is_pinned: pinned_ids.read().contains(&main_post.id),
                    creator_pubkey: creator_pubkey.read().clone(),
                    current_pins: pinned_ids.read().clone(),
                }
                if has_signer {
                    div { class: "mt-4" }
                    TopicPostComposer {
                        reply_to: Some(main_post.clone()),
                        on_success: move |_: String| {
                            // TODO: refresh replies
                        },
                    }
                }
                if !replies.read().is_empty() {
                    div { class: "mt-4" }
                    h3 {
                        class: "text-lg font-semibold text-foreground mb-2",
                        "Replies"
                    }
                    ThreadView {
                        thread: replies.read().clone(),
                        vote_counts: Rc::new(vote_counts.read().clone()),
                        cached_muted_posts: cached_muted_posts.read().clone(),
                        cached_blocked_users: cached_blocked_users.read().clone(),
                        cached_muted_words: cached_muted_words.read().clone(),
                    }
                }
            } else {
                div {
                    class: "text-center py-12 text-muted-foreground",
                    "Post not found."
                }
            }
        }
    }
}
