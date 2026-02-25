//! Topic Sidebar Component
//! Left sidebar showing subscribed topics, trending topics, and search
use crate::routes::Route;
use crate::stores::topic_store::{
    get_subscribed_topic_names, DISCOVERED_TOPICS, LOADING_SUBSCRIPTIONS,
};
use dioxus::prelude::*;

/// Sidebar for topic navigation
#[component]
pub fn TopicSidebar(
    #[props(default)]
    current_topic: Option<String>,
) -> Element {
    let subscribed = get_subscribed_topic_names();
    let loading = *LOADING_SUBSCRIPTIONS.read();

    let home_class = if current_topic.is_none() {
        "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition bg-accent font-medium"
    } else {
        "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition"
    };

    rsx! {
        div {
            class: "flex flex-col gap-4",
            // Navigation links
            div {
                class: "bg-card border border-border rounded-lg p-3",
                h3 { class: "text-sm font-semibold text-foreground mb-2", "Topics" }
                nav {
                    class: "flex flex-col gap-1",
                    Link {
                        to: Route::TopicsHome {},
                        class: "{home_class}",
                        "Home"
                    }
                    Link {
                        to: Route::TopicsPopular {},
                        class: "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition",
                        "Popular"
                    }
                    Link {
                        to: Route::TopicsBrowse {},
                        class: "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition",
                        "Browse"
                    }
                    Link {
                        to: Route::TopicNewPost {},
                        class: "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition text-primary font-medium",
                        "+ New Post"
                    }
                }
            }
            // Subscribed topics
            if !subscribed.is_empty() || loading {
                div {
                    class: "bg-card border border-border rounded-lg p-3",
                    h3 { class: "text-sm font-semibold text-foreground mb-2", "Your Topics" }
                    if loading {
                        div {
                            class: "text-sm text-muted-foreground",
                            "Loading..."
                        }
                    } else {
                        div {
                            class: "flex flex-col gap-1",
                            for topic in &subscribed {
                                {
                                    let is_active = current_topic.as_ref() == Some(topic);
                                    let link_class = if is_active {
                                        "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition bg-accent font-medium"
                                    } else {
                                        "px-3 py-1.5 text-sm rounded-md hover:bg-accent transition"
                                    };
                                    rsx! {
                                        Link {
                                            key: "{topic}",
                                            to: Route::TopicFeed { topic: topic.clone() },
                                            class: "{link_class}",
                                            "#{topic}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Trending topics
            {
                let discovered = DISCOVERED_TOPICS.read();
                let trending: Vec<_> = discovered.iter().take(10).map(|(name, info)| (name.clone(), info.post_count)).collect();
                if !trending.is_empty() {
                    rsx! {
                        div {
                            class: "bg-card border border-border rounded-lg p-3",
                            h3 { class: "text-sm font-semibold text-foreground mb-2", "Trending" }
                            div {
                                class: "flex flex-col gap-1",
                                for (name, count) in &trending {
                                    Link {
                                        key: "{name}",
                                        to: Route::TopicFeed { topic: name.clone() },
                                        class: "flex items-center justify-between px-3 py-1.5 text-sm rounded-md hover:bg-accent transition",
                                        span { "#{name}" }
                                        span { class: "text-xs text-muted-foreground", "{count} posts" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    rsx! {}
                }
            }
        }
    }
}
