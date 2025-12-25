//! Publications Home Route
//! Browse NKBIP-01 publications (Kind 30040)

use dioxus::prelude::*;
use crate::components::{PublicationCardSkeleton, PublicationGrid, PublicationCardCompact};
use crate::components::icons::{BookOpenIcon, SearchIcon, PenSquareIcon, RefreshIcon, GridIcon, ListIcon};
use crate::stores::{publication_store, nostr_client};
use crate::stores::publication_store::PublicationIndex;

/// View mode for the publications list
#[derive(Clone, Copy, PartialEq, Default)]
enum ViewMode {
    #[default]
    Grid,
    List,
}

/// Publications home page
#[component]
pub fn PublicationsHome() -> Element {
    let mut loading = use_signal(|| true);
    let mut searching = use_signal(|| false);
    let mut publications = use_signal(Vec::new);
    let mut search_results = use_signal(|| None::<Vec<PublicationIndex>>);
    let mut search_query = use_signal(String::new);
    let mut committed_query = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut view_mode = use_signal(|| ViewMode::Grid);

    // Fetch publications on initial load
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match publication_store::fetch_publications(50, None).await {
                Ok(result) => {
                    publications.set(result);
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
            match publication_store::search_publications(&query, 50).await {
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

    // Determine which publications to display
    let display_publications = use_memo(move || {
        if let Some(ref results) = *search_results.read() {
            results.clone()
        } else {
            publications.read().clone()
        }
    });

    let is_searching = search_results.read().is_some();

    let refresh = move |_| {
        search_query.set(String::new());
        committed_query.set(String::new());
        search_results.set(None);
        spawn(async move {
            loading.set(true);
            if let Ok(result) = publication_store::fetch_publications(50, None).await {
                publications.set(result);
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
                        "Publications"
                    }
                }
                div {
                    class: "flex items-center gap-2",
                    // View toggle
                    div {
                        class: "flex items-center border border-border rounded-lg overflow-hidden",
                        button {
                            class: if *view_mode.read() == ViewMode::Grid {
                                "p-2 bg-accent text-foreground"
                            } else {
                                "p-2 hover:bg-accent/50 text-muted-foreground transition-colors"
                            },
                            title: "Grid view",
                            onclick: move |_| view_mode.set(ViewMode::Grid),
                            GridIcon { class: "w-4 h-4" }
                        }
                        button {
                            class: if *view_mode.read() == ViewMode::List {
                                "p-2 bg-accent text-foreground"
                            } else {
                                "p-2 hover:bg-accent/50 text-muted-foreground transition-colors"
                            },
                            title: "List view",
                            onclick: move |_| view_mode.set(ViewMode::List),
                            ListIcon { class: "w-4 h-4" }
                        }
                    }
                    button {
                        class: "p-2 rounded-lg hover:bg-accent transition-colors",
                        onclick: refresh,
                        RefreshIcon { class: "w-5 h-5" }
                    }
                    Link {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                        to: crate::routes::Route::PublicationNew {},
                        PenSquareIcon { class: "w-5 h-5" }
                        "New Publication"
                    }
                }
            }

            // Description
            p {
                class: "text-muted-foreground mb-6",
                "Browse curated publications - books, documentation, and structured long-form content on the Nostr network."
            }

            // Search
            div {
                class: "relative mb-6",
                SearchIcon { class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground" }
                input {
                    class: "w-full pl-10 pr-4 py-2 bg-background border border-input rounded-lg focus:outline-none focus:ring-2 focus:ring-ring",
                    r#type: "text",
                    placeholder: "Search publications (press Enter to search)...",
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
                        "Found {display_publications.read().len()} results for \"{search_query}\""
                    }
                    button {
                        class: "text-primary hover:underline",
                        onclick: move |_| {
                            search_query.set(String::new());
                            committed_query.set(String::new());
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
                    "Error loading publications: {e}"
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && publications.read().is_empty()) {
                // Loading skeleton
                div {
                    class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                    for _ in 0..6 {
                        PublicationCardSkeleton {}
                    }
                }
            } else if display_publications.read().is_empty() {
                // Empty state
                div {
                    class: "flex flex-col items-center justify-center py-16 text-center",
                    BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                    h2 {
                        class: "text-xl font-semibold text-foreground mb-2",
                        if search_query.read().is_empty() {
                            "No Publications Yet"
                        } else {
                            "No Results Found"
                        }
                    }
                    p {
                        class: "text-muted-foreground mb-6 max-w-md",
                        if search_query.read().is_empty() {
                            "Be the first to create a publication! Compile your writing into a structured book or documentation."
                        } else {
                            "Try a different search term or create a new publication with this topic."
                        }
                    }
                    Link {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                        to: crate::routes::Route::PublicationNew {},
                        PenSquareIcon { class: "w-5 h-5" }
                        "Create Publication"
                    }
                }
            } else {
                // Content based on view mode
                if *view_mode.read() == ViewMode::Grid {
                    PublicationGrid {
                        publications: display_publications.read().clone(),
                        loading: *loading.read() || *searching.read(),
                    }
                } else {
                    // List view
                    div {
                        class: "space-y-3",
                        for publication in display_publications.read().iter() {
                            PublicationCardCompact {
                                key: "{publication.event.id.to_hex()}",
                                publication: publication.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}
