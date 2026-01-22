//! Bible Search Page
//! Client-side search through cached chapters

use dioxus::prelude::*;
use dioxus::html::input_data::keyboard_types::Key;

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
    // Track the trimmed query that was actually used for search (for accurate highlighting)
    let mut last_searched_query = use_signal(String::new);

    // Count cached chapters (uses efficient count-only function, no cloning)
    let cached_count = cached_chapter_count();

    // Perform search
    let mut perform_search = move || {
        // Trim whitespace to prevent whitespace-only queries passing length check
        let q = query.read().trim().to_string();
        // Use chars().count() for Unicode-safe length check (not bytes)
        if q.chars().count() >= 3 {
            let found = search_cached_verses(&q, 50);
            results.set(found);
            has_searched.set(true);
            // Store the trimmed query for accurate highlighting
            last_searched_query.set(q);
        } else {
            results.set(Vec::new());
            has_searched.set(false);
            last_searched_query.set(String::new());
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
                    class: "w-full px-4 py-3 pr-12 border border-border rounded-full focus:outline-hidden focus:ring-2 focus:ring-primary bg-background",
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
                                query: last_searched_query.read().clone()
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
/// Iterates the original haystack directly to ensure byte offsets are correct,
/// even when case-folding changes character counts (e.g., ß → ss).
/// Returns (start_byte, end_byte) or None if no match.
fn find_match_byte_range(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    // Pre-compute lowercased needle chars for comparison
    let needle_lower: Vec<char> = needle.to_lowercase().chars().collect();
    if needle_lower.is_empty() {
        return None;
    }

    // Collect char_indices from original haystack for byte offset tracking
    let char_indices: Vec<(usize, char)> = haystack.char_indices().collect();
    if char_indices.is_empty() {
        return None;
    }

    let mut match_start_idx: Option<usize> = None; // Index into char_indices
    let mut needle_pos = 0; // Current position in needle_lower

    for (idx, &(_byte_pos, c)) in char_indices.iter().enumerate() {
        // Case-fold the current haystack character for comparison
        // Note: to_lowercase() can yield multiple chars (e.g., İ → i̇), but
        // we handle the common case where it yields exactly one char
        let c_lower: Vec<char> = c.to_lowercase().collect();

        // Try to match against needle characters
        let mut matched_in_this_char = false;
        for lc in &c_lower {
            if needle_pos < needle_lower.len() && *lc == needle_lower[needle_pos] {
                if needle_pos == 0 {
                    match_start_idx = Some(idx);
                }
                needle_pos += 1;
                matched_in_this_char = true;
            } else if matched_in_this_char {
                // We were matching but this sub-char doesn't match
                // Reset and check if this sub-char starts a new match
                needle_pos = 0;
                match_start_idx = None;
                if *lc == needle_lower[0] {
                    match_start_idx = Some(idx);
                    needle_pos = 1;
                }
            }
        }

        // If we didn't match any sub-char, reset
        if !matched_in_this_char && c_lower.iter().all(|lc| *lc != needle_lower[0]) {
            needle_pos = 0;
            match_start_idx = None;
        } else if !matched_in_this_char {
            // Check if any sub-char starts a new match
            needle_pos = 0;
            match_start_idx = None;
            for lc in &c_lower {
                if *lc == needle_lower[0] {
                    match_start_idx = Some(idx);
                    needle_pos = 1;
                    break;
                }
            }
        }

        // Check if we've completed the match
        if needle_pos == needle_lower.len() {
            let start_idx = match_start_idx.unwrap();
            let start_byte = char_indices[start_idx].0;
            // End byte is either the start of the next char or end of string
            let end_byte = if idx + 1 < char_indices.len() {
                char_indices[idx + 1].0
            } else {
                haystack.len()
            };
            return Some((start_byte, end_byte));
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
