//! Topic Sidebar Component
//! Left sidebar showing subscribed topics, trending topics, search, and topic info
use crate::components::topic::TopicInfoCard;
use crate::routes::Route;
use crate::stores::topic_store::{
    fetch_topic_metadata, get_subscribed_topic_names, DISCOVERED_TOPICS, LOADING_SUBSCRIPTIONS,
    TOPIC_METADATA_CACHE,
};
use dioxus::prelude::*;

fn nav_class(active: bool, extra: &str) -> String {
    let base = "px-4 py-2 text-sm rounded-full hover:bg-accent transition";
    if active {
        format!("{base} bg-accent font-medium {extra}")
    } else {
        format!("{base} {extra}")
    }
}

/// Sidebar for topic navigation
#[component]
pub fn TopicSidebar(#[props(default)] current_topic: Option<String>) -> Element {
    let subscribed = get_subscribed_topic_names();
    let loading = *LOADING_SUBSCRIPTIONS.read();
    let current_route = use_route::<Route>();
    let mut metadata = use_signal(|| None::<crate::stores::topic_store::TopicMetadata>);
    let mut meta_loading = use_signal(|| false);
    let topic_for_meta = current_topic.clone();

    use_effect(use_reactive!(|(topic_for_meta,)| {
        let topic = match &topic_for_meta {
            Some(t) => t.clone(),
            None => return,
        };
        if let Some(cached) = TOPIC_METADATA_CACHE.read().peek(&topic).cloned() {
            metadata.set(Some(cached));
            return;
        }
        meta_loading.set(true);
        spawn(async move {
            let result = fetch_topic_metadata(&topic).await;
            metadata.set(result);
            meta_loading.set(false);
        });
    }));

    let home_class = nav_class(matches!(current_route, Route::TopicsHome {}), "");
    let popular_class = nav_class(matches!(current_route, Route::TopicsPopular {}), "");
    let browse_class = nav_class(matches!(current_route, Route::TopicsBrowse {}), "");
    let discover_class = nav_class(matches!(current_route, Route::TopicDiscover {}), "");
    let search_class = nav_class(matches!(current_route, Route::TopicSearch {}), "");
    let new_post_class = nav_class(
        matches!(current_route, Route::TopicNewPost {}),
        "text-primary font-medium",
    );

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
                        class: "{popular_class}",
                        "Popular"
                    }
                    Link {
                        to: Route::TopicsBrowse {},
                        class: "{browse_class}",
                        "Browse"
                    }
                    Link {
                        to: Route::TopicDiscover {},
                        class: "{discover_class}",
                        "Discover"
                    }
                    Link {
                        to: Route::TopicSearch {},
                        class: "{search_class}",
                        "Search"
                    }
                    Link {
                        to: Route::TopicNewPost {},
                        class: "{new_post_class}",
                        "+ New Post"
                    }
                    Link {
                        to: Route::TopicCreate {},
                        class: "px-4 py-2 text-sm rounded-full hover:bg-accent transition text-primary",
                        "+ Create Topic"
                    }
                }
            }
            // Topic info card (when viewing a specific topic)
            if let Some(_topic_name) = &current_topic {
                if let Some(meta) = &*metadata.read() {
                    TopicInfoCard {
                        metadata: meta.clone(),
                    }
                } else if *meta_loading.read() {
                    div {
                        class: "bg-card border border-border rounded-lg p-3",
                        span { class: "text-xs text-muted-foreground", "Loading info..." }
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
                                    let link_class = nav_class(is_active, "");
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
                let mut trending: Vec<_> = discovered.iter()
                    .map(|(name, info)| (name.clone(), info.post_count))
                    .collect();
                trending.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                trending.truncate(10);
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
                                        class: "flex items-center justify-between px-4 py-2 text-sm rounded-full hover:bg-accent transition",
                                        span { "#{name}" }
                                        span { class: "text-xs text-muted-foreground",
                                            if *count == 1 { "{count} post" } else { "{count} posts" }
                                        }
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
