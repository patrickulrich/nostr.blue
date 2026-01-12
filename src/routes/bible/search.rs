//! Bible Search Page
//! Client-side search through cached chapters

use dioxus::prelude::*;

use crate::stores::bible_store::{
    BibleSearchResult, search_cached_verses, cached_chapter_count,
};

/// Bible Search Page
#[component]
pub fn BibleSearch() -> Element {
    // State
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<BibleSearchResult>::new);
    let mut has_searched = use_signal(|| false);

    // Count cached chapters (uses efficient count-only function, no cloning)
    let cached_count = cached_chapter_count();

    // Perform search
    let mut perform_search = move || {
        let q = query.read().clone();
        // Use chars().count() for Unicode-safe length check (not bytes)
        if q.chars().count() >= 3 {
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

/// Find the nearest valid UTF-8 char boundary at or before the given byte index.
/// This prevents panics when slicing strings with multi-byte characters (Hebrew, Greek, etc.)
fn floor_char_boundary(s: &str, byte_idx: usize) -> usize {
    if byte_idx >= s.len() {
        return s.len();
    }
    let mut idx = byte_idx;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Find the nearest valid UTF-8 char boundary at or after the given byte index.
fn ceil_char_boundary(s: &str, byte_idx: usize) -> usize {
    if byte_idx >= s.len() {
        return s.len();
    }
    let mut idx = byte_idx;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// Truncate a string to approximately n characters, respecting char boundaries.
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    if s.chars().count() <= max_chars {
        return s;
    }
    // Find byte position of the nth character
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

/// Find a case-insensitive match and return byte offsets safe for slicing.
/// Uses character-level matching to handle Unicode case-folding correctly.
/// Returns (start_byte, end_byte) or None if no match.
fn find_match_byte_range(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let haystack_lower = haystack.to_lowercase();
    let needle_lower = needle.to_lowercase();

    // Find character position first
    let needle_chars: Vec<char> = needle_lower.chars().collect();
    if needle_chars.is_empty() {
        return None;
    }

    let mut char_start = None;
    let mut match_count = 0;

    for (char_idx, c) in haystack_lower.chars().enumerate() {
        if c == needle_chars[match_count] {
            if match_count == 0 {
                char_start = Some(char_idx);
            }
            match_count += 1;
            if match_count == needle_chars.len() {
                // Found complete match - now convert to byte indices
                let start_char = char_start.unwrap();
                let end_char = char_idx + 1;

                // Convert char indices to byte indices using the original string
                let start_byte = haystack.char_indices()
                    .nth(start_char)
                    .map(|(i, _)| i)?;
                let end_byte = haystack.char_indices()
                    .nth(end_char)
                    .map(|(i, _)| i)
                    .unwrap_or(haystack.len());

                return Some((start_byte, end_byte));
            }
        } else {
            match_count = 0;
            char_start = None;
            // Check if current char starts a new match
            if c == needle_chars[0] {
                char_start = Some(char_idx);
                match_count = 1;
            }
        }
    }
    None
}

/// Search result card
#[component]
fn SearchResultCard(result: BibleSearchResult, query: String) -> Element {
    let reference = format!(
        "{} {}:{} ({})",
        result.book_name, result.chapter, result.verse, result.translation
    );

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
                    // Use Unicode-safe character-level matching
                    if let Some((match_start, match_end)) = find_match_byte_range(&result.text, &query) {
                        // Calculate context window around the match
                        let raw_start = match_start.saturating_sub(50);
                        let raw_end = match_end + 50;

                        // Find safe char boundaries in the original text
                        let start = floor_char_boundary(&result.text, raw_start);
                        let end = ceil_char_boundary(&result.text, raw_end.min(result.text.len()));

                        let before = if start > 0 { "..." } else { "" };
                        let after = if end < result.text.len() { "..." } else { "" };

                        let text_slice = &result.text[start..end];

                        // Find the match position within the slice using Unicode-safe matching
                        if let Some((slice_match_start, slice_match_end)) = find_match_byte_range(text_slice, &query) {
                            rsx! {
                                span { "{before}" }
                                span { "{&text_slice[..slice_match_start]}" }
                                mark { class: "bg-yellow-200 dark:bg-yellow-800 px-0.5 rounded",
                                    "{&text_slice[slice_match_start..slice_match_end]}"
                                }
                                span { "{&text_slice[slice_match_end..]}" }
                                span { "{after}" }
                            }
                        } else {
                            // Fallback if match not found in slice (shouldn't happen)
                            rsx! {
                                span { "{before}{text_slice}{after}" }
                            }
                        }
                    } else {
                        // No match highlighting (shouldn't happen)
                        rsx! {
                            span {
                                {
                                    let truncated = truncate_chars(&result.text, 150);
                                    if truncated.len() < result.text.len() {
                                        format!("{}...", truncated)
                                    } else {
                                        result.text.clone()
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
