//! Publication Search Route
//! Search for publications by book reference parameters (NKBIP-08)

use crate::components::icons::{ArrowLeftIcon, BookOpenIcon, SearchIcon};
use crate::routes::Route;
use crate::stores::{nostr_client, publication_store};
use crate::utils::nkbip08::{extract_book_reference_from_tags, BookReference};
use dioxus::prelude::*;

/// Parse query string into a BookReference
fn parse_query_to_reference(query: &str) -> Option<BookReference> {
    // Parse URL query params like "T=genesis&C=bible&c=2&v=kjv"
    let mut title: Option<String> = None;
    let mut collection: Option<String> = None;
    let mut chapter: Option<String> = None;
    let mut sections: Vec<String> = Vec::new();
    let mut version: Option<String> = None;

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            let decoded = urlencoding::decode(value).unwrap_or_default().to_string();
            match key {
                "T" => title = Some(decoded),
                "C" => collection = Some(decoded),
                "c" => chapter = Some(decoded),
                "s" => sections.push(decoded),
                "v" => version = Some(decoded),
                _ => {}
            }
        }
    }

    title.map(|t| {
        let mut reference = BookReference::new(&t);
        if let Some(c) = collection {
            reference = reference.with_collection(&c);
        }
        if let Some(ch) = chapter {
            reference = reference.with_chapter(&ch);
        }
        for s in sections {
            reference = reference.with_section(&s);
        }
        if let Some(v) = version {
            reference = reference.with_version(&v);
        }
        reference
    })
}

/// Publication search route - handles book wikilink URLs
#[component]
pub fn PublicationSearch(query: String) -> Element {
    let nav = use_navigator();
    let mut loading = use_signal(|| true);
    let mut results = use_signal(Vec::new);
    let mut error = use_signal(|| None::<String>);

    // Parse the query into a book reference
    let book_reference = use_memo(move || parse_query_to_reference(&query));

    // Search for matching publications
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        if let Some(ref book_ref) = *book_reference.read() {
            let filter = book_ref.to_filter(None);
            spawn(async move {
                loading.set(true);
                match publication_store::search_publications_with_filter(filter, 50).await {
                    Ok(pubs) => {
                        results.set(pubs);
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e));
                        loading.set(false);
                    }
                }
            });
        } else {
            loading.set(false);
            error.set(Some("Invalid search query".to_string()));
        }
    });

    let go_back = move |_| {
        nav.push(Route::PublicationsHome {});
    };

    rsx! {
        div {
            class: "max-w-4xl mx-auto px-4 py-6",

            // Header
            div {
                class: "flex items-center gap-4 mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back"
                }
                h1 {
                    class: "text-2xl font-bold text-foreground",
                    "Book Reference Search"
                }
            }

            // Search info
            if let Some(ref book_ref) = *book_reference.read() {
                div {
                    class: "p-4 bg-muted/50 rounded-lg mb-6",
                    div {
                        class: "flex items-center gap-2 mb-2",
                        BookOpenIcon { class: "w-5 h-5 text-primary" }
                        span {
                            class: "font-medium",
                            "Searching for: "
                        }
                        span {
                            class: "text-foreground",
                            "{book_ref.display_text()}"
                        }
                    }
                    p {
                        class: "text-sm text-muted-foreground",
                        code {
                            class: "bg-background px-2 py-0.5 rounded",
                            "{book_ref.raw}"
                        }
                    }
                }
            }

            // Error display
            if let Some(ref e) = *error.read() {
                div {
                    class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                    "{e}"
                }
            }

            // Loading state
            if *loading.read() {
                div {
                    class: "flex items-center justify-center py-16",
                    div {
                        class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary"
                    }
                }
            } else if results.read().is_empty() && error.read().is_none() {
                // No results
                div {
                    class: "text-center py-16",
                    SearchIcon { class: "w-16 h-16 text-muted-foreground mx-auto mb-4" }
                    h2 {
                        class: "text-xl font-semibold text-foreground mb-2",
                        "No Publications Found"
                    }
                    p {
                        class: "text-muted-foreground",
                        "No publications match this book reference."
                    }
                }
            } else {
                // Results list
                div {
                    class: "space-y-4",
                    for publication in results.read().iter() {
                        Link {
                            key: "{publication.a_tag}",
                            class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition-colors",
                            to: Route::PublicationDetail { naddr: publication.naddr.clone() },
                            div {
                                class: "flex items-start gap-4",
                                // Cover image or placeholder
                                div {
                                    class: "w-16 h-20 shrink-0 rounded bg-muted flex items-center justify-center",
                                    if let Some(ref img) = publication.cover_image {
                                        img {
                                            class: "w-full h-full object-cover rounded",
                                            src: "{img}",
                                            alt: "",
                                        }
                                    } else {
                                        BookOpenIcon { class: "w-8 h-8 text-muted-foreground" }
                                    }
                                }
                                // Publication info
                                div {
                                    class: "flex-1 min-w-0",
                                    h3 {
                                        class: "font-medium text-foreground",
                                        "{publication.title}"
                                    }
                                    if let Some(ref author) = publication.author {
                                        p {
                                            class: "text-sm text-muted-foreground",
                                            "by {author}"
                                        }
                                    }
                                    if let Some(ref summary) = publication.summary {
                                        p {
                                            class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                            "{summary}"
                                        }
                                    }
                                    p {
                                        class: "text-xs text-muted-foreground mt-2",
                                        "{publication.section_addresses.len()} sections"
                                    }
                                    // Display extracted book reference from tags (if indexed)
                                    {
                                        let tags_vec: Vec<_> = publication.event.tags.iter().cloned().collect();
                                        if let Some(indexed_ref) = extract_book_reference_from_tags(&tags_vec) {
                                            rsx! {
                                                p {
                                                    class: "text-xs text-primary mt-1",
                                                    "📖 {indexed_ref.display_text()}"
                                                }
                                            }
                                        } else {
                                            rsx! {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
