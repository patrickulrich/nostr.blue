//! Wiki Home Route
//! Browse and search NIP-54 wiki pages (Kind 30818)

use dioxus::prelude::*;
use crate::components::{WikiCardSkeleton, WikiGrid, WikiCardSearchResult};
use crate::components::icons::{BookOpenIcon, SearchIcon, PenSquareIcon, RefreshIcon};
use crate::stores::{wiki_store, nostr_client};
use crate::stores::wiki_store::CachedWikiPage;

/// Wiki home page
#[component]
pub fn WikiHome() -> Element {
    let mut loading = use_signal(|| true);
    let mut searching = use_signal(|| false);
    let mut pages = use_signal(Vec::new);
    let mut search_results = use_signal(|| None::<Vec<CachedWikiPage>>);
    let mut search_query = use_signal(String::new);
    let mut committed_query = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    // Fetch wiki pages on initial load
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match wiki_store::fetch_wiki_pages(50, None).await {
                Ok(result) => {
                    pages.set(result);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Perform search when committed_query changes
    use_effect(move || {
        let query = committed_query.read().clone();

        if query.trim().is_empty() {
            search_results.set(None);
            searching.set(false);
            return;
        }

        if query.trim().len() < 2 {
            return;
        }

        spawn(async move {
            searching.set(true);
            match wiki_store::search_wiki_pages(&query, 50).await {
                Ok(results) => {
                    search_results.set(Some(results));
                }
                Err(e) => {
                    log::error!("Search failed: {}", e);
                }
            }
            searching.set(false);
        });
    });

    // Handle search input - trigger search on Enter or blur
    let mut handle_search_input = move |value: String| {
        search_query.set(value);
    };

    let handle_search_keydown = move |e: KeyboardEvent| {
        if e.key() == Key::Enter {
            committed_query.set(search_query.read().clone());
        }
    };

    let handle_search_blur = move |_| {
        let query = search_query.read().clone();
        if query != *committed_query.read() {
            committed_query.set(query);
        }
    };

    // Determine which pages to display
    let display_pages = use_memo(move || {
        if let Some(ref results) = *search_results.read() {
            results.clone()
        } else {
            pages.read().clone()
        }
    });

    let is_searching = search_results.read().is_some();

    let refresh = move |_| {
        search_query.set(String::new());
        committed_query.set(String::new());
        search_results.set(None);
        spawn(async move {
            loading.set(true);
            if let Ok(result) = wiki_store::fetch_wiki_pages(50, None).await {
                pages.set(result);
            }
            loading.set(false);
        });
    };

    rsx! {
        div {
            class: "max-w-6xl mx-auto px-4 py-6",

            // Header
            div {
                class: "flex items-center justify-between mb-6",
                div {
                    class: "flex items-center gap-3",
                    BookOpenIcon { class: "w-8 h-8 text-primary" }
                    h1 {
                        class: "text-2xl font-bold text-foreground",
                        "Wiki"
                    }
                }
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition-colors",
                        onclick: refresh,
                        RefreshIcon { class: "w-5 h-5" }
                    }
                    Link {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                        to: crate::routes::Route::WikiNew {},
                        PenSquareIcon { class: "w-5 h-5" }
                        "New Page"
                    }
                }
            }

            // Description
            p {
                class: "text-muted-foreground mb-6",
                "Browse and contribute to the decentralized wiki. Create and edit pages that anyone can link to using wikilinks."
            }

            // Search
            div {
                class: "relative mb-6",
                SearchIcon { class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground" }
                input {
                    class: "w-full pl-10 pr-4 py-2 bg-background border border-input rounded-lg focus:outline-none focus:ring-2 focus:ring-ring",
                    r#type: "text",
                    placeholder: "Search wiki pages (press Enter to search)...",
                    value: "{search_query}",
                    oninput: move |e| handle_search_input(e.value().clone()),
                    onkeydown: handle_search_keydown,
                    onblur: handle_search_blur,
                }
                // Search loading indicator
                if *searching.read() {
                    div {
                        class: "absolute right-3 top-1/2 -translate-y-1/2",
                        div {
                            class: "w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin",
                        }
                    }
                }
            }

            // Search results info
            if is_searching {
                div {
                    class: "flex items-center justify-between mb-4 text-sm text-muted-foreground",
                    span {
                        "Found {display_pages.read().len()} results for \"{search_query}\""
                    }
                    button {
                        class: "text-primary hover:underline",
                        onclick: move |_| {
                            search_query.set(String::new());
                            search_results.set(None);
                        },
                        "Clear search"
                    }
                }
            }

            // Error state
            if let Some(ref e) = *error.read() {
                div {
                    class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "Error loading wiki pages: {e}"
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && pages.read().is_empty()) {
                // Loading skeleton
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    for _ in 0..6 {
                        WikiCardSkeleton {}
                    }
                }
            } else if display_pages.read().is_empty() {
                // Empty state
                div {
                    class: "flex flex-col items-center justify-center py-16 text-center",
                    BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                    h2 {
                        class: "text-xl font-semibold text-foreground mb-2",
                        if search_query.read().is_empty() {
                            "No Wiki Pages Yet"
                        } else {
                            "No Results Found"
                        }
                    }
                    p {
                        class: "text-muted-foreground mb-6 max-w-md",
                        if search_query.read().is_empty() {
                            "Be the first to create a wiki page! Start contributing to the decentralized knowledge base."
                        } else {
                            "Try a different search term or create a new page with this topic."
                        }
                    }
                    Link {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                        to: crate::routes::Route::WikiNew {},
                        PenSquareIcon { class: "w-5 h-5" }
                        "Create Page"
                    }
                }
            } else if is_searching {
                // Search results view with content previews
                div {
                    class: "bg-card border border-border rounded-lg divide-y divide-border",
                    for page in display_pages.read().iter() {
                        WikiCardSearchResult {
                            key: "{page.event.id.to_hex()}",
                            page: page.clone(),
                            highlight: Some(search_query.read().clone()),
                        }
                    }
                }
            } else {
                // Grid view for browsing
                WikiGrid {
                    pages: display_pages.read().clone(),
                    loading: *loading.read(),
                }
            }
        }
    }
}
