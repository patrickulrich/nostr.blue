//! Book Picker Modal
//! Select publications and book references to insert into wiki pages and publications

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use crate::stores::publication_store::{
    PublicationIndex, search_publications, get_all_cached_publications,
    fetch_publications,
};
use crate::utils::nkbip08::BookReference;
use crate::components::icons::{XIcon, SearchIcon, BookOpenIcon, ChevronDownIcon};

// ============================================================================
// Input Validation Functions (Security Fix #1)
// ============================================================================

/// Validate book version input against allowed format.
/// Only lowercase letters, digits, and hyphens are allowed (per NKBIP-08).
fn is_valid_book_version(input: &str) -> bool {
    !input.is_empty()
        && input.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Validate book sections input against allowed format.
/// Only digits, commas, and hyphens are allowed (e.g., "1-5,7,10-12").
/// Also validates semantic correctness: no empty segments, valid ranges.
fn is_valid_book_sections(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }

    // Split by comma to get segments
    for segment in input.split(',') {
        let trimmed = segment.trim();

        // Empty segment (e.g., "1,,2" or leading/trailing commas)
        if trimmed.is_empty() {
            return false;
        }

        // Check if it's a single number or range
        if let Some((start, end)) = trimmed.split_once('-') {
            // Range format: N-M
            let start_num: u32 = match start.trim().parse() {
                Ok(n) if n > 0 => n,
                _ => return false, // Invalid or zero start
            };
            let end_num: u32 = match end.trim().parse() {
                Ok(n) if n > 0 => n,
                _ => return false, // Invalid or zero end
            };

            // Validate range: start <= end
            if start_num > end_num {
                return false;
            }
        } else {
            // Single number - must be valid positive integer
            match trimmed.parse::<u32>() {
                Ok(n) if n > 0 => {}
                _ => return false,
            }
        }
    }

    true
}

/// Validates that a URL is safe for use as an image source.
/// Returns true only for https:// URLs with reasonable length.
fn is_safe_image_url(url: &str) -> bool {
    const MAX_URL_LENGTH: usize = 2048;

    // Check length
    if url.len() > MAX_URL_LENGTH {
        return false;
    }

    // Only allow https:// scheme
    let url_lower = url.to_lowercase();
    if !url_lower.starts_with("https://") {
        return false;
    }

    // Reject dangerous schemes that might be URL-encoded or obfuscated
    let dangerous_patterns = ["javascript:", "data:", "file:", "vbscript:"];
    for pattern in dangerous_patterns {
        if url_lower.contains(pattern) {
            return false;
        }
    }

    true
}

/// Book selection result
#[derive(Clone, Debug)]
pub struct BookSelection {
    /// The book reference
    pub reference: BookReference,
    /// Generated markup to insert (e.g., "book::bible:genesis 2:4-9 | kjv")
    pub markup: String,
}

/// Tab selection for picker
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum BookPickerTab {
    #[default]
    Browse,
    Search,
}

#[derive(Props, Clone, PartialEq)]
pub struct BookPickerModalProps {
    /// Signal controlling modal visibility
    pub show: Signal<bool>,
    /// Callback when book reference is selected
    pub on_select: EventHandler<BookSelection>,
}

