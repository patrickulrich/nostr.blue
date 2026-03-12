//! Topic Post Detail Page
//! Full post + threaded replies
use crate::components::{ThreadView, TopicPostCard, TopicPostComposer};
use crate::stores::auth_store;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::profiles::prefetch_profiles;
use crate::stores::topic_store::{
    build_topic_thread_tree, fetch_post_by_id, fetch_post_replies, fetch_votes_batch, TopicPost,
    TopicThread, VoteCounts,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;

#[component]
pub fn TopicPostDetail(topic: String, post_id: String) -> Element {
    let mut topic_sig = use_signal(|| topic.clone());
    let mut post_id_sig = use_signal(|| post_id.clone());
    use_effect(use_reactive!(|topic| {
        topic_sig.set(topic);
    }));
    use_effect(use_reactive!(|post_id| {
        post_id_sig.set(post_id);
    }));

    let mut post = use_signal(|| None::<TopicPost>);
    let mut replies = use_signal(Vec::<Rc<TopicThread>>::new);
    let mut vote_counts = use_signal(HashMap::<String, VoteCounts>::new);
    let mut loading = use_signal(|| true);
    let has_signer = *HAS_SIGNER.read();

    // Fetch post and replies
    let _resource = use_resource(move || {
        let topic = topic_sig.read().clone();
        let post_id = post_id_sig.read().clone();
        async move {
            loading.set(true);

            // Fetch main post
            if let Ok(Some(fetched_post)) = fetch_post_by_id(&post_id).await {
                // Fetch replies
                let reply_posts = fetch_post_replies(&post_id, &topic, 200)
                    .await
                    .unwrap_or_default();

                // Collect all pubkeys for profile prefetch
                let mut pubkeys: Vec<String> =
                    reply_posts.iter().map(|p| p.pubkey.clone()).collect();
                pubkeys.push(fetched_post.pubkey.clone());
                spawn(prefetch_profiles(pubkeys));

                // Fetch votes for all posts
                let mut all_event_ids: Vec<EventId> = reply_posts
                    .iter()
                    .filter_map(|p| EventId::from_hex(&p.id).ok())
                    .collect();
                if let Ok(id) = EventId::from_hex(&fetched_post.id) {
                    all_event_ids.push(id);
                }
                let user_pk = auth_store::get_pubkey().and_then(|pk| PublicKey::from_hex(&pk).ok());
                if let Ok(votes) = fetch_votes_batch(all_event_ids, user_pk).await {
                    vote_counts.write().extend(votes);
                }

                // Build thread tree
                let tree = build_topic_thread_tree(reply_posts);
                replies.set(tree);
                post.set(Some(fetched_post));
            }

            loading.set(false);
        }
    });

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            if *loading.read() {
                div {
                    class: "flex justify-center py-12",
                    span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                }
            } else if let Some(main_post) = &*post.read() {
                TopicPostCard {
                    post: main_post.clone(),
                    vote_counts: vote_counts.read().get(&main_post.id).cloned(),
                    show_topic_badge: true,
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
