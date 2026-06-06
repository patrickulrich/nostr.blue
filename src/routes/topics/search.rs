use crate::components::TopicPostCard;
use crate::stores::topic_store::{search_topic_posts, SearchMode};
use dioxus::prelude::*;

#[component]
pub fn TopicSearch() -> Element {
    let mut query = use_signal(String::new);
    let mut topic_filter = use_signal(String::new);
    let mut results = use_signal(Vec::<crate::stores::topic_store::TopicPost>::new);
    let mut search_mode = use_signal(|| None::<SearchMode>);
    let mut loading = use_signal(|| false);
    let mut searched = use_signal(|| false);

    rsx! {
        div {
            class: "w-full max-w-6xl mx-auto px-4 py-4",
            Link {
                to: crate::routes::Route::TopicsHome {},
                class: "text-sm text-muted-foreground hover:text-foreground mb-4 inline-block",
                "← Back to Topics"
            }
            h1 { class: "text-2xl font-bold text-foreground mb-4", "Search Topics" }
            div {
                class: "bg-card border border-border rounded-lg p-4 space-y-3",
                div {
                    class: "flex gap-2",
                    input {
                        class: "flex-1 bg-muted border border-border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50",
                        r#type: "text",
                        placeholder: "Search posts...",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter && !query.read().trim().is_empty() {
                                let q = query.read().trim().to_string();
                                let t = topic_filter.read().trim().to_string();
                                let topic_opt = if t.is_empty() { None } else { Some(t) };
                                loading.set(true);
                                searched.set(true);
                                spawn(async move {
                                    match search_topic_posts(&q, topic_opt.as_deref(), 50).await {
                                        Ok((posts, mode)) => {
                                            results.set(posts);
                                            search_mode.set(Some(mode));
                                        }
                                        Err(_) => {
                                            results.set(Vec::new());
                                            search_mode.set(None);
                                        }
                                    }
                                    loading.set(false);
                                });
                            }
                        },
                    }
                    button {
                        class: "px-4 py-2 text-sm font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                        disabled: query.read().trim().is_empty() || *loading.read(),
                        onclick: move |_| {
                            let q = query.read().trim().to_string();
                            let t = topic_filter.read().trim().to_string();
                            let topic_opt = if t.is_empty() { None } else { Some(t) };
                            loading.set(true);
                            searched.set(true);
                            spawn(async move {
                                match search_topic_posts(&q, topic_opt.as_deref(), 50).await {
                                    Ok((posts, mode)) => {
                                        results.set(posts);
                                        search_mode.set(Some(mode));
                                    }
                                    Err(_) => {
                                        results.set(Vec::new());
                                        search_mode.set(None);
                                    }
                                }
                                loading.set(false);
                            });
                        },
                        if *loading.read() { "Searching..." } else { "Search" }
                    }
                }
                input {
                    class: "w-full bg-muted border border-border rounded-md px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50",
                    r#type: "text",
                    placeholder: "Filter by topic (optional)",
                    value: "{topic_filter}",
                    oninput: move |e| topic_filter.set(e.value()),
                }
            }
            if let Some(mode) = *search_mode.read() {
                div {
                    class: "mt-3 flex items-center gap-2 text-xs text-muted-foreground",
                    span {
                        class: "px-2 py-0.5 rounded-full bg-accent",
                        match mode {
                            SearchMode::Relay => "Relay search",
                            SearchMode::Local => "Local filter",
                        }
                    }
                    span { "{results.read().len()} results" }
                }
            }
            if *loading.read() {
                div {
                    class: "flex justify-center py-12",
                    span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                }
            } else if *searched.read() && results.read().is_empty() {
                div {
                    class: "text-center py-12 text-muted-foreground",
                    "No results found."
                }
            } else {
                div {
                    class: "flex flex-col gap-2 mt-4",
                    for post in results.read().iter() {
                        TopicPostCard {
                            key: "{post.id}",
                            post: post.clone(),
                            show_topic_badge: true,
                        }
                    }
                }
            }
        }
    }
}
