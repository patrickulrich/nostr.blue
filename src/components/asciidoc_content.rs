//! AsciiDoc Content Component
//! Renders AsciiDoc markup with wikilink support for publications and wiki pages

use dioxus::prelude::*;
use std::collections::HashMap;
use crate::utils::asciidoc::{
    render_asciidoc, render_content_auto, content_to_plain_text,
    render_content_with_citations, content_has_citations, extract_citation_identifiers,
};
use crate::utils::markdown::sanitize_html;
use crate::utils::nip54::extract_wikilinks;
use crate::utils::nkbip03::ResolvedCitation;
use crate::utils::nkbip08::render_book_wikilinks;
use crate::stores::citation_store::fetch_citations_by_identifiers;
use crate::routes::Route;

/// Citation metadata exposed to parent components
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CitationMetadata {
    /// Total number of citations in the content
    pub count: usize,
    /// Whether the content has any citations
    pub has_citations: bool,
    /// List of citation identifiers found
    pub identifiers: Vec<String>,
}

/// AsciiDoc content renderer component
///
/// Renders AsciiDoc or Markdown content to HTML with optional wikilink support.
/// Automatically detects the content format and uses the appropriate renderer.
/// Supports standard AsciiDoc and Markdown features like headings, lists, code blocks, etc.
#[component]
pub fn AsciiDocContent(
    /// The source content to render (AsciiDoc or Markdown - auto-detected)
    content: String,
    /// Enable wikilink parsing and rendering (for wiki pages)
    #[props(default = true)]
    enable_wikilinks: bool,
    /// Enable book:: macro parsing (for publications)
    #[props(default = false)]
    enable_book_links: bool,
    /// Enable NKBIP-03 citation support
    #[props(default = true)]
    enable_citations: bool,
    /// Custom CSS classes to apply to the container
    #[props(default = String::new())]
    class: String,
    /// Callback when citation metadata is available
    #[props(default = None)]
    on_citations_loaded: Option<EventHandler<CitationMetadata>>,
) -> Element {
    // State for resolved citations
    let mut resolved_citations: Signal<HashMap<String, ResolvedCitation>> = use_signal(HashMap::new);
    let mut citations_loading = use_signal(|| false);
    let mut citations_error = use_signal(|| false);

    // Generation token to guard async citation fetches - ensures only the latest fetch updates state
    let mut fetch_generation = use_signal(|| 0u64);

    // Track last notified citation metadata to prevent redundant callbacks
    let mut last_notified_metadata: Signal<Option<CitationMetadata>> = use_signal(|| None);

    // Check if content has citations on mount and fetch them
    // Clone content for use in the effect - necessary because content is also used later in render logic
    let content_for_effect = content.clone();
    use_effect(use_reactive!(|(content_for_effect, enable_citations)| {
        // Clear stale state when citations are disabled or content is empty
        if !enable_citations || content_for_effect.is_empty() || !content_has_citations(&content_for_effect) {
            citations_loading.set(false);
            citations_error.set(false);
            resolved_citations.set(HashMap::new());
            return;
        }

        let identifiers = extract_citation_identifiers(&content_for_effect);
        if identifiers.is_empty() {
            citations_loading.set(false);
            citations_error.set(false);
            return;
        }

        // Increment generation token for this fetch
        let current_generation = *fetch_generation.read() + 1;
        fetch_generation.set(current_generation);
        citations_loading.set(true);

        spawn(async move {
            let result = fetch_citations_by_identifiers(&identifiers).await;

            // Only apply results if this is still the latest fetch
            if *fetch_generation.read() != current_generation {
                return;
            }

            match result {
                Ok(cached) => {
                    // Convert CachedCitation to ResolvedCitation
                    let resolved: HashMap<String, ResolvedCitation> = cached
                        .into_iter()
                        .map(|(id, cached_cit)| {
                            let resolved = ResolvedCitation::from_citation(
                                id.clone(),
                                &cached_cit.citation,
                            );
                            (id, resolved)
                        })
                        .collect();
                    resolved_citations.set(resolved);
                    citations_error.set(false);
                }
                Err(e) => {
                    crate::utils::log_fetch_error("citations for article", e);
                    citations_error.set(true);
                }
            }
            citations_loading.set(false);
        });
    }));

    // Render content with citations if enabled
    let (rendered, footnotes_html, endnotes_html, citation_metadata) = if enable_citations {
        let result = render_content_with_citations(&content, &resolved_citations.read());
        let mut html = result.html;

        // Process book:: links if enabled
        if enable_book_links {
            html = render_book_wikilinks(&html);
        }

        // Build citation metadata from the result
        let metadata = CitationMetadata {
            count: result.citations.len(),
            has_citations: result.has_citations,
            identifiers: result.citation_identifiers,
        };

        (html, result.footnotes_html, result.endnotes_html, Some(metadata))
    } else {
        // Process content through our renderers with automatic format detection
        let mut html = if enable_wikilinks {
            render_content_auto(&content)
        } else {
            render_asciidoc(&content)
        };

        // Process book:: links if enabled
        if enable_book_links {
            html = render_book_wikilinks(&html);
        }

        (html, String::new(), String::new(), None)
    };

    // Notify parent of citation metadata if callback is provided
    // Only call handler when metadata actually changes from previous call to avoid infinite loops
    let citation_metadata_for_effect = citation_metadata.clone();
    use_effect(use_reactive!(|citation_metadata_for_effect| {
        if let (Some(ref handler), Some(ref metadata)) = (&on_citations_loaded, &citation_metadata_for_effect) {
            // Only notify if metadata has changed from what we last notified
            let should_notify = last_notified_metadata.read().as_ref() != Some(metadata);
            if should_notify {
                last_notified_metadata.set(Some(metadata.clone()));
                handler.call(metadata.clone());
            }
        }
    }));

    // Sanitize main content HTML to prevent XSS
    let sanitized_content = sanitize_html(&rendered);

    rsx! {
        div {
            class: "asciidoc-content prose prose-sm dark:prose-invert max-w-none {class}",
            dangerous_inner_html: "{sanitized_content}",
        }
        // Show subtle loading indicator while citations are being fetched
        if *citations_loading.read() {
            div {
                class: "text-xs text-muted-foreground mt-2 flex items-center gap-1",
                role: "status",
                aria_live: "polite",
                span { class: "animate-pulse motion-reduce:animate-none", "⏳" }
                span { "Loading citations..." }
            }
        }
        // Show subtle error indicator if citations failed to load
        if *citations_error.read() {
            div {
                class: "text-xs text-muted-foreground mt-2 flex items-center gap-1",
                role: "alert",
                span { "⚠" }
                span { "Some citations could not be loaded" }
            }
        }
        // Render footnotes section if present (sanitized for XSS protection)
        if !footnotes_html.is_empty() {
            div {
                class: "citation-footnotes-container",
                dangerous_inner_html: "{sanitize_html(&footnotes_html)}",
            }
        }
        // Render endnotes/references section if present (sanitized for XSS protection)
        if !endnotes_html.is_empty() {
            div {
                class: "citation-endnotes-container mt-8 pt-4 border-t border-border",
                dangerous_inner_html: "{sanitize_html(&endnotes_html)}",
            }
        }
    }
}

