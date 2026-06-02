use crate::components::TopicCard;
use crate::hooks::use_stale_guard;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::topic_store::{
    discover_topics, is_topic_subscribed, query_discover_topics_from_db, TopicInfo,
};
use dioxus::prelude::*;

#[component]
pub fn TopicsBrowse() -> Element {
    let mut topics = use_signal(Vec::<TopicInfo>::new);
    let mut search_query = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut loading_new = use_signal(|| false);
    let mut stale = use_stale_guard();

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let token = stale.bump();
        loading.set(true);
        loading_new.set(false);

        spawn(async move {
            if stale.is_stale(token) {
                return;
            }

            let db_topics = query_discover_topics_from_db(50).await;
            if stale.is_stale(token) {
                return;
            }
            if !db_topics.is_empty() {
                topics.set(db_topics);
                loading.set(false);
                loading_new.set(true);
            }

            if stale.is_stale(token) {
                return;
            }
            if let Ok(discovered) = discover_topics(50).await {
                if stale.is_stale(token) {
                    return;
                }
                topics.set(discovered);
            }
            loading.set(false);
            loading_new.set(false);
        });
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
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            h1 { class: "text-2xl font-bold text-foreground mb-4", "Browse Topics" }
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
            if *loading.read() && topics.read().is_empty() {
                div {
                    class: "flex justify-center py-12",
                    span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                }
            } else if *loading_new.read() && !topics.read().is_empty() {
                div {
                    class: "flex flex-col gap-3",
                    div {
                        class: "sticky top-[57px] z-20 border-b border-border bg-muted/80 backdrop-blur-sm",
                        div { class: "px-4 py-2 text-center",
                            span { class: "inline-flex items-center gap-2 text-sm text-muted-foreground",
                                span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading new topics..."
                            }
                        }
                    }
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
