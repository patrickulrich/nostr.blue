use dioxus::prelude::*;
use nostr_sdk::prelude::*;

use crate::services::content_search::{
    search_text_notes, search_articles, search_photos, search_videos, get_contact_pubkeys,
    ContentSearchResult,
};
use crate::components::{NoteCard, NoteCardSkeleton, PhotoCard, VideoCard};

#[derive(Clone, Copy, PartialEq, Debug)]
enum SearchTab {
    TextNotes,
    Articles,
    Photos,
    Videos,
}

impl SearchTab {
    fn label(&self) -> &'static str {
        match self {
            SearchTab::TextNotes => "Posts",
            SearchTab::Articles => "Articles",
            SearchTab::Photos => "Photos",
            SearchTab::Videos => "Videos",
        }
    }
}

#[component]
pub fn Search(q: String) -> Element {
    let mut active_tab = use_signal(|| SearchTab::TextNotes);
    let mut results = use_signal(|| Vec::<ContentSearchResult>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut contact_pubkeys = use_signal(|| Vec::<PublicKey>::new());
    let query = use_signal(|| q.clone());
    let mut search_version = use_signal(|| 0u64);

    // Fetch contacts on mount
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });

    // Search when query or tab changes
    use_effect(move || {
        let q = query.read().clone();
        let tab = *active_tab.read();
        let contacts = contact_pubkeys.read().clone();

        if q.is_empty() {
            // Increment version to invalidate any in-flight searches (without subscribing)
            search_version.with_mut(|v| {
                *v += 1;
            });
            results.set(Vec::new());
            loading.set(false);
            return;
        }

        loading.set(true);
        error.set(None);

        // Increment version to invalidate any in-flight searches (without subscribing)
        let current_version = search_version.with_mut(|v| {
            *v += 1;
            *v
        });

        spawn(async move {
            let search_result = match tab {
                SearchTab::TextNotes => search_text_notes(&q, 50, &contacts).await,
                SearchTab::Articles => search_articles(&q, 50, &contacts).await,
                SearchTab::Photos => search_photos(&q, 50, &contacts).await,
                SearchTab::Videos => search_videos(&q, 50, &contacts).await,
            };

            // Only update state if this is still the most recent search
            if *search_version.read() == current_version {
                match search_result {
                    Ok(search_results) => {
                        results.set(search_results);
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Search failed: {}", e)));
                        loading.set(false);
                    }
                }
            }
        });
    });

    let tabs = [
        SearchTab::TextNotes,
        SearchTab::Articles,
        SearchTab::Photos,
        SearchTab::Videos,
    ];

    rsx! {
        div {
            class: "min-h-screen",

            // Header with query
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    h2 {
                        class: "text-xl font-bold flex items-center gap-2",
                        span { "🔍" }
                        "Search Results"
                    }
                    p {
                        class: "text-sm text-muted-foreground mt-1",
                        "Searching for: \"{query.read()}\""
                    }
                }

                // Tabs
                div {
                    class: "flex border-b border-border overflow-x-auto scrollbar-hide",
                    for tab in tabs.iter() {
                        {
                            let tab_value = *tab;
                            let is_active = *active_tab.read() == tab_value;

                            rsx! {
                                button {
                                    key: "{tab.label()}",
                                    class: if is_active {
                                        "px-6 py-3 text-sm font-medium border-b-2 border-primary text-primary transition"
                                    } else {
                                        "px-6 py-3 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground hover:border-border transition"
                                    },
                                    onclick: move |_| {
                                        active_tab.set(tab_value);
                                    },
                                    "{tab.label()}"
                                }
                            }
                        }
                    }
                }
            }

            // Error state
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            }

            // Loading state
            if *loading.read() && results.read().is_empty() {
                div {
                    class: "divide-y divide-border",
                    for i in 0..5 {
                        NoteCardSkeleton { key: "{i}" }
                    }
                }
            }

            // Empty state
            if !*loading.read() && results.read().is_empty() && query.read().len() > 0 {
                div {
                    class: "flex flex-col items-center justify-center py-16 px-4",
                    div {
                        class: "text-6xl mb-4",
                        "🔍"
                    }
                    p {
                        class: "text-lg font-medium text-muted-foreground mb-2",
                        "No results found"
                    }
                    p {
                        class: "text-sm text-muted-foreground text-center max-w-md",
                        "Try searching with different keywords or switch to another tab"
                    }
                }
            }

            // Results
            if !results.read().is_empty() {
                div {
                    class: "divide-y divide-border",

                    // Summary
                    div {
                        class: "px-4 py-3 bg-muted/30",
                        p {
                            class: "text-sm text-muted-foreground",
                            "Found {results.read().len()} {active_tab.read().label().to_lowercase()}"
                            if results.read().iter().any(|r| r.is_from_contact) {
                                span {
                                    class: "ml-2 text-blue-600 dark:text-blue-400",
                                    "• Results from people you follow are shown first"
                                }
                            }
                        }
                    }

                    // Render results based on tab type
                    for result in results.read().iter() {
                        {
                            let event_clone = result.event.clone();
                            let is_from_contact = result.is_from_contact;
                            let tab = *active_tab.read();

                            rsx! {
                                div {
                                    key: "{result.event.id.to_hex()}",
                                    class: if is_from_contact {
                                        "relative border-l-4 border-l-blue-500"
                                    } else {
                                        ""
                                    },

                                    // Contact badge overlay
                                    if is_from_contact {
                                        div {
                                            class: "absolute top-2 right-2 z-10",
                                            span {
                                                class: "text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded-full",
                                                "Following"
                                            }
                                        }
                                    }

                                    // Render appropriate card based on tab
                                    match tab {
                                        SearchTab::TextNotes | SearchTab::Articles => rsx! {
                                            NoteCard {
                                                event: event_clone,
                                                collapsible: true
                                            }
                                        },
                                        SearchTab::Photos => rsx! {
                                            PhotoCard {
                                                event: event_clone,
                                            }
                                        },
                                        SearchTab::Videos => rsx! {
                                            VideoCard {
                                                event: event_clone,
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }

                    // Load more placeholder (for future pagination)
                    if results.read().len() >= 50 {
                        div {
                            class: "p-8 text-center",
                            p {
                                class: "text-sm text-muted-foreground",
                                "Showing first 50 results. Refine your search for more specific results."
                            }
                        }
                    }
                }
            }

            // Empty query state
            if query.read().is_empty() {
                div {
                    class: "flex flex-col items-center justify-center py-16 px-4",
                    div {
                        class: "text-6xl mb-4",
                        "🔍"
                    }
                    p {
                        class: "text-lg font-medium text-muted-foreground mb-2",
                        "Start searching"
                    }
                    p {
                        class: "text-sm text-muted-foreground text-center max-w-md",
                        "Use the search bar above to find posts, articles, photos, and videos on Nostr"
                    }
                }
            }
        }
    }
}
