//! Quran Search Page
//! Client-side search through cached ayahs + API search fallback
use crate::services::quran_api::DEFAULT_TRANSLATION_EDITION;
use crate::stores::quran_store::{cached_surah_count, search_cached_ayahs, QuranSearchResult};
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;

#[component]
pub fn QuranSearch() -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<QuranSearchResult>::new);
    let mut has_searched = use_signal(|| false);
    let mut searching = use_signal(|| false);
    let cached_count = cached_surah_count();
    let translation = use_signal(|| DEFAULT_TRANSLATION_EDITION.to_string());

    let mut perform_search = move || {
        let q = query.read().trim().to_string();
        if q.chars().count() < 3 {
            results.set(Vec::new());
            has_searched.set(false);
            return;
        }
        let cached_results = search_cached_ayahs(&q, 50);
        if !cached_results.is_empty() {
            results.set(cached_results);
            has_searched.set(true);
            return;
        }
        let edition = translation.read().clone();
        spawn(async move {
            searching.set(true);
            match crate::services::quran_api::fetch_search(&q, "all", &edition).await {
                Ok(data) => {
                    let api_results: Vec<QuranSearchResult> = data
                        .matches
                        .iter()
                        .map(|m| QuranSearchResult {
                            surah: m.surah.number,
                            surah_name: m.surah.english_name.clone(),
                            ayah: m.number,
                            ayah_in_surah: m.number_in_surah,
                            text: m.text.clone(),
                            edition: m.edition.identifier.clone(),
                        })
                        .collect();
                    results.set(api_results);
                }
                Err(e) => {
                    log::error!("Quran search failed: {}", e);
                    results.set(Vec::new());
                }
            }
            has_searched.set(true);
            searching.set(false);
        });
    };

    rsx! {
        div { class: "max-w-3xl mx-auto p-4 space-y-6",
            div { class: "flex items-center gap-4",
                Link {
                    to: crate::routes::Route::QuranHome {},
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
                            d: "M15 19l-7-7 7-7",
                        }
                    }
                }
                h1 { class: "text-2xl font-bold", "Search Quran" }
            }

            div { class: "relative",
                input {
                    r#type: "text",
                    placeholder: "Search ayahs... (min 3 characters)",
                    class: "w-full px-4 py-3 pr-12 border border-border rounded-full bg-card text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50",
                    value: "{query}",
                    oninput: move |evt| { query.set(evt.value()); },
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter { perform_search(); }
                    },
                }
                button {
                    class: "absolute right-2 top-1/2 -translate-y-1/2 p-2 hover:bg-muted rounded-full transition",
                    onclick: move |_| perform_search(),
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-5 h-5 text-muted-foreground",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
                        }
                    }
                }
            }

            div { class: "text-sm text-muted-foreground",
                if *searching.read() {
                    p { "Searching..." }
                } else if cached_count == 0 {
                    p { "No surahs cached yet. Search will use the API." }
                } else {
                    p { "Searching {cached_count} cached surahs + API fallback" }
                }
            }

            if *has_searched.read() {
                if results.read().is_empty() {
                    div { class: "text-center py-12 text-muted-foreground",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-12 h-12 mx-auto mb-4 opacity-50",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z",
                            }
                        }
                        p { "No results found" }
                        p { class: "text-sm mt-1", "Try different keywords or read surahs first to build cache." }
                    }
                } else {
                    div { class: "text-sm text-muted-foreground mb-2",
                        "{results.read().len()} results"
                    }
                    div { class: "space-y-3",
                        for result in results.read().iter() {
                            SearchResultCard { result: result.clone(), query: query.read().clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchResultCard(result: QuranSearchResult, query: String) -> Element {
    let display_text = if result.text.len() > 200 {
        format!("{}...", &result.text[..200.min(result.text.len())])
    } else {
        result.text.clone()
    };
    let highlighted = highlight_match(&display_text, &query);

    rsx! {
        Link {
            to: crate::routes::Route::QuranSurah { surah: result.surah },
            class: "block p-4 bg-card border border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition",
            div { class: "flex items-start justify-between",
                div { class: "flex-1 min-w-0" }
            }
            div { class: "flex items-center gap-2 text-sm text-muted-foreground mb-1",
                span { class: "font-medium text-foreground", "{result.surah_name}" }
                span { "·" }
                span { "Ayah {result.ayah_in_surah}" }
            }
            div { class: "text-sm leading-relaxed",
                dangerous_inner_html: "{highlighted}"
            }
        }
    }
}

fn highlight_match(text: &str, query: &str) -> String {
    if query.is_empty() {
        return html_escape(text);
    }
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();
    let mut result = String::new();
    let mut last_end = 0;
    if let Some(pos) = text_lower.find(&query_lower) {
        result.push_str(&html_escape(&text[last_end..pos]));
        result.push_str("<mark class=\"bg-yellow-200 dark:bg-yellow-800 rounded px-0.5\">");
        result.push_str(&html_escape(&text[pos..pos + query.len()]));
        result.push_str("</mark>");
        last_end = pos + query.len();
    }
    result.push_str(&html_escape(&text[last_end..]));
    result
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
