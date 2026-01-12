//! Bible Search Page
//! Client-side search through cached chapters

use dioxus::prelude::*;

use crate::stores::bible_store::{
    BibleSearchResult, search_cached_verses, get_all_cached_chapters,
};

/// Bible Search Page
#[component]
pub fn BibleSearch() -> Element {
    // State
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<BibleSearchResult>::new);
    let mut has_searched = use_signal(|| false);

    // Count cached chapters
    let cached_count = get_all_cached_chapters().len();

    // Perform search
    let mut perform_search = move || {
        let q = query.read().clone();
        if q.len() >= 3 {
            let found = search_cached_verses(&q, 50);
            results.set(found);
            has_searched.set(true);
        } else {
            results.set(Vec::new());
            has_searched.set(false);
        }
    };

    rsx! {
        div { class: "max-w-3xl mx-auto p-4 space-y-6",

            // Header
            div { class: "flex items-center gap-4",
                Link {
                    to: crate::routes::Route::BibleHome {},
                    class: "p-2 hover:bg-muted rounded-lg transition",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-5 h-5",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M15 19l-7-7 7-7"
                        }
                    }
                }
                h1 { class: "text-2xl font-bold", "Search Bible" }
            }

            // Search input
            div { class: "relative",
                input {
                    r#type: "text",
                    placeholder: "Search verses... (min 3 characters)",
                    class: "w-full px-4 py-3 pr-12 border border-border rounded-full focus:outline-none focus:ring-2 focus:ring-primary bg-background",
                    value: "{query}",
                    oninput: move |evt| {
                        query.set(evt.value());
                    },
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter {
                            perform_search();
                        }
                    }
                }
                button {
                    class: "absolute right-3 top-1/2 -translate-y-1/2 p-2 hover:bg-muted rounded-full transition text-muted-foreground",
                    onclick: move |_| perform_search(),
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-5 h-5",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                        }
                    }
                }
            }

            // Cache info
            div { class: "text-sm text-muted-foreground",
                if cached_count == 0 {
                    p {
                        "No chapters cached yet. Browse some chapters first to enable search."
                    }
                } else {
                    p {
                        "Searching {cached_count} cached chapters. "
                        span { class: "text-xs",
                            "Browse more chapters to expand searchable content."
                        }
                    }
                }
            }

            // Results
            if *has_searched.read() {
                if results.read().is_empty() {
                    div { class: "text-center py-12",
                        div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-8 h-8 text-muted-foreground",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                                }
                            }
                        }
                        p { class: "text-muted-foreground font-medium",
                            "No results found"
                        }
                        p { class: "text-sm text-muted-foreground mt-1",
                            "Try a different search term or browse more chapters."
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        p { class: "text-sm text-muted-foreground",
                            "{results.read().len()} results"
                        }

                        for result in results.read().iter() {
                            SearchResultCard {
                                key: "{result.translation}-{result.book}-{result.chapter}-{result.verse}",
                                result: result.clone(),
                                query: query.read().clone()
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Search result card
#[component]
fn SearchResultCard(result: BibleSearchResult, query: String) -> Element {
    let reference = format!(
        "{} {}:{} ({})",
        result.book_name, result.chapter, result.verse, result.translation
    );

    // Highlight matching text
    let text_lower = result.text.to_lowercase();
    let query_lower = query.to_lowercase();

    rsx! {
        Link {
            to: crate::routes::Route::BibleChapter {
                translation: result.translation.clone(),
                book: result.book.clone(),
                chapter: result.chapter,
            },
            class: "block p-4 bg-card border border-border rounded-lg hover:border-primary/50 transition",

            // Reference
            p { class: "font-medium text-sm text-primary mb-1",
                "{reference}"
            }

            // Verse text with highlighted match
            p { class: "text-sm text-muted-foreground line-clamp-3",
                {
                    if let Some(idx) = text_lower.find(&query_lower) {
                        // Show context around match
                        let start = idx.saturating_sub(50);
                        let end = (idx + query.len() + 50).min(result.text.len());

                        let before = if start > 0 { "..." } else { "" };
                        let after = if end < result.text.len() { "..." } else { "" };

                        let text_slice = &result.text[start..end];
                        let match_start = idx - start;
                        let match_end = match_start + query.len();

                        rsx! {
                            span { "{before}" }
                            span { "{&text_slice[..match_start]}" }
                            mark { class: "bg-yellow-200 dark:bg-yellow-800 px-0.5 rounded",
                                "{&text_slice[match_start..match_end]}"
                            }
                            span { "{&text_slice[match_end..]}" }
                            span { "{after}" }
                        }
                    } else {
                        // No match highlighting (shouldn't happen)
                        rsx! {
                            span {
                                if result.text.len() > 150 {
                                    "{&result.text[..150]}..."
                                } else {
                                    "{result.text}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