#[component]
pub fn BookPickerModal(mut props: BookPickerModalProps) -> Element {
    let mut active_tab = use_signal(|| BookPickerTab::Browse);
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(Vec::<PublicationIndex>::new);
    let mut is_searching = use_signal(|| false);
    // Debounce counter to cancel stale search requests
    let mut debounce_counter = use_signal(|| 0u32);

    // Selected publication and reference building
    let mut selected_publication = use_signal(|| None::<PublicationIndex>);
    let mut selected_chapter = use_signal(|| None::<String>);
    let mut selected_sections = use_signal(String::new);
    let mut selected_version = use_signal(String::new);

    // Validation error signals (Security Fix #1)
    let mut version_error = use_signal(|| false);
    let mut sections_error = use_signal(|| false);

    // Loading state
    let mut loading = use_signal(|| false);
    // Error state for fetch/search operations
    let mut fetch_error = use_signal(|| None::<String>);

    // Load publications when modal opens
    use_effect(use_reactive(
        &*props.show.read(),
        move |is_shown| {
            if is_shown {
                loading.set(true);
                fetch_error.set(None);
                spawn(async move {
                    match fetch_publications(100, None).await {
                        Ok(_) => fetch_error.set(None),
                        Err(e) => {
                            crate::utils::log_fetch_error("publications", e.clone());
                            fetch_error.set(Some(format!("Failed to load publications: {}", e)));
                        }
                    }
                    loading.set(false);
                });
                // Reset selection and validation errors when opening
                selected_publication.set(None);
                selected_chapter.set(None);
                selected_sections.set(String::new());
                selected_version.set(String::new());
                version_error.set(false);
                sections_error.set(false);
                search_query.set(String::new());
                search_results.set(Vec::new());
            }
        },
    ));

    // Handle search with 300ms debouncing to avoid excessive requests
    let mut handle_search = move |query: String| {
        search_query.set(query.clone());

        if query.is_empty() {
            search_results.set(Vec::new());
            is_searching.set(false);
            return;
        }

        // Increment counter to invalidate any pending search
        debounce_counter.set(debounce_counter() + 1);
        let current_counter = debounce_counter();

        is_searching.set(true);
        spawn(async move {
            // Wait 300ms before searching
            TimeoutFuture::new(300).await;

            // Check if this search is still valid (no newer keystroke)
            if debounce_counter() != current_counter {
                return; // Stale request, another search is pending
            }

            match search_publications(&query, 50).await {
                Ok(results) => {
                    // Double-check counter after async operation
                    if debounce_counter() == current_counter {
                        search_results.set(results);
                        fetch_error.set(None);
                    }
                }
                Err(e) => {
                    log::warn!("Publication search failed: {}", e);
                    if debounce_counter() == current_counter {
                        fetch_error.set(Some(format!("Search failed: {}", e)));
                    }
                }
            }

            // Only clear searching state if this is still the current search
            if debounce_counter() == current_counter {
                is_searching.set(false);
            }
        });
    };

    // Get publications to display based on tab
    let publications_to_display = use_memo(move || {
        if *active_tab.read() == BookPickerTab::Search && !search_query.read().is_empty() {
            search_results.read().clone()
        } else {
            get_all_cached_publications()
        }
    });

    // Validation effect - runs when inputs change (no side effects in memo)
    use_effect(use_reactive!(|(selected_version, selected_sections)| {
        let version = selected_version.read();
        let sections = selected_sections.read();

        // Validate version: error if non-empty and invalid
        version_error.set(!version.is_empty() && !is_valid_book_version(&version));
        // Validate sections: error if non-empty and invalid
        sections_error.set(!sections.is_empty() && !is_valid_book_sections(&sections));
    }));

    // Build book reference from selections (pure computation - no side effects)
    let book_reference = use_memo(move || {
        selected_publication.read().as_ref().map(|pub_| {
            let mut reference = BookReference::new(&pub_.d_tag);

            // Add version only if valid
            let version_input = selected_version.read();
            if !version_input.is_empty() && is_valid_book_version(&version_input) {
                reference = reference.with_version(&version_input);
            }

            // Add chapter (validated by dropdown selection)
            if let Some(ref chapter) = *selected_chapter.read() {
                reference = reference.with_chapter(chapter);
            }

            // Add sections only if valid
            let sections_str = selected_sections.read();
            if !sections_str.is_empty() && is_valid_book_sections(&sections_str) {
                for section in sections_str.split(',') {
                    let section = section.trim();
                    if !section.is_empty() {
                        reference = reference.with_section(section);
                    }
                }
            }

            reference
        })
    });

    // Track overall validation state (for disabling Insert button)
    let has_validation_error = use_memo(move || {
        *version_error.read() || *sections_error.read()
    });

    // Generate markup preview
    let markup_preview = use_memo(move || {
        book_reference.read().as_ref().map(|r| r.raw.clone()).unwrap_or_default()
    });

    // Close modal with search-in-progress warning
    let close_modal = move |_| {
        if *is_searching.read() {
            let confirmed = web_sys::window()
                .and_then(|w| w.confirm_with_message("A search is in progress. Close anyway?").ok())
                .unwrap_or(false);
            if !confirmed {
                return;
            }
        }
        props.show.set(false);
    };

    // Handle publication selection
    let mut handle_publication_click = move |publication: PublicationIndex| {
        selected_publication.set(Some(publication));
        selected_chapter.set(None);
        selected_sections.set(String::new());
        selected_version.set(String::new());
    };

    // Handle chapter selection
    let mut handle_chapter_click = move |chapter: String| {
        selected_chapter.set(Some(chapter));
    };

    // Handle insert
    let handle_insert = move |_| {
        // Guard against validation errors (keyboard/programmatic triggers)
        if *has_validation_error.read() {
            return;
        }
        if let Some(reference) = book_reference.read().clone() {
            let markup = reference.raw.clone();
            props.on_select.call(BookSelection {
                reference,
                markup,
            });
            props.show.set(false);
        }
    };

    if !*props.show.read() {
        return rsx! {};
    }

    rsx! {
        // Backdrop (click to close)
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: close_modal,

            // Modal content (dialog semantics + Escape key)
            div {
                class: "bg-background border border-border rounded-xl shadow-xl w-full max-w-3xl max-h-[85vh] overflow-hidden flex flex-col",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "book-picker-title",
                tabindex: "-1",
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        // Check for search-in-progress before closing (same logic as close_modal)
                        if *is_searching.read() {
                            let confirmed = web_sys::window()
                                .and_then(|w| w.confirm_with_message("A search is in progress. Close anyway?").ok())
                                .unwrap_or(false);
                            if !confirmed {
                                return;
                            }
                        }
                        props.show.set(false);
                    }
                },

                // Header
                div {
                    class: "flex items-center justify-between px-6 py-4 border-b border-border",

                    div {
                        class: "flex items-center gap-2",
                        BookOpenIcon { class: "w-5 h-5 text-primary".to_string() }
                        h2 {
                            id: "book-picker-title",
                            class: "text-lg font-semibold",
                            "Insert Book Reference"
                        }
                    }

                    button {
                        class: "p-2 text-muted-foreground hover:text-foreground rounded-lg hover:bg-accent transition-colors",
                        onclick: close_modal,
                        XIcon { class: "w-5 h-5".to_string() }
                    }
                }

                // Tab bar and search
                div {
                    class: "px-6 py-3 border-b border-border space-y-3",

                    // Tabs
                    div {
                        class: "flex gap-2",

                        button {
                            class: if *active_tab.read() == BookPickerTab::Browse {
                                "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground"
                            } else {
                                "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors"
                            },
                            onclick: move |_| {
                                active_tab.set(BookPickerTab::Browse);
                                // Bug Fix #11: Invalidate pending searches when switching tabs
                                debounce_counter.set(debounce_counter() + 1);
                                search_query.set(String::new());
                                search_results.set(Vec::new());
                                is_searching.set(false);
                            },
                            "Browse"
                        }

                        button {
                            class: if *active_tab.read() == BookPickerTab::Search {
                                "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground"
                            } else {
                                "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors"
                            },
                            onclick: move |_| active_tab.set(BookPickerTab::Search),
                            "Search"
                        }
                    }

                    // Search input (shown when Search tab is active)
                    if *active_tab.read() == BookPickerTab::Search {
                        div {
                            class: "relative",
                            div {
                                class: "absolute inset-y-0 left-3 flex items-center pointer-events-none",
                                SearchIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                            }
                            input {
                                r#type: "text",
                                class: "w-full pl-10 pr-4 py-2 bg-muted/50 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50",
                                placeholder: "Search by title, author...",
                                value: "{search_query}",
                                oninput: move |e| handle_search(e.value()),
                            }
                        }
                    }
                }

                // Content area - two columns
                div {
                    class: "flex-1 overflow-hidden flex flex-col md:flex-row",

                    // Publication list
                    div {
                        class: "flex-1 overflow-y-auto p-4 border-b md:border-b-0 md:border-r border-border",

                        if *loading.read() || *is_searching.read() {
                            div {
                                class: "flex items-center justify-center py-8",
                                div {
                                    class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary"
                                }
                            }
                        } else if let Some(ref err) = *fetch_error.read() {
                            div {
                                class: "text-center py-8",
                                p {
                                    class: "text-red-600 dark:text-red-400",
                                    "{err}"
                                }
                                p {
                                    class: "text-sm mt-2 text-muted-foreground",
                                    "Check your connection and try again"
                                }
                            }
                        } else if publications_to_display.read().is_empty() {
                            div {
                                class: "text-center py-8 text-muted-foreground",
                                p { "No publications found" }
                                p {
                                    class: "text-sm mt-2",
                                    "Try searching or check your relay connections"
                                }
                            }
                        } else {
                            div {
                                class: "space-y-2",
                                for publication in publications_to_display.read().iter() {
                                    {
                                        let pub_clone = publication.clone();
                                        let is_selected = selected_publication.read()
                                            .as_ref()
                                            .map(|s| s.a_tag == publication.a_tag)
                                            .unwrap_or(false);

                                        rsx! {
                                            button {
                                                key: "{publication.a_tag}",
                                                class: format!(
                                                    "w-full text-left p-3 rounded-lg border transition-colors {}",
                                                    if is_selected {
                                                        "border-primary bg-primary/10"
                                                    } else {
                                                        "border-border hover:bg-accent/50"
                                                    }
                                                ),
                                                onclick: move |_| handle_publication_click(pub_clone.clone()),

                                                div {
                                                    class: "flex items-start gap-3",
                                                    // Cover image or placeholder (with URL validation)
                                                    div {
                                                        class: "w-12 h-16 flex-shrink-0 rounded bg-muted flex items-center justify-center",
                                                        if let Some(ref img) = publication.cover_image {
                                                            if is_safe_image_url(img) {
                                                                img {
                                                                    class: "w-full h-full object-cover rounded",
                                                                    src: "{img}",
                                                                    alt: "",
                                                                }
                                                            } else {
                                                                BookOpenIcon { class: "w-6 h-6 text-muted-foreground".to_string() }
                                                            }
                                                        } else {
                                                            BookOpenIcon { class: "w-6 h-6 text-muted-foreground".to_string() }
                                                        }
                                                    }
                                                    // Publication info
                                                    div {
                                                        class: "flex-1 min-w-0",
                                                        p {
                                                            class: "font-medium text-foreground truncate",
                                                            "{publication.title}"
                                                        }
                                                        if let Some(ref author) = publication.author {
                                                            p {
                                                                class: "text-sm text-muted-foreground truncate",
                                                                "by {author}"
                                                            }
                                                        }
                                                        p {
                                                            class: "text-xs text-muted-foreground mt-1",
                                                            "{publication.section_addresses.len()} sections"
                                                        }
                                                    }
                                                    if is_selected {
                                                        ChevronDownIcon { class: "w-5 h-5 text-primary flex-shrink-0 -rotate-90".to_string() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Reference builder panel
                    div {
                        class: "w-full md:w-80 p-4 bg-muted/30 flex flex-col",

                        if selected_publication.read().is_some() {
                            div {
                                class: "space-y-4 flex-1",

                                // Chapter selection (if publication has sections)
                                if let Some(ref pub_) = *selected_publication.read() {
                                    if !pub_.section_addresses.is_empty() {
                                        div {
                                            h3 {
                                                class: "text-sm font-medium mb-2",
                                                "Chapter/Section (optional)"
                                            }
                                            select {
                                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50",
                                                value: selected_chapter.read().clone().unwrap_or_default(),
                                                onchange: move |e| {
                                                    let val = e.value();
                                                    if val.is_empty() {
                                                        selected_chapter.set(None);
                                                    } else {
                                                        handle_chapter_click(val);
                                                    }
                                                },

                                                option {
                                                    value: "",
                                                    "Entire book"
                                                }
                                                for (idx, section_ref) in pub_.section_addresses.iter().enumerate() {
                                                    {
                                                        // Extract d-tag from address (kind:pubkey:d-tag format)
                                                        // Use splitn(3, ':') to handle d-tags that contain colons
                                                        let d_tag = section_ref.address
                                                            .splitn(3, ':')
                                                            .nth(2)
                                                            .unwrap_or(&section_ref.address);
                                                        // Use d-tag as the chapter identifier for proper referencing
                                                        // Display a user-friendly label with index for ordering
                                                        let display_label = format!("{}. {}", idx + 1, d_tag);
                                                        rsx! {
                                                            option {
                                                                key: "{section_ref.address}",
                                                                value: "{d_tag}",
                                                                "{display_label}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Section range input with validation (Security Fix #1)
                                div {
                                    h3 {
                                        class: "text-sm font-medium mb-2",
                                        "Verse/Paragraph Range (optional)"
                                    }
                                    input {
                                        r#type: "text",
                                        class: if *sections_error.read() {
                                            "w-full px-3 py-2 bg-background border-2 border-red-500 rounded-lg focus:outline-none focus:ring-2 focus:ring-red-500/50"
                                        } else {
                                            "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50"
                                        },
                                        placeholder: "e.g., 4-9 or 1,3,5",
                                        value: "{selected_sections}",
                                        oninput: move |e| selected_sections.set(e.value()),
                                    }
                                    if *sections_error.read() {
                                        p {
                                            class: "text-xs text-red-600 dark:text-red-400 mt-1",
                                            "Only numbers, commas, and hyphens allowed"
                                        }
                                    } else {
                                        p {
                                            class: "text-xs text-muted-foreground mt-1",
                                            "Use ranges (4-9) or comma-separated (1,3,5)"
                                        }
                                    }
                                }

                                // Version input with validation (Security Fix #1)
                                div {
                                    h3 {
                                        class: "text-sm font-medium mb-2",
                                        "Version/Edition (optional)"
                                    }
                                    input {
                                        r#type: "text",
                                        class: if *version_error.read() {
                                            "w-full px-3 py-2 bg-background border-2 border-red-500 rounded-lg focus:outline-none focus:ring-2 focus:ring-red-500/50"
                                        } else {
                                            "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary/50"
                                        },
                                        placeholder: "e.g., kjv, 1st-edition",
                                        value: "{selected_version}",
                                        oninput: move |e| selected_version.set(e.value()),
                                    }
                                    if *version_error.read() {
                                        p {
                                            class: "text-xs text-red-600 dark:text-red-400 mt-1",
                                            "Only lowercase letters, numbers, and hyphens allowed"
                                        }
                                    }
                                }

                                // Markup preview
                                div {
                                    class: "mt-4",
                                    h3 {
                                        class: "text-sm font-medium mb-2",
                                        "Markup Preview"
                                    }
                                    div {
                                        class: "p-3 bg-muted rounded-lg text-sm font-mono break-all",
                                        "{markup_preview}"
                                    }
                                }

                                // Display preview
                                if let Some(ref reference) = *book_reference.read() {
                                    div {
                                        class: "mt-2",
                                        h3 {
                                            class: "text-sm font-medium mb-2",
                                            "Display Preview"
                                        }
                                        div {
                                            class: "p-3 bg-muted rounded-lg text-sm",
                                            "{reference.display_text()}"
                                        }
                                    }
                                }
                            }

                            // Insert button (disabled when validation errors - Security Fix #1)
                            button {
                                class: "w-full mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *has_validation_error.read(),
                                onclick: handle_insert,
                                "Insert Book Reference"
                            }
                        } else {
                            div {
                                class: "flex-1 flex items-center justify-center text-center text-muted-foreground",
                                div {
                                    BookOpenIcon { class: "w-12 h-12 mx-auto mb-3 opacity-50".to_string() }
                                    p { "Select a publication from the list" }
                                    p {
                                        class: "text-sm mt-1",
                                        "Then customize your book reference"
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
