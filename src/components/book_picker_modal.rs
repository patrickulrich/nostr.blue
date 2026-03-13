//! Book Picker Modal
//! Select publications and book references to insert into wiki pages and publications
use crate::components::icons::{BookOpenIcon, ChevronDownIcon, SearchIcon, XIcon};
use crate::stores::publication_store::{
    fetch_publications, get_all_cached_publications, has_cached_publications_snapshot,
    search_publications, PublicationIndex,
};
use crate::utils::nkbip08::BookReference;
use dioxus::prelude::*;
use dioxus_core::Task;
/// Validate book identifier (d-tag or chapter) against NKBIP-08 format.
/// Only lowercase letters, digits, and hyphens are allowed.
fn is_valid_book_id(input: &str) -> bool {
    !input.is_empty()
        && input
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
/// Validate book version input against allowed format.
/// Only lowercase letters, digits, and hyphens are allowed (per NKBIP-08).
fn is_valid_book_version(input: &str) -> bool {
    !input.is_empty()
        && input
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
/// Validate book sections input against allowed format (per NKBIP-08 spec).
/// Only digits, commas, and hyphens are allowed (e.g., "1-5,7,10-12").
/// NO SPACES allowed - spec requires strict format compliance.
/// Also validates semantic correctness: no empty segments, valid ranges.
fn is_valid_book_sections(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    if !input
        .chars()
        .all(|c| c.is_ascii_digit() || c == ',' || c == '-')
    {
        return false;
    }
    for segment in input.split(',') {
        if segment.is_empty() {
            return false;
        }
        if let Some((start, end)) = segment.split_once('-') {
            let start_num: u32 = match start.parse() {
                Ok(n) if n > 0 => n,
                _ => return false,
            };
            let end_num: u32 = match end.parse() {
                Ok(n) if n > 0 => n,
                _ => return false,
            };
            if start_num > end_num {
                return false;
            }
        } else {
            match segment.parse::<u32>() {
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
    if url.len() > MAX_URL_LENGTH {
        return false;
    }
    let url_lower = url.to_lowercase();
    if !url_lower.starts_with("https://") {
        return false;
    }
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
    let mut debounce_counter = use_signal(|| 0u32);
    let mut search_task: Signal<Option<Task>> = use_signal(|| None);
    let mut selected_publication = use_signal(|| None::<PublicationIndex>);
    let mut selected_chapter = use_signal(|| None::<String>);
    let mut selected_sections = use_signal(String::new);
    let mut selected_version = use_signal(String::new);
    let mut book_id_error = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut fetch_error = use_signal(|| None::<String>);
    let mut publications_version = use_signal(|| 0usize);
    let mut fetch_generation = use_signal(|| 0u64);
    let mut fetch_task: Signal<Option<Task>> = use_signal(|| None);
    use_effect(use_reactive(&*props.show.read(), move |is_shown| {
        if is_shown {
            let has_cached_publications = has_cached_publications_snapshot();
            // Increment generation token to invalidate any in-flight requests
            let current_generation = fetch_generation.peek().wrapping_add(1);
            fetch_generation.set(current_generation);
            if let Some(task) = fetch_task.take() {
                task.cancel();
            }

            loading.set(!has_cached_publications);
            fetch_error.set(None);
            let task = spawn(async move {
                let result = fetch_publications(100, None).await;

                // Check staleness before state updates
                if *fetch_generation.peek() != current_generation {
                    return;
                }

                match result {
                    Ok(_) => {
                        fetch_error.set(None);
                        publications_version.set(publications_version().wrapping_add(1));
                    }
                    Err(e) => {
                        crate::utils::log_fetch_error("publications", e.clone());
                        if !has_cached_publications_snapshot() {
                            fetch_error.set(Some(format!("Failed to load publications: {}", e)));
                        }
                    }
                }
                loading.set(false);
            });
            fetch_task.set(Some(task));
            selected_publication.set(None);
            selected_chapter.set(None);
            selected_sections.set(String::new());
            selected_version.set(String::new());
            book_id_error.set(false);
            search_query.set(String::new());
            search_results.set(Vec::new());
            is_searching.set(false);
            if let Some(task) = search_task.take() {
                task.cancel();
            }
            debounce_counter.set(0);
        } else if let Some(task) = fetch_task.take() {
            task.cancel();
            if let Some(search_task) = search_task.take() {
                search_task.cancel();
            }
        }
    }));
    let mut handle_search = move |query: String| {
        search_query.set(query.clone());
        if query.is_empty() {
            debounce_counter.set(debounce_counter().wrapping_add(1));
            if let Some(search_task) = search_task.take() {
                search_task.cancel();
            }
            search_results.set(Vec::new());
            is_searching.set(false);
            let should_clear = fetch_error
                .peek()
                .as_ref()
                .map(|err| err.starts_with("Search failed:"))
                .unwrap_or(false);
            if should_clear {
                fetch_error.set(None);
            }
            return;
        }
        debounce_counter.set(debounce_counter().wrapping_add(1));
        let current_counter = debounce_counter();
        is_searching.set(true);
        if let Some(task) = search_task.take() {
            task.cancel();
        }
        let new_task = spawn(async move {
            crate::platform::timer::sleep_ms(300).await;
            if debounce_counter() != current_counter {
                return;
            }
            match search_publications(&query, 50).await {
                Ok(results) => {
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
            if debounce_counter() == current_counter {
                is_searching.set(false);
            }
        });
        search_task.set(Some(new_task));
    };
    let publications_to_display = use_memo(move || {
        let _ = *publications_version.read();
        if *active_tab.read() == BookPickerTab::Search && !search_query.read().is_empty() {
            search_results.read().clone()
        } else {
            get_all_cached_publications()
        }
    });
    // Validate selected_publication d_tag in an effect (side effects don't belong in memos)
    // Track selected_chapter to re-validate when "Entire book" is selected
    use_effect(move || {
        let _ = selected_chapter.read(); // Establish dependency on chapter changes
        if let Some(pub_) = selected_publication.read().as_ref() {
            book_id_error.set(!is_valid_book_id(&pub_.d_tag));
        }
    });
    let version_has_error = use_memo(move || {
        let version = selected_version.read();
        !version.is_empty() && !is_valid_book_version(&version)
    });
    let sections_has_error = use_memo(move || {
        let sections = selected_sections.read();
        !sections.is_empty() && !is_valid_book_sections(&sections)
    });
    let book_reference = use_memo(move || {
        selected_publication.read().as_ref().and_then(|pub_| {
            if !is_valid_book_id(&pub_.d_tag) {
                log::warn!("Invalid publication d_tag: {}", pub_.d_tag);
                return None;
            }
            let mut reference = BookReference::new(&pub_.d_tag);
            let version_input = selected_version.read();
            if !version_input.is_empty() && is_valid_book_version(&version_input) {
                reference = reference.with_version(&version_input);
            }
            if let Some(ref chapter) = *selected_chapter.read() {
                if is_valid_book_id(chapter) {
                    reference = reference.with_chapter(chapter);
                } else {
                    log::warn!("Invalid chapter id: {}", chapter);
                }
            }
            let sections_str = selected_sections.read();
            if !sections_str.is_empty() && is_valid_book_sections(&sections_str) {
                for section in sections_str.split(',') {
                    if !section.is_empty() {
                        reference = reference.with_section(section);
                    }
                }
            }
            Some(reference)
        })
    });
    let has_validation_error = use_memo(move || {
        *version_has_error.read() || *sections_has_error.read() || *book_id_error.read()
    });
    let markup_preview = use_memo(move || {
        book_reference
            .read()
            .as_ref()
            .map(|r| r.raw.clone())
            .unwrap_or_default()
    });
    let close_modal = move |_| {
        if *is_searching.read() {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("A search is in progress. Close anyway?")
                            .ok()
                    })
                    .unwrap_or(false);
                if !confirmed {
                    return;
                }
            }
        }
        props.show.set(false);
    };
    let mut handle_publication_click = move |publication: PublicationIndex| {
        selected_publication.set(Some(publication));
        selected_chapter.set(None);
        selected_sections.set(String::new());
        selected_version.set(String::new());
    };
    let mut handle_chapter_click = move |chapter: String| {
        if is_valid_book_id(&chapter) {
            selected_chapter.set(Some(chapter));
        } else {
            log::warn!("Invalid chapter id: {}", chapter);
            selected_chapter.set(None);
        }
    };
    let handle_insert = move |_| {
        if *has_validation_error.read() {
            return;
        }
        if let Some(reference) = book_reference.read().clone() {
            let markup = reference.raw.clone();
            props.on_select.call(BookSelection { reference, markup });
            props.show.set(false);
        }
    };
    if !*props.show.read() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: close_modal,
            div {
                class: "bg-background border border-border rounded-xl shadow-xl w-full max-w-3xl max-h-[85vh] overflow-hidden flex flex-col",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "book-picker-title",
                tabindex: "-1",
                onmounted: move |_evt| {
                    #[cfg(feature = "web")]
                    {
                        if let Some(html_element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                },
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        if *is_searching.read() {
                            #[cfg(feature = "web")]
                            {
                                let confirmed = web_sys::window()
                                    .and_then(|w| {
                                        w.confirm_with_message("A search is in progress. Close anyway?")
                                            .ok()
                                    })
                                    .unwrap_or(false);
                                if !confirmed {
                                    return;
                                }
                            }
                        }
                        props.show.set(false);
                    }
                },
                div { class: "flex items-center justify-between px-6 py-4 border-b border-border",
                    div { class: "flex items-center gap-2",
                        BookOpenIcon { class: "w-5 h-5 text-primary".to_string() }
                        h2 {
                            id: "book-picker-title",
                            class: "text-lg font-semibold",
                            "Insert Book Reference"
                        }
                    }
                    button {
                        class: "p-2 text-muted-foreground hover:text-foreground rounded-lg hover:bg-accent transition-colors",
                        r#type: "button",
                        onclick: close_modal,
                        aria_label: "Close",
                        title: "Close",
                        XIcon { class: "w-5 h-5".to_string() }
                    }
                }
                div { class: "px-6 py-3 border-b border-border space-y-3",
                    div { class: "flex gap-2",
                        button {
                            class: if *active_tab.read() == BookPickerTab::Browse { "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground" } else { "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors" },
                            onclick: move |_| {
                                active_tab.set(BookPickerTab::Browse);
                                debounce_counter.set(debounce_counter().wrapping_add(1));
                                if let Some(search_task) = search_task.take() {
                                    search_task.cancel();
                                }
                                search_query.set(String::new());
                                search_results.set(Vec::new());
                                is_searching.set(false);
                                fetch_error.set(None);
                            },
                            "Browse"
                        }
                        button {
                            class: if *active_tab.read() == BookPickerTab::Search { "px-3 py-1.5 text-sm font-medium rounded-lg bg-primary text-primary-foreground" } else { "px-3 py-1.5 text-sm font-medium rounded-lg text-muted-foreground hover:bg-accent transition-colors" },
                            onclick: move |_| active_tab.set(BookPickerTab::Search),
                            "Search"
                        }
                    }
                    if *active_tab.read() == BookPickerTab::Search {
                        div { class: "relative",
                            div { class: "absolute inset-y-0 left-3 flex items-center pointer-events-none",
                                SearchIcon { class: "w-4 h-4 text-muted-foreground".to_string() }
                            }
                            input {
                                r#type: "text",
                                class: "w-full pl-10 pr-4 py-2 bg-muted/50 border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                placeholder: "Search by title, author...",
                                value: "{search_query}",
                                oninput: move |e| handle_search(e.value()),
                            }
                        }
                    }
                }
                div { class: "flex-1 overflow-hidden flex flex-col md:flex-row",
                    div { class: "flex-1 overflow-y-auto p-4 border-b md:border-b-0 md:border-r border-border",
                        if *loading.read() || *is_searching.read() {
                            div { class: "flex items-center justify-center py-8",
                                div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary" }
                            }
                        } else if let Some(ref err) = *fetch_error.read() {
                            div { class: "text-center py-8",
                                p { class: "text-red-600 dark:text-red-400", "{err}" }
                                p { class: "text-sm mt-2 text-muted-foreground",
                                    "Check your connection and try again"
                                }
                            }
                        } else if publications_to_display.read().is_empty() {
                            div { class: "text-center py-8 text-muted-foreground",
                                p { "No publications found" }
                                p { class: "text-sm mt-2",
                                    "Try searching or check your relay connections"
                                }
                            }
                        } else {
                            div { class: "space-y-2",
                                for publication in publications_to_display.read().iter() {
                                    {
                                        let pub_clone = publication.clone();
                                        let is_selected = selected_publication
                                            .read()
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
                                                    },
                                                ),
                                                onclick: move |_| handle_publication_click(pub_clone.clone()),
                                                div { class: "flex items-start gap-3",
                                                    div { class: "w-12 h-16 shrink-0 rounded bg-muted flex items-center justify-center",
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
                                                    div { class: "flex-1 min-w-0",
                                                        p { class: "font-medium text-foreground truncate", "{publication.title}" }
                                                        if let Some(ref author) = publication.author {
                                                            p { class: "text-sm text-muted-foreground truncate", "by {author}" }
                                                        }
                                                        p { class: "text-xs text-muted-foreground mt-1",
                                                            "{publication.section_addresses.len()} sections"
                                                        }
                                                    }
                                                    if is_selected {
                                                        ChevronDownIcon { class: "w-5 h-5 text-primary shrink-0 -rotate-90".to_string() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "w-full md:w-80 p-4 bg-muted/30 flex flex-col",
                        if selected_publication.read().is_some() {
                            div { class: "space-y-4 flex-1",
                                if *book_id_error.read() {
                                    p { class: "text-xs text-destructive",
                                        "Selected publication has an invalid identifier"
                                    }
                                }
                                if let Some(ref pub_) = *selected_publication.read() {
                                    if !pub_.section_addresses.is_empty() {
                                        div {
                                            h3 { class: "text-sm font-medium mb-2",
                                                "Chapter/Section (optional)"
                                            }
                                            select {
                                                class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50",
                                                value: selected_chapter.read().clone().unwrap_or_default(),
                                                onchange: move |e| {
                                                    let val = e.value();
                                                    if val.is_empty() {
                                                        selected_chapter.set(None);
                                                    } else {
                                                        handle_chapter_click(val);
                                                    }
                                                },
                                                option { value: "", "Entire book" }
                                                for (idx , section_ref) in pub_.section_addresses.iter().enumerate() {
                                                    if let Some(d_tag) = section_ref.address.splitn(3, ':').nth(2) {
                                                        if !d_tag.is_empty() && is_valid_book_id(d_tag) {
                                                            {
                                                                let display_label = format!("{}. {}", idx + 1, d_tag);
                                                                rsx! {
                                                                    option { key: "{section_ref.address}", value: "{d_tag}", "{display_label}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                div {
                                    h3 { class: "text-sm font-medium mb-2",
                                        "Verse/Paragraph Range (optional)"
                                    }
                                    input {
                                        r#type: "text",
                                        class: if *sections_has_error.read() { "w-full px-3 py-2 bg-background border-2 border-red-500 rounded-lg focus:outline-hidden focus:ring-2 focus:ring-red-500/50" } else { "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50" },
                                        placeholder: "e.g., 4-9 or 1,3,5",
                                        value: "{selected_sections}",
                                        oninput: move |e| selected_sections.set(e.value()),
                                    }
                                    if *sections_has_error.read() {
                                        p { class: "text-xs text-red-600 dark:text-red-400 mt-1",
                                            "Only digits, commas, and hyphens allowed (no spaces)"
                                        }
                                    } else {
                                        p { class: "text-xs text-muted-foreground mt-1",
                                            "Use ranges (4-9) or comma-separated (1,3,5)"
                                        }
                                    }
                                }
                                div {
                                    h3 { class: "text-sm font-medium mb-2",
                                        "Version/Edition (optional)"
                                    }
                                    input {
                                        r#type: "text",
                                        class: if *version_has_error.read() { "w-full px-3 py-2 bg-background border-2 border-red-500 rounded-lg focus:outline-hidden focus:ring-2 focus:ring-red-500/50" } else { "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary/50" },
                                        placeholder: "e.g., kjv, 1st-edition",
                                        value: "{selected_version}",
                                        oninput: move |e| selected_version.set(e.value()),
                                    }
                                    if *version_has_error.read() {
                                        p { class: "text-xs text-red-600 dark:text-red-400 mt-1",
                                            "Only lowercase letters, numbers, and hyphens allowed"
                                        }
                                    }
                                }
                                div { class: "mt-4",
                                    h3 { class: "text-sm font-medium mb-2", "Markup Preview" }
                                    div { class: "p-3 bg-muted rounded-lg text-sm font-mono break-all",
                                        "{markup_preview}"
                                    }
                                }
                                if let Some(ref reference) = *book_reference.read() {
                                    div { class: "mt-2",
                                        h3 { class: "text-sm font-medium mb-2", "Display Preview" }
                                        div { class: "p-3 bg-muted rounded-lg text-sm",
                                            "{reference.display_text()}"
                                        }
                                    }
                                }
                            }
                            button {
                                class: "w-full mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *has_validation_error.read() || book_reference.read().is_none(),
                                onclick: handle_insert,
                                "Insert Book Reference"
                            }
                        } else {
                            div { class: "flex-1 flex items-center justify-center text-center text-muted-foreground",
                                div {
                                    BookOpenIcon { class: "w-12 h-12 mx-auto mb-3 opacity-50".to_string() }
                                    p { "Select a publication from the list" }
                                    p { class: "text-sm mt-1", "Then customize your book reference" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
