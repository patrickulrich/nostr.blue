//! Topics Browse Page
//! Grid of TopicCards for discovery with search and subscribe
use crate::components::{TopicCard, TopicSidebar};
use crate::stores::topic_store::{discover_topics, is_topic_subscribed, TopicInfo};
use dioxus::prelude::*;

#[component]
pub fn TopicsBrowse() -> Element {
    let mut topics = use_signal(Vec::<TopicInfo>::new);
    let mut search_query = use_signal(String::new);
    let mut loading = use_signal(|| true);

    // Fetch discovered topics
    let _resource = use_resource(move || async move {
        loading.set(true);
        if let Ok(discovered) = discover_topics(50).await {
            topics.set(discovered);
        }
        loading.set(false);
    });

    let query = search_query.read().to_lowercase();
    let filtered: Vec<TopicInfo> = topics
        .read()
        .iter()
        .filter(|t| query.is_empty() || t.name.to_lowercase().contains(&query))
        .cloned()
        .collect();

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
                h1 { class: "text-2xl font-bold text-foreground mb-4", "Browse Topics" }
                // Search
                div {
                    class: "mb-4",
                    input {
                        class: "w-full bg-muted border border-border rounded-lg px-4 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50",
                        r#type: "text",
                        placeholder: "Search topics...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
                // Grid
                if *loading.read() {
                    div {
                        class: "flex justify-center py-12",
                        span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                    }
                } else if filtered.is_empty() {
                    div {
                        class: "text-center py-12 text-muted-foreground",
                        if query.is_empty() {
                            "No topics discovered yet."
                        } else {
                            "No topics matching your search."
                        }
                    }
                } else {
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                        for info in &filtered {
                            TopicCard {
                                key: "{info.name}",
                                topic_info: info.clone(),
                                is_subscribed: is_topic_subscribed(&info.name),
                            }
                        }
                    }
                }
            }
        }
    }
}
