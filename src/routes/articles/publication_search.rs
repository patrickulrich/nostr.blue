use crate::components::icons::{ArrowLeftIcon, BookOpenIcon, SearchIcon};
use crate::hooks::{use_nostr_resource_public, NostrResourceState};
use crate::routes::Route;
use crate::stores::publication_store;
use crate::utils::nkbip08::{extract_book_reference_from_tags, BookReference};
use dioxus::prelude::*;
/// Parse query string into a BookReference
fn parse_query_to_reference(query: &str) -> Option<BookReference> {
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
    let book_reference = use_memo(move || parse_query_to_reference(&query));
    let results = use_nostr_resource_public(move || {
        let book_ref = book_reference.read().clone();
        async move {
            match book_ref {
                Some(bref) => {
                    let filter = bref.to_filter(None);
                    publication_store::search_publications_with_filter(filter, 50).await
                }
                None => Err("Invalid search query".to_string()),
            }
        }
    });
    let results_state = results.state();
    let go_back = move |_| {
        nav.push(Route::PublicationsHome {});
    };
    rsx! {
        div { class: "max-w-4xl mx-auto px-4 py-6",
            div { class: "flex items-center gap-4 mb-6",
                button {
                    class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors",
                    onclick: go_back,
                    ArrowLeftIcon { class: "w-5 h-5" }
                    "Back"
                }
                h1 { class: "text-2xl font-bold text-foreground", "Book Reference Search" }
            }
            if let Some(ref book_ref) = *book_reference.read() {
                div { class: "p-4 bg-muted/50 rounded-lg mb-6",
                    div { class: "flex items-center gap-2 mb-2",
                        BookOpenIcon { class: "w-5 h-5 text-primary" }
                        span { class: "font-medium", "Searching for: " }
                        span { class: "text-foreground", "{book_ref.display_text()}" }
                    }
                    p { class: "text-sm text-muted-foreground",
                        code { class: "bg-background px-2 py-0.5 rounded", "{book_ref.raw}" }
                    }
                }
            }
            match &*results_state.read() {
                NostrResourceState::Error(e) => rsx! {
                    div { class: "p-4 rounded-lg bg-destructive/10 text-destructive mb-6",
                        "{e}"
                    }
                },
                NostrResourceState::Loading | NostrResourceState::Initializing => rsx! {
                    div { class: "flex items-center justify-center py-16",
                        div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary" }
                    }
                },
                NostrResourceState::Loaded(data) if !data.is_empty() => rsx! {
                    div { class: "space-y-4",
                        for publication in data.iter() {
                            Link {
                                key: "{publication.a_tag}",
                                class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition-colors",
                                to: Route::PublicationDetail {
                                    naddr: publication.naddr.clone(),
                                },
                                div { class: "flex items-start gap-4",
                                    div { class: "w-16 h-20 shrink-0 rounded bg-muted flex items-center justify-center",
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
                                    div { class: "flex-1 min-w-0",
                                        h3 { class: "font-medium text-foreground", "{publication.title}" }
                                        if let Some(ref author) = publication.author {
                                            p { class: "text-sm text-muted-foreground",
                                                "by {author}"
                                            }
                                        }
                                        if let Some(ref summary) = publication.summary {
                                            p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                                "{summary}"
                                            }
                                        }
                                        p { class: "text-xs text-muted-foreground mt-2",
                                            "{publication.section_addresses.len()} sections"
                                        }
                                        {
                                            let tags_vec: Vec<_> = publication.event.tags.iter().cloned().collect();
                                            if let Some(indexed_ref) = extract_book_reference_from_tags(&tags_vec) {
                                                rsx! {
                                                    p { class: "text-xs text-primary mt-1", "📖 {indexed_ref.display_text()}" }
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
                },
                NostrResourceState::Loaded(_) => rsx! {
                    div { class: "text-center py-16",
                        SearchIcon { class: "w-16 h-16 text-muted-foreground mx-auto mb-4" }
                        h2 { class: "text-xl font-semibold text-foreground mb-2", "No Publications Found" }
                        p { class: "text-muted-foreground", "No publications match this book reference." }
                    }
                },
                NostrResourceState::AuthRequired => rsx! {},
            }
        }
    }
}
