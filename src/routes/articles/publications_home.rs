//! Publications Home Route
//! Browse NKBIP-01 publications (Kind 30040)
use crate::components::icons::{
    BookOpenIcon, GridIcon, ListIcon, PenSquareIcon, RefreshIcon, SearchIcon,
};
use crate::components::{PublicationCardCompact, PublicationCardSkeleton, PublicationGrid};
use crate::hooks::use_infinite_scroll_with_generation;
use crate::stores::publication_store::PublicationIndex;
use crate::stores::{nostr_client, publication_store};
use crate::utils::pagination::{is_likely_future, safe_cursor_from_timestamps};
use dioxus::prelude::*;
use std::collections::HashSet;
const PAGE_SIZE: usize = 24;
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
    let mut pagination_loading = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut feed_reset_generation = use_signal(|| 0u64);
    // The sentinel renders in view-mode-specific branches gated on
    // `!searching`; toggling the view or search mode unmounts it while
    // `has_more` stays true. Bump the generation to re-attach the observer.
    use_effect(move || {
        let _ = *view_mode.read();
        let _ = *searching.read();
        feed_reset_generation += 1;
    });
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match publication_store::fetch_publications(PAGE_SIZE, None).await {
                Ok(result) => {
                    {
                        let ts: Vec<u64> = result.iter().map(|p| p.event.created_at.as_secs()).collect();
                        oldest_timestamp.set(safe_cursor_from_timestamps(&ts));
                    }
                    has_more.set(result.len() >= PAGE_SIZE / 2);
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
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }
        let until = *oldest_timestamp.peek();
        spawn(async move {
            pagination_loading.set(true);
            match publication_store::fetch_publications(PAGE_SIZE, until).await {
                Ok(fetched) => {
                    let fetched_count = fetched.len();
                    if fetched.is_empty() {
                        has_more.set(false);
                    } else {
                        {
                            let ts: Vec<u64> = fetched.iter().map(|p| p.event.created_at.as_secs()).collect();
                            oldest_timestamp.set(safe_cursor_from_timestamps(&ts));
                        }
                        let mut current = publications.peek().clone();
                        let existing_ids: HashSet<_> =
                            current.iter().map(|p| p.event.id.to_hex()).collect();
                        let mut added_count = 0;
                        for pub_item in fetched {
                            if is_likely_future(pub_item.event.created_at) { continue; }
                            if !existing_ids.contains(&pub_item.event.id.to_hex()) {
                                current.push(pub_item);
                                added_count += 1;
                            }
                        }
                        publications.set(current);
                        if fetched_count < PAGE_SIZE / 2 {
                            has_more.set(false);
                        }
                        log::info!(
                            "Pagination: fetched {}, added {} unique publications",
                            fetched_count,
                            added_count
                        );
                    }
                }
                Err(e) => log::error!("Failed to load more publications: {}", e),
            }
            pagination_loading.set(false);
        });
    };
    let sentinel_id = use_infinite_scroll_with_generation(
        load_more,
        has_more,
        pagination_loading,
        feed_reset_generation,
    );
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
        has_more.set(true);
        oldest_timestamp.set(None);
        spawn(async move {
            loading.set(true);
            if let Ok(result) = publication_store::fetch_publications(PAGE_SIZE, None).await {
                {
                    let ts: Vec<u64> = result.iter().map(|p| p.event.created_at.as_secs()).collect();
                    oldest_timestamp.set(safe_cursor_from_timestamps(&ts));
                }
                has_more.set(result.len() >= PAGE_SIZE / 2);
                publications.set(result);
            }
            loading.set(false);
        });
    };
    rsx! {
        div { class: "max-w-6xl mx-auto px-4 py-6",
            div { class: "flex items-center justify-between mb-6",
                div { class: "flex items-center gap-3",
                    BookOpenIcon { class: "w-8 h-8 text-primary" }
                    h1 { class: "text-2xl font-bold text-foreground", "Publications" }
                }
                div { class: "flex items-center gap-2",
                    div { class: "flex items-center border border-border rounded-lg overflow-hidden",
                        button {
                            class: if *view_mode.read() == ViewMode::Grid { "p-2 bg-accent text-foreground" } else { "p-2 hover:bg-accent/50 text-muted-foreground transition-colors" },
                            title: "Grid view",
                            onclick: move |_| view_mode.set(ViewMode::Grid),
                            GridIcon { class: "w-4 h-4" }
                        }
                        button {
                            class: if *view_mode.read() == ViewMode::List { "p-2 bg-accent text-foreground" } else { "p-2 hover:bg-accent/50 text-muted-foreground transition-colors" },
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
                        to: crate::routes::Route::PublicationNew {
                        },
                        PenSquareIcon { class: "w-5 h-5" }
                        "New Publication"
                    }
                }
            }
            p { class: "text-muted-foreground mb-6",
                "Browse curated publications - books, documentation, and structured long-form content on the Nostr network."
            }
            div { class: "relative mb-6",
                SearchIcon { class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground" }
                input {
                    class: "w-full pl-10 pr-4 py-2 bg-background border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                    r#type: "text",
                    placeholder: "Search publications (press Enter to search)...",
                    value: "{search_query}",
                    oninput: move |e| handle_search_input(e.value().clone()),
                    onkeydown: handle_search_keydown,
                    onblur: handle_search_blur,
                }
                if *searching.read() {
                    div { class: "absolute right-3 top-1/2 -translate-y-1/2",
                        div { class: "w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                    }
                }
            }
            if is_searching {
                div { class: "flex items-center justify-between mb-4 text-sm text-muted-foreground",
                    span { "Found {display_publications.read().len()} results for \"{search_query}\"" }
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
            if let Some(ref e) = *error.read() {
                div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "Error loading publications: {e}"
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read()
                || (*loading.read() && publications.read().is_empty())
            {
                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                    for _ in 0..6 {
                        PublicationCardSkeleton {}
                    }
                }
            } else if display_publications.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-16 text-center",
                    BookOpenIcon { class: "w-16 h-16 text-muted-foreground mb-4" }
                    h2 { class: "text-xl font-semibold text-foreground mb-2",
                        if search_query.read().is_empty() {
                            "No Publications Yet"
                        } else {
                            "No Results Found"
                        }
                    }
                    p { class: "text-muted-foreground mb-6 max-w-md",
                        if search_query.read().is_empty() {
                            "Be the first to create a publication! Compile your writing into a structured book or documentation."
                        } else {
                            "Try a different search term or create a new publication with this topic."
                        }
                    }
                    Link {
                        class: "flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors",
                        to: crate::routes::Route::PublicationNew {
                        },
                        PenSquareIcon { class: "w-5 h-5" }
                        "Create Publication"
                    }
                }
            } else {
                if *view_mode.read() == ViewMode::Grid {
                    div {
                        PublicationGrid {
                            publications: display_publications.read().clone(),
                            loading: *loading.read() || *searching.read(),
                        }
                        if *pagination_loading.read() && !is_searching {
                            div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mt-4",
                                for i in 0..6 {
                                    PublicationCardSkeleton { key: "loading-grid-{i}" }
                                }
                            }
                        }
                        if *has_more.read() && !is_searching {
                            div { id: "{sentinel_id}", class: "h-4" }
                        }
                        if !*has_more.read() && !publications.read().is_empty() && !is_searching {
                            div { class: "text-center py-8 text-muted-foreground",
                                "You've reached the end!"
                            }
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        for publication in display_publications.read().iter() {
                            PublicationCardCompact {
                                key: "{publication.event.id.to_hex()}",
                                publication: publication.clone(),
                            }
                        }
                        if *pagination_loading.read() && !is_searching {
                            for i in 0..3 {
                                PublicationCardSkeleton { key: "loading-list-skeleton-{i}" }
                            }
                        }
                        if *has_more.read() && !is_searching {
                            div { id: "{sentinel_id}", class: "h-4" }
                        }
                        if !*has_more.read() && !publications.read().is_empty() && !is_searching {
                            div { class: "text-center py-8 text-muted-foreground",
                                "You've reached the end!"
                            }
                        }
                    }
                }
            }
        }
    }
}
