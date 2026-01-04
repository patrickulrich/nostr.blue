//! Publication Section Component
//! Renders individual publication sections (NKBIP-01 Kind 30041 or nested Kind 30040)

use dioxus::prelude::*;
use std::time::Duration;
use crate::stores::publication_store::{PublicationSection, sections_by_addresses_filter, parse_publication_section};
use crate::stores::nostr_client;
use crate::components::{AsciiDocContent, CitationMetadata};
use crate::components::icons::{ArrowLeftIcon, FileVideoIcon, ClockIcon, ChevronDownIcon, BookOpenIcon};

/// Publication section content renderer
#[component]
pub fn PublicationSectionContent(
    /// The section data
    section: PublicationSection,
    /// Enable book:: link parsing
    #[props(default = true)]
    enable_book_links: bool,
    /// Show section header
    #[props(default = true)]
    show_header: bool,
    /// Callback when citation metadata is available
    #[props(default = None)]
    on_citations_loaded: Option<EventHandler<CitationMetadata>>,
    /// Callback when a child section is selected (for nested indexes)
    #[props(default = None)]
    on_child_select: Option<EventHandler<String>>,
) -> Element {
    // If this is a nested index, show child sections instead of content
    if section.is_index && !section.child_addresses.is_empty() {
        rsx! {
            NestedIndexContent {
                section: section,
                show_header: show_header,
                on_child_select: on_child_select,
            }
        }
    } else {
        rsx! {
            article {
                class: "publication-section",

                // Section header
                if show_header {
                    header {
                        class: "mb-6 pb-4 border-b border-border",
                        h1 {
                            class: "text-2xl font-bold text-foreground",
                            "{section.title}"
                        }
                    }
                }

                // Section content
                AsciiDocContent {
                    content: section.content.clone(),
                    enable_wikilinks: true,
                    enable_book_links: enable_book_links,
                    on_citations_loaded: on_citations_loaded,
                }
            }
        }
    }
}

/// Component for displaying nested index content (kind 30040 with children)
#[component]
fn NestedIndexContent(
    section: PublicationSection,
    show_header: bool,
    on_child_select: Option<EventHandler<String>>,
) -> Element {
    let mut child_sections = use_signal(Vec::<PublicationSection>::new);
    let mut loading = use_signal(|| true);
    let child_addresses = section.child_addresses.clone();

    // Fetch child sections
    use_effect(move || {
        let addresses = child_addresses.clone();
        spawn(async move {
            loading.set(true);
            let filters = sections_by_addresses_filter(&addresses);
            let mut loaded_sections = Vec::new();

            for filter in filters {
                if let Ok(events) = nostr_client::fetch_events_aggregated(
                    filter,
                    Duration::from_secs(10),
                ).await {
                    for event in events {
                        if let Some(section) = parse_publication_section(&event) {
                            loaded_sections.push(section);
                        }
                    }
                }
            }

            // Sort by the order in child_addresses
            let address_order: std::collections::HashMap<_, _> = addresses.iter()
                .enumerate()
                .map(|(i, a)| (a.address.clone(), i))
                .collect();
            loaded_sections.sort_by_key(|s| address_order.get(&s.a_tag).copied().unwrap_or(usize::MAX));

            child_sections.set(loaded_sections);
            loading.set(false);
        });
    });

    rsx! {
        article {
            class: "publication-section",

            // Section header
            if show_header {
                header {
                    class: "mb-6 pb-4 border-b border-border",
                    h1 {
                        class: "text-2xl font-bold text-foreground",
                        "{section.title}"
                    }
                    p {
                        class: "text-muted-foreground mt-2",
                        "{section.child_addresses.len()} chapters"
                    }
                }
            }

            // Child sections list
            if *loading.read() {
                div {
                    class: "space-y-3",
                    for _ in 0..5 {
                        div {
                            class: "h-14 bg-muted/50 rounded-lg animate-pulse"
                        }
                    }
                }
            } else if child_sections.read().is_empty() {
                div {
                    class: "text-center py-8 text-muted-foreground",
                    BookOpenIcon { class: "w-12 h-12 mx-auto mb-3 opacity-50" }
                    p { "No chapters found" }
                }
            } else {
                div {
                    class: "space-y-2",
                    for (idx, child) in child_sections.read().iter().enumerate() {
                        ChildSectionCard {
                            key: "{child.a_tag}",
                            section: child.clone(),
                            index: idx,
                            on_select: on_child_select,
                        }
                    }
                }
            }
        }
    }
}