/// Compact AsciiDoc/Markdown content renderer with collapsing support
#[component]
pub fn AsciiDocContentCollapsible(
    /// The source content to render (AsciiDoc or Markdown - auto-detected)
    content: String,
    /// Enable wikilink parsing and rendering
    #[props(default = true)]
    enable_wikilinks: bool,
    /// Maximum height before collapsing (in rem)
    #[props(default = 24.0)]
    max_height: f64,
    /// Custom CSS classes
    #[props(default = String::new())]
    class: String,
) -> Element {
    let mut is_expanded = use_signal(|| false);

    // Estimate if content is long based on character count
    let is_long_content = content.chars().count() > 800;

    // Compute rendered content directly - props are not reactive so use_memo wouldn't recompute on changes
    // Sanitize to prevent XSS
    let rendered = {
        let html = if enable_wikilinks {
            render_content_auto(&content)
        } else {
            render_asciidoc(&content)
        };
        sanitize_html(&html)
    };

    if is_long_content {
        rsx! {
            div {
                class: "relative {class}",
                div {
                    class: "asciidoc-content prose prose-sm dark:prose-invert max-w-none overflow-hidden",
                    // Use inline style for dynamic max-height since Tailwind can't detect format! classes
                    style: if *is_expanded.read() { String::new() } else { format!("max-height: {}rem;", max_height) },
                    dangerous_inner_html: "{rendered}",
                }
                if !*is_expanded.read() {
                    div {
                        class: "absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-background via-background/95 to-transparent flex items-end justify-center pb-1",
                        button {
                            class: "px-4 py-1.5 text-sm font-medium text-primary border border-border rounded-md bg-background hover:bg-accent transition-colors",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                is_expanded.set(true);
                            },
                            "Show More"
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "asciidoc-content prose prose-sm dark:prose-invert max-w-none {class}",
                dangerous_inner_html: "{rendered}",
            }
        }
    }
}

/// Preview component for content (summary/excerpt view)
///
/// Automatically detects Markdown vs AsciiDoc and strips formatting appropriately.
#[component]
pub fn AsciiDocPreview(
    /// The source content (Markdown or AsciiDoc - auto-detected)
    content: String,
    /// Maximum number of characters to show
    #[props(default = 200)]
    max_chars: usize,
    /// Custom CSS classes
    #[props(default = String::new())]
    class: String,
) -> Element {
    // Compute preview directly - props are not reactive so use_memo wouldn't recompute on changes
    let plain = content_to_plain_text(&content);
    let preview = if plain.chars().count() > max_chars {
        let truncated: String = plain.chars().take(max_chars).collect();
        format!("{}...", truncated.trim_end())
    } else {
        plain
    };

    rsx! {
        p {
            class: "text-muted-foreground text-sm line-clamp-3 {class}",
            "{preview}"
        }
    }
}

/// Extract wikilinks from content for display
#[component]
pub fn WikilinksList(
    /// The AsciiDoc source content
    content: String,
    /// CSS classes
    #[props(default = String::new())]
    class: String,
) -> Element {
    // Compute links directly - props are not reactive so use_memo wouldn't recompute on changes
    let links = extract_wikilinks(&content);

    // Early return if no links
    if links.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "flex flex-wrap gap-2 {class}",
            for (idx, link) in links.iter().enumerate() {
                Link {
                    key: "{link.target}-{idx}",
                    class: "inline-flex items-center px-2 py-1 text-xs bg-accent rounded-md hover:bg-accent/80 transition-colors",
                    to: Route::WikiDetail { identifier: link.target.clone() },
                    {link.display_text().to_string()}
                }
            }
        }
    }
}

