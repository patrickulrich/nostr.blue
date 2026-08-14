use crate::components::TopicBadge;
use crate::hooks::use_stale_guard;
use crate::stores::nostr_client::{CLIENT_INITIALIZED, HAS_SIGNER};
use crate::stores::profiles::get_cached_profile;
use crate::stores::topic_store::{
    discover_unsubscribed_topics, subscribe_to_topic, DiscoverTopic,
};
use crate::utils::format::format_relative_time_or;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

#[component]
pub fn TopicDiscover() -> Element {
    let mut topics = use_signal(Vec::<DiscoverTopic>::new);
    let mut loading = use_signal(|| true);
    let mut stale = use_stale_guard();

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let token = stale.bump();
        loading.set(true);

        spawn(async move {
            if stale.is_stale(token) {
                return;
            }
            match discover_unsubscribed_topics(50).await {
                Ok(discovered) => {
                    if !stale.is_stale(token) {
                        topics.set(discovered);
                    }
                }
                Err(e) => log::warn!("Discover fetch failed: {}", e),
            }
            if !stale.is_stale(token) {
                loading.set(false);
            }
        });
    });

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            h1 { class: "text-2xl font-bold text-foreground mb-1", "Discover Topics" }
            p { class: "text-sm text-muted-foreground mb-4", "Topics you haven't joined — sorted by activity in the last 7 days." }

            if *loading.read() {
                div {
                    class: "flex justify-center py-12",
                    span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                }
            } else if topics.read().is_empty() {
                div {
                    class: "text-center py-16 text-muted-foreground",
                    p { "You've already joined all the active topics!" }
                    p { class: "text-sm mt-1", "Check back later for new ones." }
                }
            } else {
                div {
                    class: "flex flex-col gap-3",
                    for topic in topics.read().iter() {
                        DiscoverTopicCard {
                            key: "{topic.info.name}",
                            topic: topic.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DiscoverTopicCard(topic: DiscoverTopic) -> Element {
    let toast = consume_toast();
    let has_signer = *HAS_SIGNER.read();
    let mut joining = use_signal(|| false);
    let name = topic.info.name.clone();
    let post_count = topic.info.post_count;
    let preview = topic.preview_content.clone();
    let latest = topic.info.latest_post_at;
    let topic_for_join = topic.info.name.clone();
    let topic_for_link = topic.info.name.clone();
    let author_pk = topic.preview_author.clone();

    let author_name = author_pk.as_ref().and_then(|pk| {
        get_cached_profile(pk)
            .and_then(|p| p.resolved_name())
    });

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg p-4 hover:border-primary/30 transition",
            div {
                class: "flex items-center justify-between",
                a {
                    href: "/topics/t/{topic_for_link}",
                    onclick: move |e: MouseEvent| {
                        e.prevent_default();
                        navigator().push(crate::routes::Route::TopicFeed { topic: topic_for_link.clone() });
                    },
                    class: "flex-1 min-w-0",
                    div {
                        class: "flex items-center gap-2 mb-1",
                        TopicBadge { topic: name.clone() }
                        span { class: "text-xs text-muted-foreground",
                            "{post_count} {post_count_label(post_count)} this week"
                        }
                    }
                    if let Some(text) = &preview {
                        p {
                            class: "text-sm text-muted-foreground truncate mt-1",
                            if let Some(author) = &author_name {
                                span { class: "text-foreground/50 mr-1", "{author}:" }
                            }
                            "{text}"
                        }
                    }
                    if let Some(ts) = latest {
                        p {
                            class: "text-xs text-muted-foreground/70 mt-1",
                            "Last active {format_relative_time_or(ts, \"just now\")}"
                        }
                    }
                }
                if has_signer {
                    button {
                        class: "px-3 py-1.5 text-xs font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50 shrink-0 ml-3",
                        disabled: *joining.read(),
                        onclick: move |_| {
                            let topic = topic_for_join.clone();
                            joining.set(true);
                            spawn(async move {
                                match subscribe_to_topic(&topic).await {
                                    Ok(()) => {
                                        toast.info(format!("Joined #{topic}"), ToastOptions::new());
                                    }
                                    Err(e) => {
                                        toast.error(format!("Failed to join: {e}"), ToastOptions::new());
                                    }
                                }
                                joining.set(false);
                            });
                        },
                        if *joining.read() { "Joining..." } else { "Join" }
                    }
                }
            }
        }
    }
}

fn post_count_label(count: usize) -> &'static str {
    if count == 1 { "post" } else { "posts" }
}
