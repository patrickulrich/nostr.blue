//! Topic Card Component
//! Discovery card showing topic name, post count, and subscribe button
use crate::routes::Route;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::topic_store::{
    subscribe_to_topic, unsubscribe_from_topic, TopicInfo,
};
use dioxus::prelude::*;

/// Discovery card for browsing topics
#[component]
pub fn TopicCard(
    topic_info: TopicInfo,
    #[props(default = false)]
    is_subscribed: bool,
) -> Element {
    let has_signer = *HAS_SIGNER.read();
    let mut subscribing = use_signal(|| false);
    let mut subscribed = use_signal(|| is_subscribed);

    // Keep local signal in sync when the parent prop changes
    use_effect(use_reactive(&is_subscribed, move |v| {
        subscribed.set(v);
    }));

    let topic_name = topic_info.name.clone();
    let topic_for_action = topic_info.name.clone();

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg p-4 hover:bg-accent/50 transition",
            div {
                class: "flex items-start justify-between gap-3",
                // Topic info
                Link {
                    to: Route::TopicFeed { topic: topic_name.clone() },
                    class: "flex-1 min-w-0",
                    h3 {
                        class: "text-lg font-bold text-foreground hover:text-primary transition",
                        "#{topic_name}"
                    }
                    div {
                        class: "flex items-center gap-3 mt-1 text-sm text-muted-foreground",
                        span { "{topic_info.post_count} posts" }
                    }
                }
                // Subscribe button
                if has_signer {
                    button {
                        class: if *subscribed.read() {
                            "px-3 py-1.5 text-xs font-medium rounded-md border border-border text-muted-foreground hover:bg-destructive/10 hover:text-destructive hover:border-destructive/50 transition"
                        } else {
                            "px-3 py-1.5 text-xs font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition"
                        },
                        disabled: *subscribing.read(),
                        onclick: move |_| {
                            let topic = topic_for_action.clone();
                            let currently_subscribed = *subscribed.read();
                            subscribing.set(true);
                            spawn(async move {
                                let result = if currently_subscribed {
                                    unsubscribe_from_topic(&topic).await
                                } else {
                                    subscribe_to_topic(&topic).await
                                };
                                match result {
                                    Ok(()) => subscribed.set(!currently_subscribed),
                                    Err(e) => log::error!("Subscription change failed: {}", e),
                                }
                                subscribing.set(false);
                            });
                        },
                        if *subscribing.read() {
                            "..."
                        } else if *subscribed.read() {
                            "Subscribed"
                        } else {
                            "Subscribe"
                        }
                    }
                }
            }
        }
    }
}