/// Card component for a child section
#[component]
fn ChildSectionCard(
    section: PublicationSection,
    index: usize,
    on_select: Option<EventHandler<String>>,
) -> Element {
    let a_tag = section.a_tag.clone();
    let has_content = !section.content.is_empty();
    let is_nested = section.is_index;
    let child_count = section.child_addresses.len();

    rsx! {
        button {
            class: "w-full flex items-center gap-4 p-4 rounded-lg border border-border hover:bg-accent/50 transition-colors text-left group",
            onclick: move |_| {
                if let Some(handler) = &on_select {
                    handler.call(a_tag.clone());
                }
            },

            // Chapter number
            div {
                class: "w-8 h-8 rounded-full bg-primary/10 text-primary flex items-center justify-center text-sm font-medium flex-shrink-0",
                "{index + 1}"
            }

            // Title and metadata
            div {
                class: "flex-1 min-w-0",
                h3 {
                    class: "font-medium text-foreground truncate",
                    "{section.title}"
                }
                if is_nested && child_count > 0 {
                    p {
                        class: "text-sm text-muted-foreground",
                        "{child_count} sections"
                    }
                } else if has_content {
                    p {
                        class: "text-sm text-muted-foreground",
                        "{estimate_word_count(&section.content)} words"
                    }
                }
            }

            // Arrow indicator (rotated chevron for "right" direction)
            ChevronDownIcon {
                class: "w-5 h-5 -rotate-90 text-muted-foreground group-hover:text-foreground transition-colors flex-shrink-0"
            }
        }
    }
}

/// Section navigation component
#[component]
pub fn SectionNavigation(
    /// Previous section (if any)
    prev_section: Option<(String, String)>, // (address, title)
    /// Next section (if any)
    next_section: Option<(String, String)>, // (address, title)
    /// Callback when navigation is triggered
    on_navigate: EventHandler<String>,
) -> Element {
    rsx! {
        nav {
            class: "flex items-center justify-between py-6 border-t border-border",

            // Previous
            if let Some((addr, title)) = prev_section {
                button {
                    class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors",
                    onclick: move |_| on_navigate.call(addr.clone()),
                    ArrowLeftIcon { class: "w-4 h-4" }
                    div {
                        class: "text-left",
                        span { class: "text-xs block", "Previous" }
                        span { class: "font-medium line-clamp-1", "{title}" }
                    }
                }
            } else {
                div {}
            }

            // Next
            if let Some((addr, title)) = next_section {
                button {
                    class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors",
                    onclick: move |_| on_navigate.call(addr.clone()),
                    div {
                        class: "text-right",
                        span { class: "text-xs block", "Next" }
                        span { class: "font-medium line-clamp-1", "{title}" }
                    }
                    ArrowLeftIcon { class: "w-4 h-4 rotate-180" }
                }
            } else {
                div {}
            }
        }
    }
}

/// Section loading skeleton
#[component]
pub fn PublicationSectionSkeleton() -> Element {
    rsx! {
        article {
            class: "publication-section animate-pulse",
            header {
                class: "mb-6 pb-4 border-b border-border",
                div { class: "h-8 bg-muted rounded w-2/3" }
            }
            div {
                class: "space-y-4",
                for _ in 0..5 {
                    div { class: "h-4 bg-muted rounded w-full" }
                }
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-5/6" }
            }
        }
    }
}

/// Section metadata display
#[component]
pub fn SectionMetadata(
    section: PublicationSection,
) -> Element {
    rsx! {
        div {
            class: "flex flex-wrap gap-4 text-sm text-muted-foreground",

            // Word count estimate
            div {
                class: "flex items-center gap-1",
                FileVideoIcon { class: "w-4 h-4" }
                span {
                    {format!("{} words", estimate_word_count(&section.content))}
                }
            }

            // Reading time estimate
            div {
                class: "flex items-center gap-1",
                ClockIcon { class: "w-4 h-4" }
                span {
                    {format!("{} min read", estimate_reading_time(&section.content))}
                }
            }
        }
    }
}

/// Estimate word count from content
fn estimate_word_count(content: &str) -> usize {
    content.split_whitespace().count()
}

/// Estimate reading time in minutes (assuming 200 words per minute)
fn estimate_reading_time(content: &str) -> usize {
    let words = estimate_word_count(content);
    std::cmp::max(1, words / 200)
}

/// Section outline (headings extracted from content)
#[component]
pub fn SectionOutline(
    content: String,
    on_heading_click: EventHandler<String>,
) -> Element {
    // Extract headings from AsciiDoc content
    let headings = use_memo(move || extract_headings(&content));

    if headings.read().is_empty() {
        return rsx! {};
    }

    rsx! {
        nav {
            class: "section-outline",
            h4 {
                class: "text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2",
                "On this page"
            }
            ul {
                class: "space-y-1",
                for (level, text, id) in headings.read().iter() {
                    li {
                        style: "padding-left: {(*level - 1) * 8}px",
                        button {
                            class: "text-sm text-muted-foreground hover:text-foreground transition-colors text-left w-full truncate",
                            onclick: {
                                let id = id.clone();
                                move |_| on_heading_click.call(id.clone())
                            },
                            "{text}"
                        }
                    }
                }
            }
        }
    }
}

/// Extract headings from AsciiDoc content
fn extract_headings(content: &str) -> Vec<(usize, String, String)> {
    let heading_pattern = regex::Regex::new(r"(?m)^(=+)\s+(.+)$").unwrap();

    heading_pattern.captures_iter(content)
        .map(|cap| {
            let level = cap.get(1).map(|m| m.as_str().len()).unwrap_or(1);
            let text = cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let id = slug_from_text(&text);
            (level, text, id)
        })
        .collect()
}

/// Generate a URL-safe slug from heading text
fn slug_from_text(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_lowercase().next().unwrap())
            } else if c.is_whitespace() || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
